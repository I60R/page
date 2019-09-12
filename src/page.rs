pub(crate) mod cli;
pub(crate) mod neovim;
pub(crate) mod context;


use log::info;
use std::{env, str::FromStr};


fn main() {
    init_logger();
    let cli_ctx = context::after_page_spawned();
    info!("cli_ctx: {:#?}", &cli_ctx);
    let mut nvim_conn = neovim::create_connection(&cli_ctx);
    let nvim_ctx = context::after_neovim_connected(cli_ctx, nvim_conn.nvim_proc.is_some());
    info!("nvim_ctx: {:#?}", &nvim_ctx);
    let mut page_actions = page::begin_handling(&mut nvim_conn, &nvim_ctx);
    page_actions.handle_close_instance_buffer();
    page_actions.handle_display_files();
    if nvim_ctx.use_outp_buf {
        let (buf, outp_ctx) = if let Some(inst_name) = nvim_ctx.inst_mode.any() {
            if let Some((buf, buf_pty_path)) = page_actions.find_instance_buffer(inst_name) {
                (buf, context::after_output_found(nvim_ctx, buf_pty_path))
            } else {
                let (buf, buf_pty_path) = page_actions.create_instance_output_buffer(inst_name);
                (buf, context::after_output_created(nvim_ctx, buf_pty_path))
            }
        } else {
            let (buf, buf_pty_path) = page_actions.create_oneoff_output_buffer();
            (buf, context::after_output_created(nvim_ctx, buf_pty_path))
        };
        info!("oup_ctx: {:#?}", &outp_ctx);
        let mut outp_actions = output::begin_handling(&mut nvim_conn, &outp_ctx, buf);
        outp_actions.handle_buffer_title();
        outp_actions.handle_instance_buffer_reset();
        outp_actions.handle_commands();
        outp_actions.handle_cursor_position();
        outp_actions.handle_output();
        outp_actions.handle_disconnect();
    }
    neovim::close_connection(nvim_conn);
}

fn init_logger() {
    let rust_log = env::var("RUST_LOG");
    let level = rust_log.as_ref().map(String::as_ref).unwrap_or("warn");
    let level_filter = log::LevelFilter::from_str(level).expect("Cannot parse $RUST_LOG value");
    fern::Dispatch::new()
        .format(|cb, msg, record| cb.finish({
            format_args!("[{}][{}] {}", record.level(), record.target(), msg)
        }))
        .level(level_filter)
        .level_for("neovim_lib", log::LevelFilter::Off)
        .chain(std::io::stderr())
        .apply()
        .unwrap();
}


mod page {
    use crate::{context::NeovimContext, neovim::NeovimConnection};
    use log::warn;
    use neovim_lib::neovim_api::Buffer;
    use std::path::PathBuf;

