/// A module for actions done with neovim

use crate::cli::Options;

use log::{error, trace, warn};
use neovim_lib::{
    neovim_api::{Buffer, Window},
    NeovimApi, Value,
};
use std::{
    path::PathBuf,
    sync::mpsc,
};


/// This struct wraps neovim_lib::Neovim in order to enhance it with methods required in page.
/// Results returned from underlying Neovim methods are mostly unwrapped, since we anyway cannot provide
/// any meaningful falback logic on call side.
pub struct NeovimActions {
    nvim: neovim_lib::Neovim,
}

fn extend_neovim(nvim: neovim_lib::Neovim) -> NeovimActions {
    NeovimActions { nvim }
}

impl NeovimActions {
    pub fn get_current_window_and_buffer(&mut self) -> (Window, Buffer) {
        (self.nvim.get_current_win().unwrap(), self.nvim.get_current_buf().unwrap())
    }

    pub fn get_current_buffer(&mut self) -> Buffer {
        self.nvim.get_current_buf().unwrap()
    }

    pub fn get_buffer_number(&mut self, buf: &Buffer) -> i64 {
        buf.get_number(&mut self.nvim).unwrap()
    }

    pub fn create_output_buffer_with_pty(&mut self) -> (Buffer, PathBuf) {
        self.nvim.command("term tail -f <<EOF").unwrap();
        let buf = self.get_current_buffer();
        let buf_pty_path: PathBuf = self.nvim.eval("nvim_get_chan_info(&channel)").expect("Cannot get channel info")
            .as_map().unwrap()
            .iter().find(|(k, _)| k.as_str().map(|s| s == "pty").unwrap()).expect("Cannot find 'pty' on channel info")
            .1.as_str().unwrap()
            .into();
        trace!(target: "new output buffer", "{} => {}", self.get_buffer_number(&buf), buf_pty_path.display());
        (buf, buf_pty_path)
    }

    pub fn mark_buffer_as_instance(&mut self, buffer: &Buffer, inst_name: &str, inst_pty_path: &str) {
        trace!(target: "register instance buffer", "{:?}->{}->{}", buffer, inst_name, inst_pty_path);
        let v = Value::from(vec![Value::from(inst_name), Value::from(inst_pty_path)]);
        if let Err(e) = buffer.set_var(&mut self.nvim, "page_instance", v) {
            error!("Error when setting instance mark: {}", e);
        }
    }

    pub fn find_instance_buffer(&mut self, inst_name: &str) -> Option<(Buffer, PathBuf)> {
        for buf in self.nvim.list_bufs().unwrap() {
            let inst_var = buf.get_var(&mut self.nvim, "page_instance");
            trace!(target: "find instance", "{:?} => {}: {:?}", buf.get_number(&mut self.nvim), inst_name, inst_var);
            match inst_var {
                Err(e) => {
                    let descr = e.to_string();
                    if descr != "1 - Key 'page_instance' not found"
                    && descr != "1 - Key not found: page_instance" { // for new neovim version
                        panic!("Error when getting instance mark: {}", e);
                    }
                }
                Ok(v) => {
                    if let Some(arr) = v.as_array().map(|a|a.iter().map(Value::as_str).collect::<Vec<_>>()) {
                        if let [Some(inst_name_found), Some(inst_pty_path)] = arr[..] {
                            trace!(target: "found instance", "{}->{}", inst_name_found, inst_pty_path);
                            if inst_name == inst_name_found {
                                let sink = PathBuf::from(inst_pty_path.to_string());
                                return Some((buf, sink))
                            }
                        }
                    }
                }
            }
        };
        None
    }

    pub fn close_instance_buffer(&mut self, inst_name: &str) {
        trace!(target: "close instance buffer", "{}", inst_name);
        if let Some((buf, _)) = self.find_instance_buffer(&inst_name) {
            if let Err(e) = buf.get_number(&mut self.nvim).and_then(|inst_id| self.nvim.command(&format!("exe 'bd!' . {}", inst_id))) {
                error!("Error when closing instance buffer: {}, {}", inst_name, e);
            }
        }
    }

