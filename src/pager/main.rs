pub(crate) mod cli;
pub(crate) mod neovim;
pub(crate) mod context;

pub type NeovimConnection = connection::NeovimConnection<neovim::NeovimActions>;
pub type NeovimBuffer = connection::Buffer<connection::IoWrite>;


#[tokio::main(worker_threads=2)]
async fn main() {

    connection::init_logger();

    let env_ctx = context::gather_env::enter();

    main::warn_if_incompatible_options(&env_ctx.opt);

    validate_files(env_ctx).await;
}

mod main {

    // Some options takes effect only when page would be
    // spawned from neovim's terminal
    pub fn warn_if_incompatible_options(opt: &super::cli::Options) {
        if opt.address.is_some() {
            return
        }

        if opt.instance_close.is_some() {
            log::warn!(
                target: "usage",
                "Instance close (-x) is ignored \
                if address (-a or $NVIM) isn't set"
            );
        }
        if opt.is_output_split_implied() {
            log::warn!(
                target: "usage",
                "Split (-r -l -u -d -R -L -U -D) is ignored \
                if address (-a or $NVIM) isn't set"
            );
        }
        if opt.back || opt.back_restore {
            log::warn!(
                target: "usage",
                "Switch back (-b -B) is ignored \
                if address (-a or $NVIM) isn't set"
            );
        }
    }
}


async fn validate_files(mut env_ctx: context::EnvContext) {
    log::info!(target: "context", "{env_ctx:#?}");

    let files_count = env_ctx.opt.files.len();
    for i in 0..files_count {

        use cli::FileOption::Path;
        let Path(path) = &mut env_ctx.opt.files[i] else {
            // Uri
            continue
        };

        match std::fs::canonicalize(&path) {
            Ok(canonical) => {

                *path = canonical
                    .to_string_lossy()
                    .to_string()
            }
            Err(e) => {
                log::error!(
                    target: "open file",
                    r#"Cannot open "{path}": {e}"#);

                env_ctx.opt.files
                    .remove(i);
            }
        }
    }

    let all_files_not_exists = files_count > 0
        && env_ctx.opt.files.is_empty();
    if all_files_not_exists &&
        !env_ctx.input_from_pipe &&
        !env_ctx.opt.is_output_implied() &&
        !env_ctx.opt.is_output_split_implied()
    {
        std::process::exit(1)
    }

    prefetch_lines(env_ctx).await
}


async fn prefetch_lines(env_ctx: context::EnvContext) {
    log::info!(target: "context", "{env_ctx:#?}");

    use context::gather_env::PrefetchLinesUsage;
    use context::gather_env::PrefetchLinesSource;

    let PrefetchLinesUsage::Enabled {
        line_count,
        term_width,
        source,
    } = &env_ctx.prefetch_usage else {

        let cli_ctx = context::check_usage::enter(env_ctx);
        connect_neovim(cli_ctx).await;

        return
    };

    let mut prefetch_source_stdin;
    let mut prefetch_source_file;
    let prefetch_source: &mut dyn std::io::Read;

    match source {
        PrefetchLinesSource::Stdin => {
            prefetch_source_stdin = std::io::stdin();

            prefetch_source = &mut prefetch_source_stdin;
        },

        PrefetchLinesSource::File(path) => {
            let file = std::fs::File::open(path)
                .expect("Cannot open file");
            prefetch_source_file = std::io::BufReader::new(file);

            prefetch_source = &mut prefetch_source_file;
        },
    }

    let mut i = line_count + 1;
    let mut prefetched_lines = Vec::with_capacity(i);
    let mut bytes = std::io::Read::bytes(prefetch_source);

    'read_next_ln: while i > 0 {
        let mut ln = Vec::with_capacity(*term_width);

        while let Some(b) = bytes.next() {
            match b {
                Err(e) => {
                    panic!("Failed to prefetch line from stdin: {e}")
                }
                Ok(eol @ b'\n') => {
                    ln.push(eol);
                    ln.shrink_to_fit();
                    prefetched_lines.push(ln);
                    i -= 1;
                    continue 'read_next_ln;
                }
                Ok(b) => {
                    ln.push(b);
                    if ln.len() == *term_width {
                        prefetched_lines.push(ln);
                        i -= 1;
                        continue 'read_next_ln;
                    }
                }
            }
        }

        prefetched_lines.push(ln);

        if let PrefetchLinesUsage::Enabled {
            source: PrefetchLinesSource::File(path),
            ..
        } = env_ctx.prefetch_usage {

            let extenstion = std::path::Path::new(&path)
                .extension()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| String::from(&env_ctx.opt.output.filetype));

            dump_prefetched_lines_and_exit(
                prefetched_lines,
                &extenstion
            )
        } else {

            dump_prefetched_lines_and_exit(
                prefetched_lines,
                &env_ctx.opt.output.filetype,
            )
        };
    }

    let mut cli_ctx = context::check_usage::enter(env_ctx);
    cli_ctx
        .lines_has_been_prefetched(prefetched_lines);

    connect_neovim(cli_ctx).await;
}


