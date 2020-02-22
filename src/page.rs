pub(crate) mod cli;
pub(crate) mod neovim;
pub(crate) mod context;

use neovim_lib::neovim_api;


fn main() {
    init_logger();
    let cli_ctx = context::page_spawned::enter();
    issue_warnings(&cli_ctx);
    begin_neovim_connection_usage(cli_ctx);
}

pub fn init_logger() {
    let rust_log = std::env::var("RUST_LOG").ok();
    let level = rust_log.as_deref().unwrap_or("warn");
    use std::str::FromStr;
    let level_filter = log::LevelFilter::from_str(level).expect("Cannot parse $RUST_LOG value");
    fern::Dispatch::new()
        .format(|cb, msg, record| {
            cb.finish({ format_args!("[{}][{}] {}", record.level(), record.target(), msg) })
        })
        .level(level_filter)
        .level_for("neovim_lib", log::LevelFilter::Off)
        .chain(std::io::stderr())
        .apply()
        .unwrap();
}

fn issue_warnings(cli_ctx: &context::CliContext) {
    if cli_ctx.is_inst_close_flag_given_without_address() {
        log::warn!("Instance close (-x) is ignored if address (-a or $NVIM_LISTEN_ADDRESS) isn't set");
    }
    if cli_ctx.is_split_flag_given_without_address() {
        log::warn!("Split (-r -l -u -d -R -L -U -D) is ignored if address (-a or $NVIM_LISTEN_ADDRESS) isn't set");
    }
    if cli_ctx.is_back_flag_given_without_address() {
        log::warn!("Switch back (-b -B) is ignored if address (-a or $NVIM_LISTEN_ADDRESS) isn't set");
    }
    if cli_ctx.is_query_flag_given_without_reading_from_pipe() {
        log::warn!("Query (-q) is ignored when page doesn't read input from pipe");
    }
}


fn begin_neovim_connection_usage(cli_ctx: context::CliContext) {
    log::info!("cli_ctx: {:#?}", &cli_ctx);
    let mut nvim_conn = neovim::connection::open(&cli_ctx);
    let nvim_ctx = if nvim_conn.is_child_neovim_process_spawned() {
        context::neovim_connected::enter(cli_ctx).with_child_neovim_process_spawned()
    } else {
        context::neovim_connected::enter(cli_ctx)
    };
    begin_neovim_api_usage(&mut nvim_conn, nvim_ctx);
    neovim::connection::close(nvim_conn);
}

fn begin_neovim_api_usage(nvim_conn: &mut neovim::NeovimConnection, nvim_ctx: context::NeovimContext) {
    log::info!("nvim_ctx: {:#?}", &nvim_ctx);
    let mut api_actions = neovim_api_usage::begin(nvim_conn, &nvim_ctx);
    api_actions.close_page_instance_buffer();
    api_actions.display_files();
    if nvim_ctx.outp_buf_usage.is_disabled() {
        return
    }
    if let Some(inst_name) = nvim_ctx.inst_usage.is_enabled() {
        if let Some((buf, buf_pty_path)) = api_actions.find_instance_buffer(inst_name) {
            let outp_ctx = context::output_buffer_available::enter(nvim_ctx, buf_pty_path);
            begin_output_buffer_usage(nvim_conn, buf, outp_ctx)
        } else {
            let (buf, buf_pty_path) = api_actions.create_instance_output_buffer(inst_name);
            let outp_ctx = context::output_buffer_available::enter(nvim_ctx, buf_pty_path);
            begin_output_buffer_usage(nvim_conn, buf, outp_ctx)
        }
    } else {
        let (buf, buf_pty_path) = api_actions.create_oneoff_output_buffer();
        let outp_ctx = context::output_buffer_available::enter(nvim_ctx, buf_pty_path);
        begin_output_buffer_usage(nvim_conn, buf, outp_ctx)
    };
}

fn begin_output_buffer_usage(nvim_conn: &mut neovim::NeovimConnection, buf: neovim_api::Buffer, outp_ctx: context::OutputContext) {
    log::info!("oup_ctx: {:#?}", &outp_ctx);
    let mut outp_buf_actions = output_buffer_usage::begin(nvim_conn, &outp_ctx, buf);
    if let Some(inst_name) = outp_ctx.inst_usage.is_enabled() {
        outp_buf_actions.update_instance_buffer_title(inst_name);
        outp_buf_actions.focus_on_instance_buffer();
    } else {
        outp_buf_actions.update_buffer_title();
    }
    outp_buf_actions.execute_commands();
    outp_buf_actions.focus_on_initial_buffer();
    outp_buf_actions.handle_output();
    outp_buf_actions.execute_disconnect_commands();
}