    /// This struct implements actions that should be done before output buffer is available
    pub struct PageActions<'a> {
        nvim_conn: &'a mut NeovimConnection,
        nvim_ctx: &'a NeovimContext,
    }

    pub fn begin_handling<'a>(nvim_conn: &'a mut NeovimConnection, nvim_ctx: &'a NeovimContext) -> PageActions<'a> {
        PageActions {
            nvim_conn,
            nvim_ctx,
        }
    }

    impl<'a> PageActions<'a> {
        /// Closes buffer marked as instance, when mark is provided by -x argument.
        /// If neovim is spawned by page then it guaranteedly doesn't have any instances,
        /// so in this case -x argument will be useless and warning would be printed about that
        pub fn handle_close_instance_buffer(&mut self) {
            if let Some(ref instance) = self.nvim_ctx.opt.instance_close {
                if self.nvim_ctx.nvim_child_proc_spawned {
                    warn!("Newly spawned neovim cannot contain any instance (-x is useless)")
                } else {
                    self.nvim_conn.nvim_actions.close_instance_buffer(instance)
                }
            }
        }

        /// Opens each file provided as free arguments in separate buffers.
        /// Resets focus to initial buffer and window if further there will be created output buffer in split window,
        /// since we want to see shell from which that output buffer was spawned
        pub fn handle_display_files(&mut self) {
            let PageActions { nvim_conn: NeovimConnection { nvim_actions, initial_buf_number, initial_win_and_buf, .. }, nvim_ctx } = self;
            for f in &nvim_ctx.opt.files {
                if let Err(e) = nvim_actions.open_file_buffer(f) {
                    warn!("Error opening \"{}\": {}", f, e);
                } else {
                    let cmd = &nvim_ctx.opt.command.as_ref().map(String::as_ref).unwrap_or_default();
                    nvim_actions.prepare_file_buffer(cmd, *initial_buf_number);
                    if nvim_ctx.opt.follow_all {
                        nvim_actions.set_current_buffer_follow_output_mode();
                    } else {
                        nvim_actions.set_current_buffer_scroll_mode();
                    }
                }
            }
            if !nvim_ctx.opt.files.is_empty() && nvim_ctx.use_outp_buf_in_split {
                nvim_actions.switch_to_window_and_buffer(&initial_win_and_buf)
            }
        }

        /// Returns buffer marked as instance and path to PTY device associated with it.
        /// If neovim was spawned by page then it guaranteedly doesnt' have any instance,
        /// so in this case -i and -I arguments would be useless and warning would be printed about that
        pub fn find_instance_buffer(&mut self, inst_name: &str) -> Option<(Buffer, PathBuf)> {
            if self.nvim_ctx.nvim_child_proc_spawned {
                warn!("Newly spawned neovim cannot contain any instance (-i and -I are useless)");
            }
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
            let PageActions { nvim_conn: NeovimConnection { nvim_actions, initial_buf_number, .. }, nvim_ctx } = self;
            if nvim_ctx.use_outp_buf_in_split {
                nvim_actions.split_current_buffer(&nvim_ctx.opt);
            }
            let (buf, buf_pty_path) = nvim_actions.create_output_buffer_with_pty();
            nvim_actions.prepare_output_buffer(
                &nvim_ctx.page_id,
                &nvim_ctx.opt.filetype,
                &nvim_ctx.opt.command.as_ref().map(String::as_ref).unwrap_or_default(),
                nvim_ctx.opt.pwd,
                nvim_ctx.opt.query_lines,
                *initial_buf_number
            );
            (buf, buf_pty_path)
        }
    }
}


mod output {
    use crate::{context::OutputContext, neovim::{NeovimConnection, NotificationFromNeovim}, };
    use log::warn;
    use neovim_lib::neovim_api::Buffer;
    use std::{fs::{File, OpenOptions, }, io::{self, BufRead, BufReader, Write}, };