fn dump_prefetched_lines_and_exit(lines: Vec<Vec<u8>>, filetype: &str) -> ! {
    log::info!(target: "dump", "{filetype}: {} lines", lines.len());

    let stdout;
    let mut stdout_lock;
    let mut bat_proc = None;

    let output: &mut dyn std::io::Write;

    if !filetype.is_empty() && filetype != "pager" {
        let try_spawn_bat = std::process::Command::new("bat")
            .arg("--plain")
            .arg("--paging=never")
            .arg("--color=always")
            .arg(&format!("--language={}", filetype))
            .stdin(std::process::Stdio::piped())
            .spawn();

        match try_spawn_bat {
            Ok(proc) => {
                log::info!(target: "dump", "use bat");

                let proc = bat_proc.get_or_insert(proc);
                output = proc.stdin
                    .as_mut()
                    .expect("Cannot get bat stdin")
            }
            Err(e) => {
                log::warn!(target: "dump", "cannot spawn bat, use stdout: {e:?}");

                stdout = std::io::stdout();
                stdout_lock = stdout.lock();
                output = &mut stdout_lock;
            }
        }
    } else {
        log::info!(target: "dump", "use stdout");

        stdout = std::io::stdout();
        stdout_lock = stdout.lock();
        output = &mut stdout_lock
    }

    for ln in lines {
        std::io::Write::write_all(output, &ln)
            .expect("Cannot dump prefetched line");
    }
    output.flush()
        .expect("Cannot flush");

    if let Some(mut proc) = bat_proc {
        proc.wait()
            .expect("bat process ended unexpectedly");
    }

    std::process::exit(0)
}


async fn connect_neovim(cli_ctx: context::UsageContext) {
    log::info!(target: "context", "{cli_ctx:#?}");

    connection::init_panic_hook();

    let mut nvim_conn = connection::open(
        &cli_ctx.tmp_dir,
        &cli_ctx.page_id,
        &cli_ctx.opt.address,
        &cli_ctx.opt.config,
        &cli_ctx.opt.config,
        cli_ctx.print_protection
    ).await;

    let mut nvim_ctx = context::connect_neovim::enter(cli_ctx);
    if nvim_conn.nvim_proc.is_some() {
        nvim_ctx
            .child_neovim_process_has_been_spawned()
    }

    manage_page_state(&mut nvim_conn, nvim_ctx).await
}


