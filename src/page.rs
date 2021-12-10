pub(crate) mod cli;
pub(crate) mod neovim;
pub(crate) mod context;


#[tokio::main(worker_threads=2)]
async fn main() {
    _init_logger_();
    let env_ctx = crate::context::gather_env::enter();
    prefetch_lines(env_ctx).await;
}

fn _init_logger_() {
    let rust_log = std::env::var("RUST_LOG");
    let level = rust_log.as_deref().unwrap_or("warn");
    let level_filter: log::LevelFilter = std::str::FromStr::from_str(level).expect("Cannot parse $RUST_LOG value");
    let exec_time = std::time::Instant::now();
    fern::Dispatch::new()
        .format(move |cb, msg, record| {
            let (module, target) = (record.module_path().unwrap_or_default(), record.target());
            let (target, separator, module) = if module == target {
                (target, "", "")
            } else {
                (target, " in ", module)
            };
            let color = if module.starts_with("page") { "" } else { "\x1B[0;90m" }; // grayscale
            cb.finish(
                format_args!(
                    "\x1B[1m\x1B[4m[ {:010} | {importancy:5} | {target}{separator}{module} ]\x1B[0m\n{color}{message}\x1B[0m\n",
                    exec_time.elapsed().as_micros(),
                    importancy = record.level(),
                    target = target,
                    separator = separator,
                    module = module,
                    color = color,
                    message = msg,
                )
            )
        })
        .level(level_filter)
        .chain(std::io::stderr())
        .apply()
        .expect("Cannot initialize logger");
}


async fn prefetch_lines(env_ctx: context::EnvContext) {
    log::info!(target: "context", "{:#?}", &env_ctx);
    let mut prefetched_lines = Vec::with_capacity(env_ctx.echo_lines);
    while env_ctx.echo_lines > prefetched_lines.len() {
        let mut line = String::new();
        let remain = std::io::stdin().read_line(&mut line).expect("Failed to prefetch line from stdin");
        prefetched_lines.push(line);
        if remain == 0usize {
            break
        }
    }
    if env_ctx.echo_lines - prefetched_lines.len() > 0usize {
        _dump_prefetched_lines_and_exit_(prefetched_lines, &env_ctx.opt.output.filetype)
    }
    _warn_incompatible_options_(&env_ctx);
    let cli_ctx = context::check_usage::enter(prefetched_lines, env_ctx);
    connect_neovim(cli_ctx).await;
}

fn _dump_prefetched_lines_and_exit_(lines: Vec<String>, filetype: &str) -> ! {
    log::info!(target: "dump", "{}: {}", filetype, lines.len());
    use std::{io, process};
    let (stdout, mut stdout_lock);
    let mut bat_proc = None;
    let output: &mut dyn io::Write = {
        if !filetype.is_empty() && filetype != "pager" {
            match process::Command::new("bat")
                .arg("--plain")
                .arg("--paging=never")
                .arg("--color=always")
                .arg(&format!("--language={}", filetype))
                .stdin(process::Stdio::piped())
                .spawn()
            {
                Ok(proc) => {
                    log::info!(target: "dump", "use bat");
                    bat_proc.get_or_insert(proc).stdin.as_mut().expect("Cannot get bat stdin")
                }
                Err(e) => {
                    log::warn!(target: "dump", "cannot spawn bat, use stdout: {:?}", e);
                    (stdout = io::stdout(), stdout_lock = stdout.lock());
                    &mut stdout_lock
                }
            }
        } else {
            log::info!(target: "dump", "use stdout");
            (stdout = io::stdout(), stdout_lock = stdout.lock());
            &mut stdout_lock
        }
    };
    for ln in lines {
        io::Write::write(output, ln.as_bytes()).expect("Cannot write line");
    }
    output.flush().expect("Cannot flush");
    if let Some(mut proc) = bat_proc {
        proc.wait().expect("bat process ended unexpectedly");
    }
    process::exit(0)
}