mod neovim_api_usage {
    use crate::{context::NeovimContext, neovim::NeovimConnection};
    use neovim_lib::neovim_api::Buffer;
    use std::path::PathBuf;

    /// This struct implements actions that should be done before output buffer is available
    pub struct RemoteActions<'a> {
        nvim_conn: &'a mut NeovimConnection,
        nvim_ctx: &'a NeovimContext,
    }

    pub fn begin<'a>(nvim_conn: &'a mut NeovimConnection, nvim_ctx: &'a NeovimContext) -> RemoteActions<'a> {
        RemoteActions {
            nvim_conn,
            nvim_ctx,
        }
    }

    impl<'a> RemoteActions<'a> {
        /// Closes buffer marked as instance, when mark is provided by -x argument.
        /// If neovim is spawned by page then it guaranteedly doesn't have any instances,
        /// so in this case -x argument will be useless and warning would be printed about that
        pub fn close_page_instance_buffer(&mut self) {
            if let Some(ref instance) = self.nvim_ctx.opt.instance_close {
                self.nvim_conn.nvim_actions.close_instance_buffer(instance)
            }
        }

        /// Opens each file provided as free arguments in separate buffers.
        /// Resets focus to initial buffer and window if further there will be created output buffer in split window,
        /// since we want to see shell from which that output buffer was spawned
        pub fn display_files(&mut self) {
            let RemoteActions { nvim_conn: NeovimConnection { nvim_actions, initial_buf_number, initial_win_and_buf, .. }, nvim_ctx } = self;
            for f in &nvim_ctx.opt.files {
                if let Err(e) = nvim_actions.open_file_buffer(f) {
                    log::warn!("Error opening \"{}\": {}", f, e);
                } else {
                    let cmd = &nvim_ctx.opt.command.as_deref().unwrap_or_default();
                    nvim_actions.prepare_file_buffer(cmd, *initial_buf_number);
                    if nvim_ctx.opt.follow_all {
                        nvim_actions.set_current_buffer_follow_output_mode();
                    } else {
                        nvim_actions.set_current_buffer_scroll_mode();
                    }
                }
            }
            if nvim_ctx.is_split_flag_given_with_files() {
                nvim_actions.switch_to_window_and_buffer(&initial_win_and_buf)
            }
        }

        /// Returns buffer marked as instance and path to PTY device associated with it.
        /// If neovim was spawned by page then it guaranteedly doesnt' have any instance,
        /// so in this case -i and -I arguments would be useless and warning would be printed about that
        pub fn find_instance_buffer(&mut self, inst_name: &str) -> Option<(Buffer, PathBuf)> {
            self.nvim_conn.nvim_actions.find_instance_buffer(inst_name)
        }

        /// Creates a new output buffer and then marks it as instance buffer.
        /// Reuturns this buffer and PTY device associated with it
        pub fn create_instance_output_buffer(&mut self, inst_name: &str) -> (Buffer, PathBuf) {
            let (buf, buf_pty_path) = self.create_oneoff_output_buffer();
            self.nvim_conn.nvim_actions.mark_buffer_as_instance(&buf, inst_name, &buf_pty_path.to_string_lossy());
            (buf, buf_pty_path)
        }

        /// Creates a new output buffer using split window if required.
        /// Also sets some nvim options for better reading experience
        pub fn create_oneoff_output_buffer(&mut self) -> (Buffer, PathBuf) {
            let RemoteActions { nvim_conn: NeovimConnection { nvim_actions, initial_buf_number, .. }, nvim_ctx } = self;
            let buf = if nvim_ctx.outp_buf_usage.is_create_split() {
                nvim_actions.create_split_output_buffer(&nvim_ctx.opt)
            } else {
                nvim_actions.create_substituting_output_buffer()
            };
            let buf_pty_path = nvim_actions.get_current_buffer_pty_path();
            nvim_actions.prepare_output_buffer(
                &nvim_ctx.page_id,
                &nvim_ctx.opt.filetype,
                &nvim_ctx.opt.command.as_deref().unwrap_or_default(),
                nvim_ctx.opt.pwd,
                nvim_ctx.opt.query_lines,
                *initial_buf_number
            );
            (buf, buf_pty_path)
        }
    }
}