async fn manage_page_state(
    nvim_conn: &mut NeovimConnection,
    nvim_ctx: context::NeovimContext
) {
    log::info!(target: "context", "{nvim_ctx:#?}");

    let mut api_actions = neovim_api_usage::begin(nvim_conn, &nvim_ctx);

    api_actions
        .close_page_instance_buffer()
        .await;
    api_actions
        .display_files()
        .await;

    use context::connect_neovim::OutputBufferUsage;
    if let OutputBufferUsage::Disabled = nvim_ctx.outp_buf_usage {

        connection::close_and_exit(nvim_conn).await;
    }

    use context::connect_neovim::InstanceUsage;
    if let InstanceUsage::Enabled { name, .. } = &nvim_ctx.inst_usage {

        let active_instance = api_actions
            .find_instance_buffer(name)
            .await;

        if let Some(active_inst_outp) = active_instance {

            let outp_ctx = context::output_buffer_available::enter(
                nvim_ctx,
                active_inst_outp.pty_path
            );

            manage_output_buffer(
                nvim_conn,
                active_inst_outp.buf,
                outp_ctx
            )
                .await

        } else {
            let new_inst_outp = api_actions
                .create_instance_output_buffer(name)
                .await;

            let mut outp_ctx = context::output_buffer_available::enter(
                nvim_ctx,
                new_inst_outp.pty_path
            );
            outp_ctx
                .instance_output_buffer_has_been_created();

            manage_output_buffer(
                nvim_conn,
                new_inst_outp.buf,
                outp_ctx
            )
                .await
        }

    } else {
        let new_outp = api_actions
            .create_oneoff_output_buffer()
            .await;

        let outp_ctx = context::output_buffer_available::enter(
            nvim_ctx,
            new_outp.pty_path
        );

        manage_output_buffer(
            nvim_conn,
            new_outp.buf,
            outp_ctx
        )
            .await
    };
}


async fn manage_output_buffer(
    nvim_conn: &mut NeovimConnection,
    buf: NeovimBuffer,
    outp_ctx: context::OutputContext
) {
    log::info!(target: "context", "{outp_ctx:#?}");

    let mut outp_buf_actions = output_buffer_usage::begin(
        nvim_conn,
        &outp_ctx,
        buf
    );

    use context::connect_neovim::InstanceUsage;
    if let InstanceUsage::Enabled { name, .. } = &outp_ctx.inst_usage {

        outp_buf_actions
            .update_instance_buffer_title(name)
            .await;
        outp_buf_actions
            .focus_on_instance_buffer(name)
            .await;

    } else {

        outp_buf_actions
            .update_buffer_title()
            .await;
    }

    outp_buf_actions
        .execute_commands()
        .await;
    outp_buf_actions
        .focus_on_initial_buffer()
        .await;

    if outp_ctx.input_from_pipe {
        if outp_ctx.query_lines_count > 0 {
            outp_buf_actions
                .handle_query_output()
                .await;
        } else {
            outp_buf_actions
                .handle_output()
                .await;
        }
    }

    if outp_ctx.print_output_buf_pty {
        println!("{}", outp_ctx.buf_pty_path.to_string_lossy());
    }

    outp_buf_actions
        .execute_disconnect_commands()
        .await;

    connection::close_and_exit(nvim_conn).await;
}



mod neovim_api_usage {
    use super::{
        NeovimConnection,
        context::NeovimContext,
        neovim::{OutputBuffer, OutputCommands}
    };