fn _warn_incompatible_options_(opt_ctx: &context::EnvContext) {
    if opt_ctx.is_inst_close_flag_given_without_address() {
        log::warn!(target: "usage", "Instance close (-x) is ignored if address (-a or $NVIM_LISTEN_ADDRESS) isn't set");
    }
    if opt_ctx.is_split_flag_given_without_address() {
        log::warn!(target: "usage", "Split (-r -l -u -d -R -L -U -D) is ignored if address (-a or $NVIM_LISTEN_ADDRESS) isn't set");
    }
    if opt_ctx.is_back_flag_given_without_address() {
        log::warn!(target: "usage", "Switch back (-b -B) is ignored if address (-a or $NVIM_LISTEN_ADDRESS) isn't set");
    }
    if opt_ctx.is_query_flag_given_without_reading_from_pipe() {
        log::warn!(target: "usage", "Query (-q) is ignored when page doesn't read input from pipe");
    }
}

async fn connect_neovim(cli_ctx: context::UsageContext) {
    log::info!(target: "context", "{:#?}", &cli_ctx);
    _init_panic_hook_();
    let mut nvim_conn = neovim::connection::open(&cli_ctx).await;
    let nvim_ctx = if let Some(_) = nvim_conn.nvim_proc {
        context::connect_neovim::enter(cli_ctx).with_child_neovim_process_spawned()
    } else {
        context::connect_neovim::enter(cli_ctx)
    };
    manage_page_state(&mut nvim_conn, nvim_ctx).await;
    neovim::connection::close_and_exit(&mut nvim_conn).await;
}

fn _init_panic_hook_() {
    use std::{io, panic, process};
    let default_panic_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        // If neovim died unexpectedly then it messes terminal, so we must reset it
        let reset = process::Command::new("reset").spawn()
            .and_then(|mut child| child.wait())
            .and_then(|exit_code| exit_code.success()
                .then(|| ()).ok_or_else(|| io::Error::new(io::ErrorKind::Other, format!("Reset exited with status: {}", exit_code))));
        if let Err(e) = reset {
            log::error!(target: "termreset", "Cannot reset terminal: {:?}", e);
        }
        default_panic_hook(info);
    }));
}

async fn manage_page_state(nvim_conn: &mut neovim::NeovimConnection, nvim_ctx: context::NeovimContext) {
    log::info!(target: "context", "{:#?}", &nvim_ctx);
    let mut api_actions = neovim_api_usage::begin(nvim_conn, &nvim_ctx);
    api_actions.close_page_instance_buffer().await;
    api_actions.display_files().await;
    if nvim_ctx.outp_buf_usage.is_disabled() {
        return
    }
    if let Some(inst_name) = nvim_ctx.inst_usage.is_enabled() {
        if let Some((buf, buf_pty_path)) = api_actions.find_instance_buffer(inst_name).await {
            let outp_ctx = context::output_buffer_available::enter(nvim_ctx, buf_pty_path);
            manage_output_buffer(nvim_conn, buf, outp_ctx).await
        } else {
            let (buf, buf_pty_path) = api_actions.create_instance_output_buffer(inst_name).await;
            let outp_ctx = context::output_buffer_available::enter(nvim_ctx, buf_pty_path).with_new_instance_output_buffer();
            manage_output_buffer(nvim_conn, buf, outp_ctx).await
        }
    } else {
        let (buf, buf_pty_path) = api_actions.create_oneoff_output_buffer().await;
        let outp_ctx = context::output_buffer_available::enter(nvim_ctx, buf_pty_path);
        manage_output_buffer(nvim_conn, buf, outp_ctx).await
    };
}