mod output_buffer_usage {
    use crate::{context::OutputContext, neovim::{NeovimConnection, NotificationFromNeovim}};
    use neovim_lib::neovim_api::Buffer;
    use std::{
        fs::{File, OpenOptions},
        io::{self, BufRead, BufReader, Write},
    };

    /// This struct implements actions that should be done after output buffer is attached
    pub struct BufferActions<'a> {
        nvim_conn: &'a mut NeovimConnection,
        outp_ctx: &'a OutputContext,
        buf: Buffer,
        buf_pty: Option<File>,
    }

    pub fn begin<'a>(nvim_conn: &'a mut NeovimConnection, outp_ctx: &'a OutputContext, buf: Buffer) -> BufferActions<'a> {
        BufferActions {
            nvim_conn,
            outp_ctx,
            buf,
            buf_pty: None,
        }
    }

    impl<'a> BufferActions<'a> {
        /// This function updates buffer title depending on -n value.
        /// Icon symbol is received from neovim side and prepended it to the left of buffer title.
        pub fn update_buffer_title(&mut self) {
            let BufferActions { outp_ctx, buf, nvim_conn: NeovimConnection { nvim_actions, .. }, .. } = self;
            let (page_icon_default, page_icon_key) = if outp_ctx.input_from_pipe { (" |", "page_icon_pipe") } else { (" >", "page_icon_redirect") };
            let mut buf_title = nvim_actions.get_var_or(page_icon_key, page_icon_default);
            if let Some(ref buf_name) = outp_ctx.opt.name {
                buf_title.insert_str(0, buf_name);
            }
            nvim_actions.update_buffer_title(&buf, &buf_title);
        }

        /// This function updates instance buffer title depending on its name and -n value.
        /// Instance name will be prepended to the left of icon symbol.
        pub fn update_instance_buffer_title(&mut self, inst_name: &str) {
            let BufferActions { outp_ctx, buf, nvim_conn: NeovimConnection { nvim_actions, .. }, .. } = self;
            let (page_icon_key, page_icon_default) = ("page_icon_instance", "@ ");
            let mut buf_title = nvim_actions.get_var_or(page_icon_key, page_icon_default);
            buf_title.insert_str(0, inst_name);
            if let Some(ref buf_name) = outp_ctx.opt.name {
                if buf_name != inst_name {
                    buf_title.push_str(buf_name);
                }
            }
            nvim_actions.update_buffer_title(&buf, &buf_title);
        }

        /// Updates buffer title and resets instance buffer focus and content.
        /// This is required to provide some functionality not available through neovim API.
        pub fn focus_on_instance_buffer(&mut self) {
            let BufferActions { outp_ctx, buf, nvim_conn: NeovimConnection { nvim_actions, .. }, .. } = self;
            if outp_ctx.inst_usage.is_enabled_and_focus_on_it_required() {
                nvim_actions.focus_instance_buffer(&buf);
                if outp_ctx.inst_usage.is_enabled_and_should_replace_its_content() {
                    writeln!(self.get_buffer_pty(), "\x1B[3J\x1B[H\x1b[2J").expect("Cannot write clear screen sequence");
                }
            }
        }

        /// Executes PageConnect (-C) and post command (-E) on page buffer.
        /// If any of these flags are passed then output buffer should be already focused.
        pub fn execute_commands(&mut self) {
            let BufferActions { outp_ctx, nvim_conn: NeovimConnection { nvim_actions, .. }, .. } = self;
            if outp_ctx.opt.command_auto {
                nvim_actions.execute_connect_autocmd_on_current_buffer();
            }
            if let Some(ref command) = outp_ctx.opt.command_post {
                nvim_actions.execute_command_post(&command);
            }
        }

        /// Sets cursor position on page buffer and on current buffer depending on -f, -b, and -B flags provided.
        /// First if condition on this function ensures that it's really necessary to do any action,
        /// to circumvent flicker with `page -I existed -b` and `page -I existed -B` invocations
        pub fn focus_on_initial_buffer(&mut self) {
            let BufferActions { outp_ctx, nvim_conn: NeovimConnection { nvim_actions, initial_win_and_buf, .. }, .. } = self;
            if outp_ctx.inst_usage.is_enabled_but_focus_on_it_was_skipped() {
                return
            }
            if outp_ctx.opt.follow {
                nvim_actions.set_current_buffer_follow_output_mode();
            } else {
                nvim_actions.set_current_buffer_scroll_mode();
            }
            if outp_ctx.restore_initial_buf_focus.is_disabled() {
                return
            }
            nvim_actions.switch_to_window_and_buffer(&initial_win_and_buf);
            if outp_ctx.restore_initial_buf_focus.is_vi_mode_insert() {
                nvim_actions.set_current_buffer_insert_mode();
            }
        }

        /// Writes lines from stdin directly into PTY device associated with output buffer.
        /// In case if -q <count> argument provided it might block until next line from neovim is requested.
        /// If page isn't piped then it simply prints actual path to PTY device associated with output buffer,
        /// so user can redirect into it manually.
        pub fn handle_output(&mut self) {
            if self.outp_ctx.input_from_pipe {
                let stdin = io::stdin();
                if self.outp_ctx.is_query_disabled() {
                    io::copy(&mut io::stdin().lock(), self.get_buffer_pty()).expect("Read from stdin failed unexpectedly");
                    return
                }
                let mut stdin_lines = BufReader::new(stdin.lock()).lines();
                let mut s = QueryState::default();
                s.next_part(self.outp_ctx.opt.query_lines);
                loop {
                    if s.is_whole_part_sent() {
                        self.nvim_conn.nvim_actions.notify_query_finished(s.how_many_lines_was_sent());
                        match self.nvim_conn.rx.recv() {
                            Ok(NotificationFromNeovim::FetchLines(n)) => s.next_part(n),
                            Ok(NotificationFromNeovim::FetchPart) => s.next_part(self.outp_ctx.opt.query_lines),
                            _ => break
                        }
                    }
                    match stdin_lines.next() {
                        Some(Ok(l)) => writeln!(self.get_buffer_pty(), "{}", l).expect("Cannot write next line"),
                        Some(Err(e)) => {
                            log::warn!("Error reading line from stdin: {}", e);
                            break
                        }
                        None => {
                            self.nvim_conn.nvim_actions.notify_query_finished(s.how_many_lines_was_sent());
                            break
                        }
                    }
                    s.line_sent();
                }
                self.nvim_conn.nvim_actions.notify_end_of_input();
            }
            if self.outp_ctx.print_output_buf_pty {
                println!("{}", self.outp_ctx.buf_pty_path.to_string_lossy());
            }
        }

        /// Executes PageDisconnect autocommand if -C flag was provided.
        /// Some time might pass since page buffer was created and output was started,
        /// so this function might temporarily focus on output buffer in order to run
        /// autocommand.
        pub fn execute_disconnect_commands(&mut self) {
            let BufferActions { nvim_conn: NeovimConnection { nvim_actions, initial_win_and_buf, .. }, buf, outp_ctx, .. } = self;
            if outp_ctx.opt.command_auto {
                let active_buf = nvim_actions.get_current_buffer();
                let switched = buf != &active_buf;
                if switched {
                    nvim_actions.switch_to_buffer(&buf);
                }
                nvim_actions.execute_disconnect_autocmd_on_current_buffer();
                if switched {
                    nvim_actions.switch_to_buffer(&active_buf);
                    if initial_win_and_buf.1 == active_buf && outp_ctx.restore_initial_buf_focus.is_vi_mode_insert() {
                        nvim_actions.set_current_buffer_insert_mode();
                    }
                }
            }
        }

        /// Returns PTY device associated with output buffer.
        /// This function ensures that PTY device will be opened only once
        fn get_buffer_pty(&mut self) -> &mut File {
            let buf_pty_path = &self.outp_ctx.buf_pty_path;
            self.buf_pty.get_or_insert_with(|| OpenOptions::new().append(true).open(buf_pty_path).expect("Cannot open page PTY"))
        }
    }


    /// Encapsulates state of querying lines from neovim side with :Page <count> command.
    /// Is useful only when -q <count> argument is provided
    #[derive(Default)]
    struct QueryState {
        expect: u64,
        remain: u64,
    }

    impl QueryState {
        fn next_part(&mut self, lines_to_read: u64) {
            self.expect = lines_to_read;
            self.remain = lines_to_read;
        }
        fn line_sent(&mut self) {
            self.remain -= 1;
        }
        fn is_whole_part_sent(&self) -> bool {
            self.remain == 0
        }
        fn how_many_lines_was_sent(&self) -> u64 {
            self.expect - self.remain
        }
    }
}
