use neovim::IoWrite;
use nvim_rs::Buffer;

pub(crate) mod cli;
pub(crate) mod neovim;
pub(crate) mod context;


#[tokio::main]
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
        .format(move |cb, msg, record| cb.finish(
            format_args!(
                "\x1B[1m\x1B[4m[ {:010} | {module} | {target} | {importancy} ]\x1B[0m\n{message}\n",
                exec_time.elapsed().as_micros(),
                module = record.module_path().unwrap_or_default(),
                target = record.target(),
                importancy = record.level(),
                message = msg
            )
        ))
        .level(level_filter)
        .chain(std::io::stderr())
        .apply()
        .unwrap();
}


async fn prefetch_lines(env_ctx: context::EnvContext) {
    log::info!(target: "context", "{:#?}", &env_ctx);
    let mut prefetched_lines = Vec::with_capacity(env_ctx.echo_lines);
    while env_ctx.echo_lines > prefetched_lines.len() {
        let mut line = String::new();
        let remain = std::io::stdin().read_line(&mut line).unwrap();
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
    use std::{io, process};
    let (stdout, mut stdout_lock);
    let mut bat_proc = None;
    let output: &mut dyn io::Write = {
        if !filetype.is_empty() {
            match process::Command::new("bat")
                .arg("--plain")
                .arg("--paging=never")
                .arg("--color=always")
                .arg(&format!("--language={}", filetype))
                .stdin(process::Stdio::piped())
                .spawn()
            {
                Ok(proc) => {
                    bat_proc.get_or_insert(proc).stdin.as_mut().unwrap()
                }
                _ => {
                    (stdout = io::stdout(), stdout_lock = stdout.lock());
                    &mut stdout_lock
                }
            }
        } else {
            (stdout = io::stdout(), stdout_lock = stdout.lock());
            &mut stdout_lock
        }
    };
    for ln in lines {
        io::Write::write(output, ln.as_bytes()).unwrap();
    }
    output.flush().unwrap();
    if let Some(mut proc) = bat_proc {
        proc.wait().unwrap();
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
    let mut nvim_conn = neovim::connection::open(&cli_ctx).await;
    let nvim_ctx = if let Some(_) = nvim_conn.nvim_proc {
        context::connect_neovim::enter(cli_ctx).with_child_neovim_process_spawned()
    } else {
        context::connect_neovim::enter(cli_ctx)
    };
    manage_page_state(&mut nvim_conn, nvim_ctx).await;
    neovim::connection::close(nvim_conn).await;
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
    use nvim_rs::Buffer;
    use crate::{context::NeovimContext, neovim::{IoWrite, NeovimConnection}, neovim::OutputCommands};

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
            let ApiActions { nvim_conn: NeovimConnection { nvim_actions, initial_buf_number, .. }, nvim_ctx } = self;
            let buf = if nvim_ctx.outp_buf_usage.is_create_split() {
                nvim_actions.create_split_output_buffer(&nvim_ctx.opt.output.split).await
            } else {
                nvim_actions.create_substituting_output_buffer().await
            };
            let buf_pty_path = nvim_actions.get_current_buffer_pty_path().await;
            let outp_buf_opts = OutputCommands::for_output_buffer(&nvim_ctx.page_id, &nvim_ctx.opt.output);
            nvim_actions.prepare_output_buffer(*initial_buf_number, outp_buf_opts).await;
            (buf, buf_pty_path)
        }
    }
}

mod output_buffer_usage {
    use nvim_rs::Buffer;

    use crate::{context::OutputContext, neovim::{IoWrite, NeovimConnection, NotificationFromNeovim}};
    use std::{
        fs::{File, OpenOptions},
        io::{BufRead, Write},
    };

    /// This struct implements actions that should be done after output buffer is attached
    pub struct BufferActions<'a> {
        nvim_conn: &'a mut NeovimConnection,
        outp_ctx: &'a OutputContext,
        buf: Buffer<IoWrite>,
        buf_pty: Option<File>,
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
                    writeln!(self.get_buffer_pty(), "\x1B[3J\x1B[H\x1b[2J").expect("Cannot write clear screen sequence");
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
                for ln in self.outp_ctx.prefetched_lines.as_slice() {
                    write!(self.get_buffer_pty(), "{}", ln).expect("Cannot write next prefetched line");
                }
                // Then copy the rest of lines from stdin into buffer pty
                let stdin = std::io::stdin();
                let mut stdin_lock = stdin.lock();
                if !self.outp_ctx.is_query_enabled() {
                    std::io::copy(&mut stdin_lock, self.get_buffer_pty()).expect("Read from stdin failed unexpectedly");
                    return
                }
                // If query (-q) is enabled then wait for it
                let mut stdin_lines = stdin_lock.lines();
                let mut s = QueryState::default();
                s.next_part(self.outp_ctx.opt.output.query_lines);
                loop {
                    if s.is_whole_part_sent() {
                        self.nvim_conn.nvim_actions.notify_query_finished(s.how_many_lines_was_sent()).await;
                        match self.nvim_conn.rx.recv().await {
                            Some(NotificationFromNeovim::FetchLines(n)) => s.next_part(n),
                            Some(NotificationFromNeovim::FetchPart) => s.next_part(self.outp_ctx.opt.output.query_lines),
                            _ => break
                        }
                    }
                    match stdin_lines.next() {
                        Some(Ok(ln)) => writeln!(self.get_buffer_pty(), "{}", ln).expect("Cannot write next line"),
                        Some(Err(e)) => {
                            log::warn!(target: "output", "Error reading line from stdin: {}", e);
                            break
                        }
                        None => {
                            self.nvim_conn.nvim_actions.notify_query_finished(s.how_many_lines_was_sent()).await;
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

        /// Executes PageDisconnect autocommand if -C flag was provided.
        /// Some time might pass since page buffer was created and output was started,
        /// so this function might temporarily refocus on output buffer in order to run
        /// autocommand
        pub async fn execute_disconnect_commands(&mut self) {
            let BufferActions { nvim_conn: NeovimConnection { nvim_actions, initial_win_and_buf, .. }, buf, outp_ctx, .. } = self;
            if outp_ctx.opt.command_auto {
                let active_buf = nvim_actions.get_current_buffer().await;
                let switched = buf != &active_buf;
                if switched {
                    nvim_actions.switch_to_buffer(&buf).await;
                }
                nvim_actions.execute_disconnect_autocmd_on_current_buffer().await;
                if switched {
                    nvim_actions.switch_to_buffer(&active_buf).await;
                    if initial_win_and_buf.1 == active_buf && outp_ctx.restore_initial_buf_focus.is_vi_mode_insert() {
                        nvim_actions.set_current_buffer_insert_mode().await;
                    }
                }
            }
        }

        /// Returns PTY device associated with output buffer.
        /// This function ensures that PTY device is opened only once
        fn get_buffer_pty(&mut self) -> &mut File {
            let buf_pty_path = &self.outp_ctx.buf_pty_path;
            self.buf_pty.get_or_insert_with(|| OpenOptions::new().append(true).open(buf_pty_path).expect("Cannot open page PTY"))
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