use crate::neovim::{Buffer, IoWrite};
async fn manage_output_buffer(nvim_conn: &mut neovim::NeovimConnection, buf: Buffer<IoWrite>, outp_ctx: context::OutputContext) {
    log::info!(target: "context", "{:#?}", &outp_ctx);
    let mut outp_buf_actions = output_buffer_usage::begin(nvim_conn, &outp_ctx, buf);
    if let Some(inst_name) = outp_ctx.inst_usage.is_enabled() {
        outp_buf_actions.update_instance_buffer_title(inst_name).await;
        outp_buf_actions.focus_on_instance_buffer().await;
    } else {
        outp_buf_actions.update_buffer_title().await;
    }
    outp_buf_actions.execute_commands().await;
    outp_buf_actions.focus_on_initial_buffer().await;
    outp_buf_actions.handle_output().await;
    outp_buf_actions.execute_disconnect_commands().await;
}



mod neovim_api_usage {
    use crate::{context::NeovimContext, neovim::{Buffer, IoWrite, NeovimConnection, OutputCommands}};

    type BufferAndPty = (Buffer<IoWrite>, std::path::PathBuf);

    /// This struct implements actions that should be done before output buffer is available
    pub struct ApiActions<'a> {
        nvim_conn: &'a mut NeovimConnection,
        nvim_ctx: &'a NeovimContext,
    }

    pub fn begin<'a>(nvim_conn: &'a mut NeovimConnection, nvim_ctx: &'a NeovimContext) -> ApiActions<'a> {
        ApiActions {
            nvim_conn,
            nvim_ctx,
        }
    }

    impl<'a> ApiActions<'a> {
        /// Closes buffer marked as instance, when mark is provided by -x argument
        pub async fn close_page_instance_buffer(&mut self) {
            if let Some(ref instance) = self.nvim_ctx.opt.instance_close {
                self.nvim_conn.nvim_actions.close_instance_buffer(instance).await
            }
        }

        /// Opens each file provided as free arguments in separate buffers.
        /// Resets focus to initial buffer and window if further there will be created output buffer in split window,
        /// since we want to see shell from which that output buffer was spawned
        pub async fn display_files(&mut self) {
            let ApiActions { nvim_conn: NeovimConnection { nvim_actions, initial_buf_number, initial_win_and_buf, .. }, nvim_ctx } = self;
            for f in &nvim_ctx.opt.files {
                if let Err(e) = nvim_actions.open_file_buffer(f).await {
                    log::warn!(target: "page file", "Error opening \"{}\": {}", f, e);
                } else {
                    let cmd_provided_by_user = &nvim_ctx.opt.output.command.as_deref().unwrap_or_default();
                    let file_buf_opts = OutputCommands::for_file_buffer(cmd_provided_by_user, nvim_ctx.opt.output.writable);
                    nvim_actions.prepare_output_buffer(*initial_buf_number, file_buf_opts).await;
                    if nvim_ctx.opt.follow_all {
                        nvim_actions.set_current_buffer_follow_output_mode().await;
                    } else {
                        nvim_actions.set_current_buffer_scroll_mode().await;
                    }
                }
            }
            if nvim_ctx.is_split_flag_given_with_files() {
                nvim_actions.switch_to_window_and_buffer(&initial_win_and_buf).await
            }
        }

        /// Returns buffer marked as instance and path to PTY device associated with it (if some exists)
        pub async fn find_instance_buffer(&mut self, inst_name: &str) -> Option<BufferAndPty> {
            self.nvim_conn.nvim_actions.find_instance_buffer(inst_name).await
        }

        /// Creates a new output buffer and then marks it as instance buffer
        pub async fn create_instance_output_buffer(&mut self, inst_name: &str) -> BufferAndPty {
            let (buf, buf_pty_path) = self.create_oneoff_output_buffer().await;
            self.nvim_conn.nvim_actions.mark_buffer_as_instance(&buf, inst_name, &buf_pty_path.to_string_lossy()).await;
            (buf, buf_pty_path)
        }

        /// Creates a new output buffer using split window if required.
        /// Also sets some nvim options for better reading experience
        pub async fn create_oneoff_output_buffer(&mut self) -> BufferAndPty {
            let ApiActions { nvim_conn: NeovimConnection { nvim_actions, initial_buf_number, channel, nvim_proc, .. }, nvim_ctx } = self;
            let buf_and_pty = if nvim_proc.is_some() {
                nvim_actions.create_replacing_output_buffer().await
            } else if nvim_ctx.outp_buf_usage.is_create_split() {
                nvim_actions.create_split_output_buffer(&nvim_ctx.opt.output.split).await
            } else {
                nvim_actions.create_switching_output_buffer().await
            };
            let outp_buf_opts = OutputCommands::for_output_buffer(&nvim_ctx.page_id, *channel, &nvim_ctx.opt.output);
            nvim_actions.prepare_output_buffer(*initial_buf_number, outp_buf_opts).await;
            buf_and_pty
        }
    }
}