    pub fn focus_instance_buffer(&mut self, inst_buf: &Buffer) {
        trace!(target: "focus instance buffer", "{:?}", inst_buf);
        if &self.get_current_buffer() != inst_buf {
            for win in self.nvim.list_wins().unwrap() {
                trace!(target: "focus instance buffer", "check window: {:?}", win.get_number(&mut self.nvim));
                if &win.get_buf(&mut self.nvim).unwrap() == inst_buf {
                    trace!(target: "focus instance buffer", "set last window");
                    self.nvim.set_current_win(&win).unwrap();
                    return;
                }
            }
        } else {
            trace!(target: "focus instance buffer", "not from window");
        }
        self.nvim.set_current_buf(inst_buf).unwrap();
    }

    pub fn split_current_buffer(&mut self, opt: &Options) {
        trace!(target: "split", "");
        let e = "Error when splitting current buffer";
        let ratio = |buf_size, n| buf_size * 3 / (u64::from(n) + 1);
        if opt.split_right > 0 {
            self.nvim.command("belowright vsplit").expect(e);
            let buf_width = self.nvim.call_function("winwidth", vec![Value::from(0)]).unwrap().as_u64().unwrap();
            self.nvim.command(&format!("vertical resize {} | set wfw", ratio(buf_width, opt.split_right))).expect(e);
        } else if opt.split_left > 0 {
            self.nvim.command("aboveleft vsplit").expect(e);
            let buf_width = self.nvim.call_function("winwidth", vec![Value::from(0)]).unwrap().as_u64().unwrap();
            self.nvim.command(&format!("vertical resize {} | set wfw", ratio(buf_width, opt.split_left))).expect(e);
        } else if opt.split_below > 0 {
            self.nvim.command("belowright split").expect(e);
            let buf_height = self.nvim.call_function("winheight", vec![Value::from(0)]).unwrap().as_u64().unwrap();
            self.nvim.command(&format!("resize {} | set wfh", ratio(buf_height, opt.split_below))).expect(e);
        } else if opt.split_above > 0 {
            self.nvim.command("aboveleft split").expect(e);
            let buf_height = self.nvim.call_function("winheight", vec![Value::from(0)]).unwrap().as_u64().unwrap();
            self.nvim.command(&format!("resize {} | set wfh", ratio(buf_height, opt.split_above))).expect(e);
        } else if let Some(split_right_cols) = opt.split_right_cols {
            self.nvim.command(&format!("belowright vsplit | vertical resize {} | set wfw", split_right_cols)).expect(e);
        } else if let Some(split_left_cols) = opt.split_left_cols {
            self.nvim.command(&format!("aboveleft vsplit | vertical resize {} | set wfw", split_left_cols)).expect(e);
        } else if let Some(split_below_rows) = opt.split_below_rows {
            self.nvim.command(&format!("belowright split | resize {} | set wfh", split_below_rows)).expect(e);
        } else if let Some(split_above_rows) = opt.split_above_rows {
            self.nvim.command(&format!("aboveleft split | resize {} | set wfh", split_above_rows)).expect(e);
        }
    }

    pub fn update_buffer_title(&mut self, buf: &Buffer, buf_title: &str) {
        trace!(target: "set title", "{:?} => {}", buf.get_number(&mut self.nvim), buf_title);
        let a = std::iter::once((0, buf_title.to_string()));
        let b = (1..99).map(|attem| (attem, format!("{}({})", buf_title, attem)));
        for (attempt_nr, name) in a.chain(b) {
            match buf.set_name(&mut self.nvim, &name) {
                Err(e) => {
                    trace!(target: "set title", "{:?} => {}: {:?}", buf.get_number(&mut self.nvim), buf_title, e);
                    if 99 < attempt_nr || e.to_string() != "0 - Failed to rename buffer" {
                        error!("Cannot update buffer title '{}': {}", buf_title, e);
                        return;
                    }
                }
                _ => {
                    self.nvim.command("redraw!").unwrap();  // To update statusline
                    return;
                }
            }
        }
    }