    /// This struct implements actions that should be done
    /// before output buffer is available
    pub struct ApiActions<'a> {
        nvim_conn: &'a mut NeovimConnection,
        nvim_ctx: &'a NeovimContext,
    }

    pub fn begin<'a>(
        nvim_conn: &'a mut NeovimConnection,
        nvim_ctx: &'a NeovimContext
    ) -> ApiActions<'a> {
        ApiActions {
            nvim_conn,
            nvim_ctx,
        }
    }

    impl<'a> ApiActions<'a> {
        /// Closes buffer marked as instance, when mark is provided by -x argument
        pub async fn close_page_instance_buffer(&mut self) {
            let opt = &self.nvim_ctx.opt;

            if let Some(ref instance) = opt.instance_close {
                self.nvim_conn.nvim_actions
                    .close_instance_buffer(instance)
                    .await
            }
        }


        /// Opens each file provided as free arguments in separate buffers.
        /// Resets focus to initial buffer and window if further
        /// there will be created output buffer in split window,
        /// since we want to see shell from which that output buffer was spawned
        pub async fn display_files(&mut self) {
            let ApiActions {
                nvim_conn: NeovimConnection {
                    nvim_actions,
                    initial_buf_number,
                    initial_win_and_buf,
                    ..
                },
                nvim_ctx
            } = self;

            for f in &nvim_ctx.opt.files {
                if let Err(e) = nvim_actions.open_file_buffer(f.as_str()).await {
                    log::warn!(target: "page file", r#"Error opening "{f:?}": {e}"#);

                    continue;
                }

                let cmd_provided_by_user = &nvim_ctx.opt.output.command.as_deref()
                    .unwrap_or_default();
                let lua_provided_by_user = &nvim_ctx.opt.output.lua.as_deref()
                    .unwrap_or_default();
                let writeable = nvim_ctx.opt.output.writable;

                let file_buf_opts = OutputCommands::for_file_buffer(
                    cmd_provided_by_user,
                    lua_provided_by_user,
                    writeable
                );

                nvim_actions
                    .prepare_output_buffer(*initial_buf_number, file_buf_opts)
                    .await;

                if nvim_ctx.opt.follow_all {
                    nvim_actions
                        .set_current_buffer_follow_output_mode()
                        .await;
                } else {
                    nvim_actions
                        .set_current_buffer_scroll_mode()
                        .await;
                }
            }

            if nvim_ctx.is_split_flag_given_with_files() {
                // Split terminal buffer instead of file buffer
                nvim_actions
                    .switch_to_window_and_buffer(initial_win_and_buf)
                    .await
            }
        }


        /// Returns buffer marked as instance,
        /// together with path to PTY device
        /// associated with it (if some exists)
        pub async fn find_instance_buffer(
            &mut self,
            inst_name: &str
        ) -> Option<OutputBuffer> {
            let outp = self.nvim_conn.nvim_actions
                .find_instance_buffer(inst_name)
                .await;

            outp
        }


        /// Creates a new output buffer
        /// and then marks it as instance buffer
        pub async fn create_instance_output_buffer(
            &mut self,
            inst_name: &str
        ) -> OutputBuffer {
            let outp = self
                .create_oneoff_output_buffer()
                .await;

            self.nvim_conn.nvim_actions
                .mark_buffer_as_instance(
                    &outp.buf,
                    inst_name,
                    &outp.pty_path.to_string_lossy()
                )
                .await;

            outp
        }


        /// Creates a new output buffer using split window if required.
        /// Also sets some nvim options for better reading experience
        pub async fn create_oneoff_output_buffer(&mut self) -> OutputBuffer {
            let ApiActions {
                nvim_conn: NeovimConnection {
                    nvim_actions,
                    initial_buf_number,
                    channel,
                    nvim_proc,
                    ..
                },
                nvim_ctx
            } = self;

            let outp = if nvim_proc.is_some() && nvim_ctx.opt.files.is_empty() {
                nvim_actions
                    .create_replacing_output_buffer()
                    .await
            } else if nvim_ctx.outp_buf_usage.is_create_split() {
                nvim_actions
                    .create_split_output_buffer(&nvim_ctx.opt.output.split)
                    .await
            } else {
                nvim_actions
                    .create_switching_output_buffer()
                    .await
            };

            let outp_buf_opts = OutputCommands::for_output_buffer(
                &nvim_ctx.page_id,
                *channel,
                nvim_ctx.query_lines_count,
                &nvim_ctx.opt.output
            );
            nvim_actions
                .prepare_output_buffer(*initial_buf_number, outp_buf_opts)
                .await;

            outp
        }
    }
}

mod output_buffer_usage {
    use super::{NeovimConnection, NeovimBuffer, context::OutputContext};
    use connection::NotificationFromNeovim;
    use std::io::{Read, Write};