mod output_buffer_usage {
    use crate::{context::OutputContext, neovim::{self, Buffer, IoWrite, NeovimConnection, NotificationFromNeovim}};

    use tokio::io::AsyncWriteExt;
    use std::io::{self, BufRead};

    /// This struct implements actions that should be done after output buffer is attached
    pub struct BufferActions<'a> {
        nvim_conn: &'a mut NeovimConnection,
        outp_ctx: &'a OutputContext,
        buf: Buffer<IoWrite>,
        buf_pty: Option<tokio::fs::File>,
    }

    pub fn begin<'a>(nvim_conn: &'a mut NeovimConnection, outp_ctx: &'a OutputContext, buf: Buffer<IoWrite>) -> BufferActions<'a> {
        BufferActions {
            nvim_conn,
            outp_ctx,
            buf,
            buf_pty: None,
        }
    }

    impl<'a> BufferActions<'a> {
        /// This function updates buffer title depending on -n value.
        /// Icon symbol is received from neovim side and is prepended to the left of buffer title
        pub async fn update_buffer_title(&mut self) {
            let BufferActions { outp_ctx, buf, nvim_conn: NeovimConnection { nvim_actions, .. }, .. } = self;
            let (page_icon_default, page_icon_key) = if outp_ctx.input_from_pipe { (" |", "page_icon_pipe") } else { (" >", "page_icon_redirect") };
            let mut buf_title = nvim_actions.get_var_or(page_icon_key, page_icon_default).await;
            if let Some(ref buf_name) = outp_ctx.opt.name {
                buf_title.insert_str(0, buf_name);
            }
            nvim_actions.update_buffer_title(&buf, &buf_title).await;
        }

        /// This function updates instance buffer title depending on its name and -n value.
        /// Instance name will be prepended to the left of the icon symbol.
        pub async fn update_instance_buffer_title(&mut self, inst_name: &str) {
            let BufferActions { outp_ctx, buf, nvim_conn: NeovimConnection { nvim_actions, .. }, .. } = self;
            let (page_icon_key, page_icon_default) = ("page_icon_instance", "@ ");
            let mut buf_title = nvim_actions.get_var_or(page_icon_key, page_icon_default).await;
            buf_title.insert_str(0, inst_name);
            if let Some(ref buf_name) = outp_ctx.opt.name {
                if buf_name != inst_name {
                    buf_title.push_str(buf_name);
                }
            }
            nvim_actions.update_buffer_title(&buf, &buf_title).await;
        }

        /// Resets instance buffer focus and content.
        /// This is required to provide some functionality not available through neovim API
        pub async fn focus_on_instance_buffer(&mut self) {
            let BufferActions { outp_ctx, buf, nvim_conn: NeovimConnection { nvim_actions, .. }, .. } = self;
            if outp_ctx.inst_usage.is_enabled_and_should_be_focused() {
                nvim_actions.focus_instance_buffer(&buf).await;
                if outp_ctx.inst_usage.is_enabled_and_should_replace_its_content() {
                    self.get_buffer_pty().await.write_all(b"\x1B[3J\x1B[H\x1b[2J").await.expect("Cannot write clear screen sequence");
                }
            }
        }

        /// Executes PageConnect (-C) and post command (-E) on page buffer.
        /// If any of these flags are passed then output buffer should be already focused
        pub async fn execute_commands(&mut self) {
            let BufferActions { outp_ctx, nvim_conn: NeovimConnection { nvim_actions, .. }, .. } = self;
            if outp_ctx.opt.command_auto {
                nvim_actions.execute_connect_autocmd_on_current_buffer().await;
            }
            if let Some(ref command) = outp_ctx.opt.command_post {
                nvim_actions.execute_command_post(&command).await;
            }
        }

        /// Sets cursor position on page buffer and on current buffer depending on -f, -b, and -B flags provided.
        /// First if condition on this function ensures that it's really necessary to do any action,
        /// to circumvent flicker with `page -I existed -b` and `page -I existed -B` invocations
        pub async fn focus_on_initial_buffer(&mut self) {
            let BufferActions { outp_ctx, nvim_conn: NeovimConnection { nvim_actions, initial_win_and_buf, .. }, .. } = self;
            if outp_ctx.inst_usage.is_enabled_but_should_be_unfocused() {
                return
            }
            if outp_ctx.opt.follow {
                nvim_actions.set_current_buffer_follow_output_mode().await;
            } else {
                nvim_actions.set_current_buffer_scroll_mode().await;
            }
            if outp_ctx.restore_initial_buf_focus.is_disabled() {
                return
            }
            nvim_actions.switch_to_window_and_buffer(&initial_win_and_buf).await;
            if outp_ctx.restore_initial_buf_focus.is_vi_mode_insert() {
                nvim_actions.set_current_buffer_insert_mode().await;
            }
        }

        /// Writes lines from stdin directly into PTY device associated with output buffer.
        /// In case if -q <count> argument provided it might block until next line will be request from neovim side.
        /// If page isn't piped then it simply prints actual path to PTY device associated with output buffer,
        /// and user might redirect into it directly
        pub async fn handle_output(&mut self) {
            if self.outp_ctx.input_from_pipe {
                // First write all prefetched lines if any available
                for ln in self.outp_ctx.prefetched_lines.0.as_slice() {
                    self.display_line(ln.as_bytes()).await.expect("Cannot write next prefetched line");
                }
                // Then copy the rest of lines from stdin into buffer pty
                let stdin = std::io::stdin();
                let (mut buf, mut stdin_lines) = (String::with_capacity(4096), stdin.lock());
                if !self.outp_ctx.is_query_enabled() {
                    loop {
                        match stdin_lines.read_line(&mut buf) {
                            Ok(0) => return,
                            Ok(_) => {
                                self.display_line(buf.as_bytes()).await.expect("Write to PTY failed unexpectedly");
                                buf.clear();
                            }
                            Err(e) => {
                                log::warn!(target: "output", "Error reading line from stdin: {}", e);
                                return
                            }
                        }
                    }
                }
                // If query (-q) is enabled then wait for it
                let mut s = QueryState::default();
                s.next_part(self.outp_ctx.opt.output.query_lines);
                loop {
                    if s.is_whole_part_sent() {
                        self.nvim_conn.nvim_actions.notify_query_finished(s.how_many_lines_was_sent()).await;
                        match self.nvim_conn.rx.recv().await {
                            Some(NotificationFromNeovim::FetchLines(n)) => s.next_part(n),
                            Some(NotificationFromNeovim::FetchPart) => s.next_part(self.outp_ctx.opt.output.query_lines),
                            Some(NotificationFromNeovim::BufferClosed) => return,
                            None => {
                                log::info!(target: "output", "Neovim was closed, not all pages are shown");
                                neovim::connection::close_and_exit(self.nvim_conn).await
                            }
                        }
                    }
                    match stdin_lines.read_line(&mut buf) {
                        Ok(0) => {
                            self.nvim_conn.nvim_actions.notify_query_finished(s.how_many_lines_was_sent()).await;
                            break
                        }
                        Ok(_) => {
                            self.display_line(buf.as_bytes()).await.expect("Cannot write next line");
                            buf.clear()
                        }
                        Err(e) => {
                            log::warn!(target: "output", "Error reading line from stdin: {}", e);
                            break
                        }
                    }
                    s.line_sent();
                }
                self.nvim_conn.nvim_actions.notify_end_of_input().await;
            }
            if self.outp_ctx.print_output_buf_pty {
                println!("{}", self.outp_ctx.buf_pty_path.to_string_lossy());
            }
        }

        /// Writes line to PTY device and gracefully handles failures: if error occurs then page waits for
        /// "page_buffer_closed" notification that's sent on BufDelete event and signals that buffer was
        /// closed intentionally, so page must just exit. If no such notification was arrived then page
        /// crashes with the received IO error
        async fn display_line(&mut self, ln: &[u8]) -> io::Result<()> {
            if let Err(e) = self.get_buffer_pty().await.write_all(ln).await {
                log::info!(target: "writeline", "got error: {:?}", e);
                match tokio::time::timeout(std::time::Duration::from_secs(1), self.nvim_conn.rx.recv()).await {
                    Ok(Some(NotificationFromNeovim::BufferClosed)) => {
                        log::info!(target: "writeline", "Buffer was closed, not all input is shown");
                        neovim::connection::close_and_exit(self.nvim_conn).await
                    },
                    Ok(None) if self.nvim_conn.nvim_proc.is_some() => {
                        log::info!(target: "writeline", "Neovim was closed, not all input is shown");
                        neovim::connection::close_and_exit(self.nvim_conn).await
                    },
                    _ => return Err(e),
                }
            }
            Ok(())
        }

        /// Executes PageDisconnect autocommand if -C flag was provided.
        /// Some time might pass since page buffer was created and output was started,
        /// so this function might temporarily refocus on output buffer in order to run
        /// autocommand
        pub async fn execute_disconnect_commands(&mut self) {
            let BufferActions { nvim_conn: NeovimConnection { nvim_actions, initial_win_and_buf, .. }, buf, outp_ctx, .. } = self;
            if outp_ctx.opt.command_auto {
                let active_buf = nvim_actions.get_current_buffer().await.expect("Cannot get currently active buffer to execute PageDisconnect");
                let switched = buf != &active_buf;
                if switched {
                    nvim_actions.switch_to_buffer(&buf).await.expect("Cannot switch back to page buffer");
                }
                nvim_actions.execute_disconnect_autocmd_on_current_buffer().await;
                if switched {
                    if !active_buf.is_loaded().await.expect("Cannot check if buffer loaded") {
                        // Page buffer probably was closed in autocommand
                        return
                    }
                    nvim_actions.switch_to_buffer(&active_buf).await.expect("Cannot switch back to active buffer");
                    if initial_win_and_buf.1 == active_buf && outp_ctx.restore_initial_buf_focus.is_vi_mode_insert() {
                        nvim_actions.set_current_buffer_insert_mode().await;
                    }
                }
            }
        }

        /// Returns PTY device associated with output buffer.
        /// This function ensures that PTY device is opened only once
        async fn get_buffer_pty(&mut self) -> &mut tokio::fs::File {
            if let Some(ref mut pty) = self.buf_pty {
                pty
            } else {
                self.buf_pty.insert(tokio::fs::OpenOptions::new().append(true).open(&self.outp_ctx.buf_pty_path).await.expect("Cannot open PTY device"))
            }
        }
    }


    /// Encapsulates state of querying lines from neovim side with :Page <count> command.
    /// Used only when -q <count> argument is provided
    #[derive(Default)]
    struct QueryState {
        expect: usize,
        remain: usize,
    }

    impl QueryState {
        fn next_part(&mut self, lines_to_read: usize) {
            self.expect = lines_to_read;
            self.remain = lines_to_read;
        }
        fn line_sent(&mut self) {
            self.remain -= 1;
        }
        fn is_whole_part_sent(&self) -> bool {
            self.remain == 0
        }
        fn how_many_lines_was_sent(&self) -> usize {
            self.expect - self.remain
        }
    }
}