    pub fn prepare_file_buffer(&mut self, cmd_user: &str, initial_buf_nr: i64) {
        let au = "| exe 'silent doautocmd User PageOpenFile'";
        self.prepare_current_buffer("", cmd_user, "", au, initial_buf_nr)
    }

    pub fn prepare_output_buffer(&mut self, page_id: &str, ft: &str, cmd_user: &str, pwd: bool, query_lines: u64, initial_buf_nr: i64) {
        let ft = format!("filetype={}", ft);
        let mut cmd_pre = String::new();
        if 0u64 < query_lines {
            cmd_pre.push_str(&format!("\
                | exe 'command! -nargs=? Page call rpcnotify(0, ''page_fetch_lines'', ''{page_id}'', <args>)' \
                | exe 'autocmd BufEnter <buffer> command! -nargs=? Page call rpcnotify(0, ''page_fetch_lines'', ''{page_id}'', <args>)' \
                | exe 'autocmd BufDelete <buffer> call rpcnotify(0, ''page_buffer_closed'', ''{page_id}'')' \
            ",
                page_id = page_id,
            ));
        }
        if pwd {
            cmd_pre.push_str(&format!("\
                | let b:page_lcd_backup = getcwd() \
                | lcd {pwd} \
                | exe 'autocmd BufEnter <buffer> lcd {pwd}' \
                | exe 'autocmd BufLeave <buffer> lcd ' .. b:page_lcd_backup \
            ",
                pwd = std::env::var("PWD").unwrap()
            ));
        }
        self.prepare_current_buffer(&ft, cmd_user, &cmd_pre, "", initial_buf_nr)
    }

    fn prepare_current_buffer(&mut self, ft: &str, cmd_user: &str, cmd_pre: &str, cmd_post: &str, initial_buf_nr: i64) {
        let cmd_user = if cmd_user.is_empty() {
            String::new()
        } else {
            format!("exe '{}'", cmd_user.replace("'", "''")) // Ecranizes viml literal string
        };
        let options = &format!(" \
            | let b:page_alternate_bufnr={initial_buf_nr} \
            | let b:page_scrolloff_backup=&scrolloff \
            | setl scrollback=-1 scrolloff=999 signcolumn=no nonumber nomodifiable {ft} \
            | exe 'autocmd BufEnter <buffer> set scrolloff=999' \
            | exe 'autocmd BufLeave <buffer> let &scrolloff=b:page_scrolloff_backup' \
            {cmd_pre}\
            | exe 'silent doautocmd User PageOpen' \
            | redraw \
            {cmd_user}\
            {cmd_post}\
        ",
            initial_buf_nr = initial_buf_nr,
            ft = ft,
            cmd_user = cmd_user,
            cmd_pre = cmd_pre,
            cmd_post = cmd_post,
        );
        trace!(target: "set page options", "{}", &options);
        if let Err(e) = self.nvim.command(options) {
            error!("Unable to set page options to current buffer, text might be displayed improperly: {}", e);
        }
    }

    pub fn execute_connect_autocmd_on_current_buffer(&mut self) {
        trace!(target: "autocmd PageConnect", "");
        if let Err(e) = self.nvim.command("silent doautocmd User PageConnect") {
            error!("Cannot execute PageConnect: {}", e);
        }
    }

    pub fn execute_disconnect_autocmd_on_current_buffer(&mut self) {
        trace!(target: "autocmd PageDisconnect", "");
        if let Err(e) = self.nvim.command("silent doautocmd User PageDisconnect") {
            error!("Cannot execute PageDisconnect: {}", e);
        }
    }

    pub fn execute_command_post(&mut self, cmd: &str) {
        trace!(target: "exec command_post", "{}", cmd);
        if let Err(e) = self.nvim.command(cmd) {
            error!("Error when executing post command '{}': {}", cmd, e);
        }
    }