    /// This struct implements actions that should be done
    /// after output buffer is attached
    pub struct BufferActions<'a> {
        nvim_conn: &'a mut NeovimConnection,
        outp_ctx: &'a OutputContext,
        buf: NeovimBuffer,
        buf_pty: once_cell::unsync::OnceCell<std::fs::File>,
        lines_displayed: usize,
    }

    pub fn begin<'a>(
        nvim_conn: &'a mut NeovimConnection,
        outp_ctx: &'a OutputContext,
        buf: NeovimBuffer
    ) -> BufferActions<'a> {
        BufferActions {
            nvim_conn,
            outp_ctx,
            buf,
            buf_pty: once_cell::unsync::OnceCell::new(),
            lines_displayed: 0,
        }
    }

    impl<'a> BufferActions<'a> {
        /// This function updates buffer title depending on -n value.
        /// Icon symbol is received from neovim side
        /// and is prepended to the left of buffer title
        pub async fn update_buffer_title(&mut self) {
            let BufferActions {
                outp_ctx,
                buf,
                nvim_conn: NeovimConnection { nvim_actions, .. },
                ..
            } = self;

            let (page_icon_key, page_icon_default) = if outp_ctx.input_from_pipe {
                ("page_icon_pipe", " |")
            } else {
                ("page_icon_redirect", " >")
            };
            let mut buf_title = nvim_actions
                .get_var_or(page_icon_key, page_icon_default)
                .await;

            if let Some(ref buf_name) = outp_ctx.opt.name {
                buf_title.insert_str(0, buf_name);
            }

            nvim_actions
                .update_buffer_title(buf, &buf_title)
                .await;
        }


        /// This function updates instance buffer title
        /// depending on its name and -n value.
        /// Instance name will be prepended to the left
        /// of the icon symbol.
        pub async fn update_instance_buffer_title(&mut self, inst_name: &str) {
            let BufferActions {
                outp_ctx,
                buf,
                nvim_conn: NeovimConnection { nvim_actions, .. },
                ..
            } = self;

            let (page_icon_key, page_icon_default) = ("page_icon_instance", "@ ");
            let mut buf_title = nvim_actions
                .get_var_or(page_icon_key, page_icon_default)
                .await;
            buf_title.insert_str(0, inst_name);

            if let Some(ref buf_name) = outp_ctx.opt.name {
                if buf_name != inst_name {
                    buf_title.push_str(buf_name);
                }
            }

            nvim_actions
                .update_buffer_title(buf, &buf_title)
                .await;
        }


        /// Resets instance buffer focus and content.
        /// This is required to provide some functionality
        /// not available through neovim API
        pub async fn focus_on_instance_buffer(&mut self, inst_name: &str) {
            let BufferActions {
                outp_ctx,
                nvim_conn: NeovimConnection { nvim_actions, .. },
                ..
            } = self;

            if !outp_ctx.inst_usage.is_enabled_and_should_be_focused() {
                return
            }

            nvim_actions
                .focus_instance_buffer(inst_name)
                .await;

            if outp_ctx.inst_usage.is_enabled_and_should_replace_its_content() {

                const CLEAR_SCREEN_SEQ: &[u8] = b"\x1B[3J\x1B[H\x1b[2J";
                self
                    .get_buffer_pty()
                    .write_all(CLEAR_SCREEN_SEQ)
                    .expect("Cannot write clear screen sequence");
            }
        }


        /// Executes PageConnect (-C) and post command (-E) on page buffer.
        /// If any of these flags are passed then
        /// output buffer should be already focused
        pub async fn execute_commands(&mut self) {
            let BufferActions {
                outp_ctx,
                nvim_conn: NeovimConnection { nvim_actions, .. },
                ..
            } = self;

            if outp_ctx.opt.command_auto {
                nvim_actions
                    .execute_connect_autocmd_on_current_buffer()
                    .await;
            }

            if let Some(ref lua_expr) = outp_ctx.opt.lua_post {
                nvim_actions
                    .execute_command_post_lua(lua_expr)
                    .await;
            }
            if let Some(ref command) = outp_ctx.opt.command_post {
                nvim_actions
                    .execute_command_post(command)
                    .await;
            }
        }


        /// Sets cursor position on page buffer and on current buffer
        /// depending on -f, -b, and -B flags provided.
        /// First if condition on this function ensures
        /// that it's really necessary to do any action,
        /// to circumvent flicker with `page -I
        /// existed -b` and `page -I existed -B` invocations
        pub async fn focus_on_initial_buffer(&mut self) {
            let BufferActions {
                outp_ctx,
                nvim_conn: NeovimConnection { nvim_actions, initial_win_and_buf, .. },
                ..
            } = self;

            if outp_ctx.inst_usage.is_enabled_but_should_be_unfocused() {
                return
            }

            if outp_ctx.opt.follow {
                nvim_actions
                    .set_current_buffer_follow_output_mode()
                    .await;
            } else {
                nvim_actions
                    .set_current_buffer_scroll_mode()
                    .await;
            }

            if outp_ctx.restore_initial_buf_focus.is_disabled() {
                return
            }

            nvim_actions
                .switch_to_window_and_buffer(initial_win_and_buf)
                .await;

            if outp_ctx.restore_initial_buf_focus.is_vi_mode_insert() {
                nvim_actions
                    .set_current_buffer_insert_mode()
                    .await;
            }
        }


        /// Writes lines from stdin directly into PTY device
        /// associated with output buffer.
        pub async fn handle_output(&mut self) {
            // First write all prefetched lines if any available
            for ln in &self.outp_ctx.prefetched_lines.0[..] {

                self.display_line(ln)
                    .await
                    .expect("Cannot write next prefetched line");
            }

            // Then copy the rest of lines from stdin into buffer pty
            let mut ln = Vec::with_capacity(2048);
            for b in std::io::stdin().bytes() {

                match b {
                    Err(e) => {
                        log::warn!(
                            target: "output",
                            "Error reading line from stdin: {e}"
                        );

                        break;
                    }

                    Ok(eol @ b'\n') => {
                        ln.push(eol);

                        self.display_line(&ln)
                            .await
                            .expect("Cannot write next line");

                        ln.clear();
                    }

                    Ok(b) => ln.push(b)
                }
            }
        }


        /// In case if -q <count> argument provided it
        /// might block until next line will be request from neovim side.
        pub async fn handle_query_output(&mut self) {
            let mut state = QueryState::default();
            state.next_part(self.outp_ctx.query_lines_count);

            // First write all prefetched lines if any available
            let mut prefetched_lines_iter = self.outp_ctx.prefetched_lines.0.iter();
            loop {
                self.exchange_query_messages(&mut state)
                    .await;

                let Some(ln) = prefetched_lines_iter.next() else {
                    log::info!(target: "output", "Proceed with stdin");

                    break
                };

                self.display_line(ln)
                    .await
                    .expect("Cannot write next prefetched queried line");

                state.line_has_been_sent();
            }

            self.exchange_query_messages(&mut state)
                .await;

            // Then copy the rest of lines from stdin into buffer pty
            let mut ln = Vec::with_capacity(2048);
            for b in std::io::stdin().bytes() {

                match b {
                    Err(e) => {
                        log::warn!(
                            target: "output",
                            "Error reading queried line from stdin: {e}"
                        );

                        break;
                    }

                    Ok(eol @ b'\n') => {
                        ln.push(eol);

                        self.display_line(&ln)
                            .await
                            .expect("Cannot write next line");

                        state.line_has_been_sent();
                        self.exchange_query_messages(&mut state)
                            .await;

                        ln.clear();
                    }

                    Ok(b) => ln.push(b)
                }
            }

            self.nvim_conn.nvim_actions
                .notify_query_finished(state.how_many_lines_was_sent())
                .await;

            self.nvim_conn.nvim_actions
                .notify_end_of_input()
                .await;
        }


        /// Writes line to PTY device and gracefully handles failures:
        /// if error occurs then page waits for "page_buffer_closed"
        /// notification that's sent on BufDelete event and signals
        /// that buffer was closed intentionally, so page must just exit.
        /// If no such notification was arrived then page crashes
        /// with the received IO error
        async fn display_line(&mut self, ln: &[u8]) -> std::io::Result<()> {
            let pty = self.get_buffer_pty();

            if let Err(e) = pty.write_all(ln) {
                log::info!(target: "writeline", "got error: {e:?}");

                let wait_secs = std::time::Duration::from_secs(1);
                let notification_future = self.nvim_conn.rx
                    .recv();

                match tokio::time::timeout(wait_secs, notification_future)
                    .await
                {
                    Ok(Some(NotificationFromNeovim::BufferClosed)) => {
                        log::info!(
                            target: "writeline",
                            "Buffer was closed, not all input is shown"
                        );

                        connection::close_and_exit(self.nvim_conn).await
                    },
                    Ok(None) if self.nvim_conn.nvim_proc.is_some() => {
                        log::info!(
                            target: "writeline",
                            "Neovim was closed, not all input is shown"
                        );

                        connection::close_and_exit(self.nvim_conn).await
                    },

                    _ => return Err(e),
                }
            }



            if self.outp_ctx.pagerization_usage.is_enabled() {
                self.lines_displayed += 1;
                if self.outp_ctx.pagerization_usage
                    .should_pagerize(self.lines_displayed)
                {
                    self.pagerize_output()
                }
            }

            Ok(())
        }

        /// If there's more than 100_000 lines to read and -z flag provided
        /// then output will be pagerized through spawning `page` again and again
        fn pagerize_output(&self) {
            let mut page_args = std::env::args();
            page_args.next(); // skip `page`

            let page_args = page_args
                .filter(|a| a != "--pagerize-hidden");

            let nvim_addr = if let Some(addr) = &self.outp_ctx.opt.address {
                addr.clone()
            } else {
                std::env::temp_dir()
                    .join("neovim-page")
                    .join(&format!("socket-{}", &self.outp_ctx.page_id))
                    .to_string_lossy()
                    .to_string()
            };

            std::process::Command::new("page")
                .args(page_args)
                .arg("--pagerize-hidden")
                .env("NVIM", &nvim_addr)
                .spawn()
                .expect("Cannot spawn `page`")
                .wait()
                .expect("`page` died unexpectedly")
                .code()
                .unwrap_or(0);
        }


        /// If the whole queried part was sent waits
        /// for notifications from neovim, processes some
        /// or schedules further query in write loop
        async fn exchange_query_messages(&mut self, s: &mut QueryState) {
            if !s.is_whole_part_sent() {
                return
            }

            self.nvim_conn.nvim_actions
                .notify_query_finished(s.how_many_lines_was_sent())
                .await;

            match self.nvim_conn.rx
                .recv()
                .await
            {
                Some(NotificationFromNeovim::FetchLines(n)) =>
                    s.next_part(n),

                Some(NotificationFromNeovim::FetchPart) =>
                    s.next_part(self.outp_ctx.query_lines_count),

                Some(NotificationFromNeovim::BufferClosed) => {
                    log::info!(target: "output-state", "Buffer closed");

                    connection::close_and_exit(self.nvim_conn).await
                }
                None => {
                    log::info!(target: "output-state", "Neovim closed");

                    connection::close_and_exit(self.nvim_conn).await
                }
            }
        }


        /// Executes PageDisconnect autocommand if -C flag was provided.
        /// Some time might pass since page buffer was created and
        /// output was started, so this function might temporarily refocus
        /// on output buffer in order to run autocommand
        pub async fn execute_disconnect_commands(&mut self) {
            let BufferActions {
                nvim_conn: NeovimConnection { nvim_actions, initial_win_and_buf, .. },
                buf,
                outp_ctx,
                ..
            } = self;

            if !outp_ctx.opt.command_auto {
                return
            }

            let active_buf = nvim_actions
                .get_current_buffer()
                .await
                .expect("Cannot get currently active buffer to execute PageDisconnect");

            let switched = buf != &active_buf;
            if switched {
                nvim_actions
                    .switch_to_buffer(buf)
                    .await
                    .expect("Cannot switch back to page buffer");
            }

            nvim_actions
                .execute_disconnect_autocmd_on_current_buffer()
                .await;

            // Page buffer probably may be closed in autocommand
            let still_loaded = active_buf.is_loaded()
                .await
                .expect("Cannot check if buffer loaded");

            if switched && still_loaded {
                nvim_actions
                    .switch_to_buffer(&active_buf)
                    .await
                    .expect("Cannot switch back to active buffer");

                let same_buffer = initial_win_and_buf.1 == active_buf;
                if same_buffer && outp_ctx.restore_initial_buf_focus.is_vi_mode_insert() {
                    nvim_actions
                        .set_current_buffer_insert_mode()
                        .await;
                }
            }
        }

        /// Returns PTY device associated with output buffer.
        /// This function ensures that PTY device is opened only once
        fn get_buffer_pty(&mut self) -> &mut std::fs::File {
            self.buf_pty
                .get_or_init(|| {
                    std::fs::OpenOptions::new()
                        .append(true)
                        .open(&self.outp_ctx.buf_pty_path)
                        .expect("Cannot open PTY device")
                });

            self.buf_pty.get_mut()
                .unwrap()
        }
    }

    /// Encapsulates state of querying lines from neovim side
    /// with :Page <count> command.
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


        fn line_has_been_sent(&mut self) {
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