    /// This struct implements actions that should be done after output buffer is attached
    pub struct BufferActions<'a> {
        nvim_conn: &'a mut NeovimConnection,
        outp_ctx: &'a OutputContext,
        buf: Buffer,
        buf_pty: Option<File>,
    }

    pub fn begin_handling<'a>(nvim_conn: &'a mut NeovimConnection, outp_ctx: &'a OutputContext, buf: Buffer) -> BufferActions<'a> {
        BufferActions {
            nvim_conn,
            outp_ctx,
            buf,
            buf_pty: None,
        }
    }

    impl<'a> BufferActions<'a> {
        /// This function updates buffer title depending on its type [oneoff/instance] and -n value.
        /// Icon symbol is received from neovim side and prepended it to the left of buffer title.
        /// If instance name is provided then it's prepended to the left of icon symbol.
        pub fn handle_buffer_title(&mut self) {
            let BufferActions { outp_ctx, buf, nvim_conn: NeovimConnection { nvim_actions, .. }, .. } = self;
            if let Some(inst_name) = outp_ctx.inst_mode.any() {
                let (page_icon_key, page_icon_default) = ("page_icon_instance", "@ ");
                let mut buf_title = nvim_actions.get_var_or_default(page_icon_key, page_icon_default);
                buf_title.insert_str(0, inst_name);
                if let Some(ref buf_name) = outp_ctx.opt.name {
                    if buf_name != inst_name {
                        buf_title.push_str(buf_name);
                    }
                }
                nvim_actions.update_buffer_title(&buf, &buf_title);
            } else {
                let (page_icon_default, page_icon_key) = if outp_ctx.input_from_pipe { (" |", "page_icon_pipe") } else { (" >", "page_icon_redirect") };
                let mut buf_title = nvim_actions.get_var_or_default(page_icon_key, page_icon_default);
                if let Some(ref buf_name) = outp_ctx.opt.name {
                    buf_title.insert_str(0, buf_name);
                }
                nvim_actions.update_buffer_title(&buf, &buf_title);
            }
        }

        /// Resets focus and content of instance buffer when required.
        /// This is required to support some functionality not available through neovim API.
        pub fn handle_instance_buffer_reset(&mut self) {
            let BufferActions { outp_ctx, buf, nvim_conn: NeovimConnection { nvim_actions, .. }, .. } = self;
            if outp_ctx.inst_focus {
                nvim_actions.focus_instance_buffer(&buf);
            }
            if outp_ctx.inst_mode.is_replace() {
                writeln!(self.get_buffer_pty(), "\x1B[3J\x1B[H\x1b[2J").expect("Cannot write clear screen sequence");
            }
        }

        /// Executes PageConnect (-C) and post command (-E) on page buffer.
        /// If any of these flags are passed then output buffer should be already focused.
        pub fn handle_commands(&mut self) {
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
        pub fn handle_cursor_position(&mut self) {
            let BufferActions { outp_ctx, nvim_conn: NeovimConnection { nvim_actions, initial_win_and_buf, .. }, .. } = self;
            if outp_ctx.move_cursor {
                if outp_ctx.opt.follow {
                    nvim_actions.set_current_buffer_follow_output_mode();
                } else {
                    nvim_actions.set_current_buffer_scroll_mode();
                }
                if outp_ctx.switch_back_mode.is_any() {
                    nvim_actions.switch_to_window_and_buffer(&initial_win_and_buf);
                    if outp_ctx.switch_back_mode.is_insert() {
                        nvim_actions.set_current_buffer_insert_mode();
                    }
                }
            }
        }

        /// Writes lines from stdin directly into PTY device associated with output buffer.
        /// In case if -q <count> argument provided it might block until next line from neovim is requested.
        /// If page isn't piped then it simply prints actual path to PTY device associated with output buffer,
        /// so user can redirect into it manually.
        pub fn handle_output(&mut self) {
            if self.outp_ctx.input_from_pipe {
                let stdin = io::stdin();
                let n_lines_in_query = self.outp_ctx.opt.query_lines;
                if n_lines_in_query == 0 {
                    io::copy(&mut io::stdin().lock(), self.get_buffer_pty()).expect("Read from stdin failed unexpectedly");
                } else {
                    let mut stdin_lines = BufReader::new(stdin.lock()).lines();
                    let mut s = QueryState::default();
                    s.next_part(n_lines_in_query);
                    loop {
                        if s.is_whole_part_sent() {
                            self.nvim_conn.nvim_actions.notify_query_finished(s.how_many_lines_sent());
                            match self.nvim_conn.rx.recv() {
                                Ok(NotificationFromNeovim::FetchLines(n)) => s.next_part(n),
                                Ok(NotificationFromNeovim::FetchPart) => s.next_part(n_lines_in_query),
                                _ => break
                            }
                        }
                        match stdin_lines.next() {
                            Some(Ok(l)) => writeln!(self.get_buffer_pty(), "{}", l).expect("Cannot write next line"),
                            Some(Err(e)) => {
                                warn!("Error reading line from stdin: {}", e);
                                break;
                            }
                            None => {
                                self.nvim_conn.nvim_actions.notify_query_finished(s.how_many_lines_sent());
                                break;
                            }
                        }
                        s.line_sent();
                    }
                }
                self.nvim_conn.nvim_actions.notify_page_read();
            }
            if self.outp_ctx.print_output_buf_pty {
                println!("{}", self.outp_ctx.buf_pty_path.to_string_lossy());
            }
        }

        /// Executes PageDisconnect autocommand if -C flag was provided.
        /// Some time might pass since page buffer was created and output was started,
        /// so this function might temporarily focus on output buffer in order to run
        /// autocommand.
        pub fn handle_disconnect(&mut self) {
            let BufferActions { nvim_conn: NeovimConnection { nvim_actions, initial_win_and_buf, .. }, buf, outp_ctx, .. } = self;
            if outp_ctx.opt.command_auto {
                let c_buf = nvim_actions.get_current_buffer();
                let switched = buf != &c_buf;
                if switched {
                    nvim_actions.switch_to_buffer(&buf);
                }
                nvim_actions.execute_disconnect_autocmd_on_current_buffer();
                if switched {
                    nvim_actions.switch_to_buffer(&c_buf);
                    if initial_win_and_buf.1 == c_buf && outp_ctx.switch_back_mode.is_insert() {
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
        fn how_many_lines_sent(&self) -> u64 {
            self.expect - self.remain
        }
    }
}