    pub fn switch_to_window_and_buffer(&mut self, (win, buf): &(Window, Buffer)) {
        trace!(target: "switch window and buffer", "win:{:?} buf:{:?}",  win.get_number(&mut self.nvim), buf.get_number(&mut self.nvim));
        if let Err(e) = self.nvim.set_current_win(win) {
            warn!("Can't switch to window: {}", e);
        }
        if let Err(e) = self.nvim.set_current_buf(buf) {
            warn!("Can't switch to buffer: {}", e);
        }
    }

    pub fn switch_to_buffer(&mut self, buf: &Buffer) {
        trace!(target: "switch buffer", "buf:{:?}", buf.get_number(&mut self.nvim));
        self.nvim.set_current_buf(buf).unwrap();
    }

    pub fn set_current_buffer_insert_mode(&mut self) {
        trace!(target: "set mode: INSERT", "");
        if let Err(e) = self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>A", 'n')"###) {// Fixes "can't enter normal mode from..."
            error!("Error when setting mode: {}", e);
        }
    }

    pub fn set_current_buffer_follow_output_mode(&mut self) {
        trace!(target: "set mode: FOLLOW", "");
        if let Err(e) = self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>G, 'n'")"###) {
            error!("Error when setting mode: {}", e);
        }
    }

    pub fn set_current_buffer_scroll_mode(&mut self) {
        trace!(target: "set mode: SCROLL", "");
        if let Err(e) = self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>ggM, 'n'")"###) {
            error!("Error when setting mode: {}", e);
        }
    }

    pub fn open_file_buffer(&mut self, file: &str) -> Result<(), Box<dyn std::error::Error>> {
        trace!(target: "open file", "{}", file);
        self.nvim.command(&format!("e {}", std::fs::canonicalize(file)?.to_string_lossy()))?;
        Ok(())
    }

    pub fn notify_query_finished(&mut self, lines_read: u64) {
        self.nvim.command(&format!("redraw | echoh Comment | echom '-- [PAGE] {} lines read; has more --' | echoh None", lines_read)).unwrap();
    }

    pub fn notify_page_read(&mut self) {
        self.nvim.command("redraw | echoh Comment | echom '-- [PAGE] end of input --' | echoh None").unwrap();
    }

    pub fn get_var_or_default(&mut self, key: &str, default: &str) -> String {
        self.nvim.get_var(key)
            .map(|v| v.to_string())
            .unwrap_or_else(|e| {
                let description = e.to_string();
                if description != format!("1 - Key '{}' not found", key)
                && description != format!("1 - Key not found: {}", key) { // for new neovim version
                    error!("Error when getting var: {}, {}", key, e);
                }
                String::from(default)
            })
    }
}



/// This is type-safe enumeration of notifications that could be done from neovim side.
/// Maybe it'll be enhanced in future
pub enum NotificationFromNeovim {
    FetchPart,
    FetchLines(u64),
    BufferClosed,
}

/// Registers handler which receives notifications from neovim side.
/// Commands are received on separate thread and further redirected to mpsc sender
/// associated with receiver returned from current function.
pub fn subscribe_to_page_notifications(nvim: &mut neovim_lib::Neovim, page_id: &str) -> mpsc::Receiver<NotificationFromNeovim> {
    trace!(target: "subscribe to notifications", "id: {}", page_id);
    let (tx, rx) = mpsc::sync_channel(16);
    nvim.session.start_event_loop_handler(listening::NotificationReceiver { tx, page_id: page_id.to_string() });
    nvim.subscribe("page_fetch_lines").unwrap();
    nvim.subscribe("page_buffer_closed").unwrap();
    rx
}

mod listening {
    use super::NotificationFromNeovim;
    use log::{trace, warn};
    use neovim_lib::Value;
    use std::sync::mpsc;

    /// Receives and collects notifications from neovim side
    pub(super) struct NotificationReceiver {
        pub tx: mpsc::SyncSender<NotificationFromNeovim>,
        pub page_id: String,
    }

    impl neovim_lib::Handler for NotificationReceiver {
        fn handle_notify(&mut self, name: &str, args: Vec<Value>) {
            trace!(target: "notification", "{}: {:?} ", name, args);
            let valid_page_id = || args.get(0).and_then(Value::as_str).map_or(false, |v| v == self.page_id);
            match name {
                "page_fetch_lines" if valid_page_id() => {
                    if let Some(lines_count) = args.get(1).map(|v| v.as_u64().unwrap()) {
                        self.tx.send(NotificationFromNeovim::FetchLines(lines_count)).unwrap();
                    } else {
                        self.tx.send(NotificationFromNeovim::FetchPart).unwrap();
                    }
                }
                "page_buffer_closed" if valid_page_id() => {
                    self.tx.send(NotificationFromNeovim::BufferClosed).unwrap();
                }
                _ => {
                    warn!(target: "unhandled notification", "{}: {:?}", name, args);
                }
            }
        }
    }

    impl neovim_lib::RequestHandler for NotificationReceiver {
        fn handle_request(&mut self, name: &str, args: Vec<Value>) -> Result<Value, Value> {
            warn!(target: "unhandled request", "{}: {:?}", name, args);
            Ok(Value::from(0))
        }
    }
}



/// This struct contains all neovim-related data which is required by page
/// after connection with neovim is established.
pub struct NeovimConnection {
    pub nvim_proc: Option<std::process::Child>,
    pub nvim_actions: NeovimActions,
    pub initial_win_and_buf: (neovim_lib::neovim_api::Window, neovim_lib::neovim_api::Buffer),
    pub initial_buf_number: i64,
    pub rx: mpsc::Receiver<NotificationFromNeovim>,
}

/// Connects to parent neovim session if possible or spawns new child neovim process and connects to it through socket.
/// Replacement for `neovim_lib::Session::new_child()`, since it uses --embed flag and steals page stdin.
pub fn create_connection(cli_ctx: &crate::context::CliContext) -> NeovimConnection {
    let (nvim_session, nvim_proc) = connection::create_session(&cli_ctx);
    let mut nvim = neovim_lib::Neovim::new(nvim_session);
    let rx = subscribe_to_page_notifications(&mut nvim, &cli_ctx.page_id);
    let mut nvim_actions = extend_neovim(nvim);
    let initial_win_and_buf = nvim_actions.get_current_window_and_buffer();
    let initial_buf_number = nvim_actions.get_buffer_number(&initial_win_and_buf.1);
    NeovimConnection {
        nvim_proc,
        nvim_actions,
        initial_win_and_buf,
        initial_buf_number,
        rx,
    }
}

/// Waits until child neovim closes. If no child neovim process then it's safe to exit from page
pub fn close_connection(nvim_connection: NeovimConnection) {
    if let Some(mut process) = nvim_connection.nvim_proc {
        process.wait().expect("Neovim process died unexpectedly");
    }
}

mod connection {
    use crate::{cli::Options, context::CliContext};
    use std::{path::PathBuf, process};
    use log::{error, trace};

    /// Creates a new session using TCP or UNIX socket, or fallbacks to a new neovim process
    /// Also prints redirection protection in appropriate circumstances.
    pub(super) fn create_session(pre_ctx: &CliContext,) -> (neovim_lib::Session, Option<process::Child>) {
        let CliContext { opt, tmp_dir, page_id, print_protection, .. } = pre_ctx;
        match opt.address.as_ref().map(String::as_ref).map(session_at_address) {
            Some(Ok(nvim_session)) => return (nvim_session, None),
            Some(Err(e)) => error!(target: "cannot connect to parent neovim", "address '{}': {:?}", opt.address.as_ref().unwrap(), e),
            _ => {}
        }
        if *print_protection {
            print_redirect_protection(&tmp_dir);
        }
        let p = tmp_dir.clone().join(&format!("socket-{}", page_id));
        let nvim_listen_addr = p.to_string_lossy();
        let nvim_proc = spawn_child_nvim_process(opt, &nvim_listen_addr);
        let mut i = 100;
        let e = loop {
            match session_at_address(&nvim_listen_addr) {
                Ok(nvim_session) => return (nvim_session, Some(nvim_proc)),
                Err(e) => {
                    if let std::io::ErrorKind::NotFound = e.kind() {
                        if i == 0 {
                            break e;
                        } else {
                            trace!(target: "cannot connect to child neovim", "[attempts: {}] address '{}': {:?}", i, nvim_listen_addr, e);
                            std::thread::sleep(std::time::Duration::from_millis(16));
                            i -= 1;
                        }
                    } else {
                        break e;
                    }
                }
            }
        };
        panic!("Cannot connect to neovim: {:?}", e);
    }

    /// Redirecting protection prevents from producing junk or corruption of existed files
    /// by invoking commands like "unset NVIM_LISTEN_ADDRESS && ls > $(page -E q)" where "$(page -E q)"
    /// evaluates not into /path/to/sink as expected but into neovim UI instead. It consists of
    /// a bunch of characters and strings, so many useless files may be created and even overwriting
    /// of existed files might occur if their name would match. To prevent that, a path to dumb directory
    /// is printed first before neovim process was spawned. This expands to "cli > dir {neovim UI}"
    /// command which fails early as redirecting text into directory is impossible.
    fn print_redirect_protection(tmp_dir: &PathBuf) {
        let d = tmp_dir.clone().join("DO-NOT-REDIRECT-OUTSIDE-OF-NVIM-TERM(--help[-W])");
        if let Err(e) = std::fs::create_dir_all(&d) {
            panic!("Cannot create protection directory '{}': {:?}", d.display(), e)
        }
        println!("{}", d.to_string_lossy());
    }

    /// Spawns child neovim process and connects to it using socket.
    /// This not uses neovim's "--embed" flag, so neovim UI is displayed properly on top of page.
    /// Also this neovim child process doesn't inherits page stdin, therefore
    /// page is able to operate on its input and redirect it into a proper target.
    /// Also custom neovim config would be used if it's present in corresponding location.
    fn spawn_child_nvim_process(opt: &Options, nvim_listen_addr: &str) -> process::Child {
        let nvim_args = {
            let mut a = String::new();
            a.push_str("--cmd 'set shortmess+=I' ");
            a.push_str("--listen ");
            a.push_str(nvim_listen_addr);
            if let Some(config) = opt.config.clone().or_else(default_config_path) {
                a.push(' ');
                a.push_str("-u ");
                a.push_str(&config);
            }
            if let Some(custom_args) = opt.arguments.as_ref() {
                a.push(' ');
                a.push_str(custom_args);
            }
            shell_words::split(&a).expect("Cannot parse neovim arguments")
        };
        trace!(target: "New neovim process", "args: {:?}", nvim_args);
        process::Command::new("nvim").args(&nvim_args)
            .stdin(process::Stdio::null())
            .spawn()
            .expect("Cannot spawn a child neovim process")
    }

    /// Returns path to custom neovim config if it's present in corresponding locations.
    fn default_config_path() -> Option<String> {
        std::env::var("XDG_CONFIG_HOME").ok().and_then(|xdg_config_home| {
            let p = PathBuf::from(xdg_config_home).join("page/init.vim");
            if p.exists() {
                trace!(target: "default config", "use $XDG_CONFIG_HOME: {}", p.display());
                Some(p)
            } else {
                None
            }
        })
        .or_else(|| std::env::var("HOME").ok().and_then(|home_dir| {
            let p = PathBuf::from(home_dir).join(".config/page/init.vim");
            if p.exists() {
                trace!(target: "default config", "use ~/.config: {}", p.display());
                Some(p)
            } else {
                None
            }
        }))
        .map(|p| p.to_string_lossy().to_string())
    }

    /// Returns neovim session either backed by TCP or UNIX socket
    fn session_at_address(nvim_listen_addr: &str) -> std::io::Result<neovim_lib::Session> {
        let session = match nvim_listen_addr.parse::<std::net::SocketAddr>() {
            Ok (_) => neovim_lib::Session::new_tcp(nvim_listen_addr)?,
            Err(_) => neovim_lib::Session::new_unix_socket(nvim_listen_addr)?,
        };
        Ok(session)
    }
}
