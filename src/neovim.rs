/// A module that extends neovim api with methods required in page


use neovim_lib::{
    neovim_api::{Buffer, Window},
    NeovimApi, Value,
};
use std::{
    path::PathBuf,
    sync::mpsc,
};


const TERM_URI: &str = concat!(
    "term://",     // Shell will be temporarily replaced with /bin/sleep
    "2147483647d"  // This will sleep for i32::MAX days
);

/// This struct wraps neovim_lib::Neovim and decorates it with methods required in page.
/// Results returned from underlying Neovim methods are mostly unwrapped, since we anyway cannot provide
/// any meaningful falback logic on call side
pub struct NeovimActions {
    nvim: neovim_lib::Neovim,
}

impl NeovimActions {
    pub fn on(nvim: neovim_lib::Neovim) -> NeovimActions {
        NeovimActions { nvim }
    }

    pub fn get_current_window_and_buffer(&mut self) -> (Window, Buffer) {
        (self.nvim.get_current_win().unwrap(), self.nvim.get_current_buf().unwrap())
    }

    pub fn get_current_buffer(&mut self) -> Buffer {
        self.nvim.get_current_buf().unwrap()
    }

    pub fn get_buffer_number(&mut self, buf: &Buffer) -> i64 {
        buf.get_number(&mut self.nvim).unwrap()
    }

    pub fn create_substituting_output_buffer(&mut self) -> Buffer {
        self.create_buffer(&format!("e {}", TERM_URI))
    }

    pub fn create_split_output_buffer(&mut self, opt: &crate::cli::SplitOptions) -> Buffer {
        let (ver_ratio, hor_ratio) = ("(winwidth(0) / 2) * 3", "(winheight(0) / 2) * 3");
        let cmd = if opt.split_right != 0u8 {
            format!("exe 'belowright ' . ({}/{}) . 'vsplit {}' | set winfixwidth", ver_ratio, opt.split_right + 1, TERM_URI)
        } else if opt.split_left != 0u8 {
            format!("exe 'aboveleft ' . ({}/{}) . 'vsplit {}' | set winfixwidth", ver_ratio, opt.split_left + 1, TERM_URI)
        } else if opt.split_below != 0u8 {
            format!("exe 'belowright ' . ({}/{}) . 'split {}' | set winfixheight", hor_ratio, opt.split_below + 1, TERM_URI)
        } else if opt.split_above != 0u8 {
            format!("exe 'aboveleft ' . ({}/{}) . 'split {}' | set winfixheight", hor_ratio, opt.split_above + 1, TERM_URI)
        } else if let Some(split_right_cols) = opt.split_right_cols {
            format!("belowright {}vsplit {} | set winfixwidth", split_right_cols, TERM_URI)
        } else if let Some(split_left_cols) = opt.split_left_cols {
            format!("aboveleft {}vsplit {} | set winfixwidth", split_left_cols, TERM_URI)
        } else if let Some(split_below_rows) = opt.split_below_rows {
            format!("belowright {}split {} | set winfixheight", split_below_rows, TERM_URI)
        } else if let Some(split_above_rows) = opt.split_above_rows {
            format!("aboveleft {}split {} | set winfixheight", split_above_rows, TERM_URI)
        } else {
            unreachable!()
        };
        self.create_buffer(&cmd)
    }

    fn create_buffer(&mut self, term_open_cmd: &str) -> Buffer {
        let cmd = format!(" \
            | let g:page_shell_backup = [&shell, &shellcmdflag] \
            | let [&shell, &shellcmdflag] = ['/bin/sleep', ''] \
            | {term_open_cmd} \
            | let [&shell, &shellcmdflag] = g:page_shell_backup \
        ",
            term_open_cmd = term_open_cmd
        );
        log::trace!(target: "create buffer", "{}", cmd);
        self.nvim.command(&cmd).expect("Error when creating split output buffer");
        let buf = self.get_current_buffer();
        log::trace!(target: "create buffer", "created: {}", self.get_buffer_number(&buf));
        buf
    }

    pub fn get_current_buffer_pty_path(&mut self) -> PathBuf {
        let buf_pty_path: PathBuf = self.nvim.eval("nvim_get_chan_info(&channel)").expect("Cannot get channel info")
            .as_map().unwrap()
            .iter().find(|(k, _)| k.as_str().map(|s| s == "pty").unwrap()).expect("Cannot find 'pty' on channel info")
            .1.as_str().unwrap()
            .into();
        log::trace!(target: "use pty", "{}", buf_pty_path.display());
        buf_pty_path
    }

    pub fn mark_buffer_as_instance(&mut self, buffer: &Buffer, inst_name: &str, inst_pty_path: &str) {
        log::trace!(target: "new instance", "{:?}->{}->{}", buffer, inst_name, inst_pty_path);
        let v = Value::from(vec![Value::from(inst_name), Value::from(inst_pty_path)]);
        if let Err(e) = buffer.set_var(&mut self.nvim, "page_instance", v) {
            log::error!(target: "new instance", "Error when setting instance mark: {}", e);
        }
    }

    pub fn find_instance_buffer(&mut self, inst_name: &str) -> Option<(Buffer, PathBuf)> {
        for buf in self.nvim.list_bufs().unwrap() {
            let inst_var = buf.get_var(&mut self.nvim, "page_instance");
            log::trace!(target: "instances", "{:?} => {}: {:?}", buf.get_number(&mut self.nvim), inst_name, inst_var);
            match inst_var {
                Err(e) => {
                    let descr = e.to_string();
                    if descr != "1 - Key 'page_instance' not found"
                    && descr != "1 - Key not found: page_instance" { // For newer neovim versions
                        panic!("Error when getting instance mark: {}", e);
                    }
                }
                Ok(v) => {
                    if let Some(arr) = v.as_array().map(|a|a.iter().map(Value::as_str).collect::<Vec<_>>()) {
                        if let [Some(inst_name_found), Some(inst_pty_path)] = arr[..] {
                            log::trace!(target: "found instance", "{}->{}", inst_name_found, inst_pty_path);
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
        log::trace!(target: "close instance", "{}", inst_name);
        if let Some((buf, _)) = self.find_instance_buffer(&inst_name) {
            if let Err(e) = buf.get_number(&mut self.nvim).and_then(|inst_id| self.nvim.command(&format!("exe 'bd!' . {}", inst_id))) {
                log::error!(target: "close instance", "Error when closing instance buffer: {}, {}", inst_name, e);
            }
        }
    }

    pub fn focus_instance_buffer(&mut self, inst_buf: &Buffer) {
        log::trace!(target: "focus instance", "{:?}", inst_buf);
        if &self.get_current_buffer() != inst_buf {
            let wins_open = self.nvim.list_wins().unwrap();
            log::trace!(target: "focus instance", "Winows open: {:?}", wins_open.iter().map(|w| w.get_number(&mut self.nvim)));
            for win in wins_open {
                if &win.get_buf(&mut self.nvim).unwrap() == inst_buf {
                    log::trace!(target: "focus instance", "Use window: {:?}", win.get_number(&mut self.nvim));
                    self.nvim.set_current_win(&win).unwrap();
                    return
                }
            }
        } else {
            log::trace!(target: "focus instance", "Not in window");
        }
        self.nvim.set_current_buf(inst_buf).unwrap();
    }


    pub fn update_buffer_title(&mut self, buf: &Buffer, buf_title: &str) {
        log::trace!(target: "update title", "{:?} => {}", buf.get_number(&mut self.nvim), buf_title);
        let a = std::iter::once((0, buf_title.to_string()));
        let b = (1..99).map(|attempt_nr| (attempt_nr, format!("{}({})", buf_title, attempt_nr)));
        for (attempt_nr, name) in a.chain(b) {
            match buf.set_name(&mut self.nvim, &name) {
                Err(e) => {
                    log::trace!(target: "update title", "{:?} => {}: {:?}", buf.get_number(&mut self.nvim), buf_title, e);
                    if 99 < attempt_nr || e.to_string() != "0 - Failed to rename buffer" {
                        log::error!(target: "update title", "Cannot update title: {}", e);
                        return
                    }
                }
                _ => {
                    self.nvim.command("redraw!").unwrap(); // To update statusline
                    return
                }
            }
        }
    }

    pub fn prepare_output_buffer(&mut self, initial_buf_nr: i64, cmds: OutputCommands) {
        let options = format!(" \
            | let b:page_alternate_bufnr={initial_buf_nr} \
            | let b:page_scrolloff_backup=&scrolloff \
            | setl scrollback=100000 scrolloff=999 signcolumn=no nonumber {ft} \
            {cmd_edit} \
            | exe 'autocmd BufEnter <buffer> setl scrolloff=999' \
            | exe 'autocmd BufLeave <buffer> let &scrolloff=b:page_scrolloff_backup' \
            {cmd_pre} \
            | exe 'silent doautocmd User PageOpen' \
            | redraw \
            {cmd_provided_by_user} \
            {cmd_post} \
        ",
            ft = cmds.ft,
            initial_buf_nr = initial_buf_nr,
            cmd_edit = cmds.edit,
            cmd_provided_by_user = cmds.provided_by_user,
            cmd_pre = cmds.pre,
            cmd_post = cmds.post,
        );
        log::trace!(target: "prepare output", "{}", options);
        if let Err(e) = self.nvim.command(&options) {
            log::error!(target: "prepare output", "Unable to set page options, text might be displayed improperly: {}", e);
        }
    }

    pub fn execute_connect_autocmd_on_current_buffer(&mut self) {
        log::trace!(target: "au PageConnect", "");
        if let Err(e) = self.nvim.command("silent doautocmd User PageConnect") {
            log::error!(target: "au PageConnect", "Cannot execute PageConnect: {}", e);
        }
    }

    pub fn execute_disconnect_autocmd_on_current_buffer(&mut self) {
        log::trace!(target: "au PageDisconnect", "");
        if let Err(e) = self.nvim.command("silent doautocmd User PageDisconnect") {
            log::error!(target: "au PageDisconnect", "Cannot execute PageDisconnect: {}", e);
        }
    }

    pub fn execute_command_post(&mut self, cmd: &str) {
        log::trace!(target: "command post", "{}", cmd);
        if let Err(e) = self.nvim.command(cmd) {
            log::error!(target: "command post", "Error when executing post command '{}': {}", cmd, e);
        }
    }

    pub fn switch_to_window_and_buffer(&mut self, (win, buf): &(Window, Buffer)) {
        log::trace!(target: "set window and buffer", "Win:{:?} Buf:{:?}",  win.get_number(&mut self.nvim), buf.get_number(&mut self.nvim));
        if let Err(e) = self.nvim.set_current_win(win) {
            log::error!(target: "set window and buffer", "Can't switch to window: {}", e);
        }
        if let Err(e) = self.nvim.set_current_buf(buf) {
            log::error!(target: "set window and buffer", "Can't switch to buffer: {}", e);
        }
    }

    pub fn switch_to_buffer(&mut self, buf: &Buffer) {
        log::trace!(target: "set buffer", "{:?}", buf.get_number(&mut self.nvim));
        self.nvim.set_current_buf(buf).unwrap();
    }

    pub fn set_current_buffer_insert_mode(&mut self) {
        log::trace!(target: "set INSERT", "");
        if let Err(e) = self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>A", 'n')"###) {// Fixes "can't enter normal mode from..."
            log::error!(target: "set INSERT", "Error when setting mode: {}", e);
        }
    }

    pub fn set_current_buffer_follow_output_mode(&mut self) {
        log::trace!(target: "set FOLLOW", "");
        if let Err(e) = self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>G, 'n'")"###) {
            log::error!(target: "set FOLLOW", "Error when setting mode: {}", e);
        }
    }

    pub fn set_current_buffer_scroll_mode(&mut self) {
        log::trace!(target: "set SCROLL", "");
        if let Err(e) = self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>ggM, 'n'")"###) {
            log::error!(target: "set SCROLL", "Error when setting mode: {}", e);
        }
    }

    pub fn open_file_buffer(&mut self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        log::trace!(target: "open file", "{}", file_path);
        self.nvim.command(&format!("e {}", std::fs::canonicalize(file_path)?.to_string_lossy()))?;
        Ok(())
    }

    pub fn notify_query_finished(&mut self, lines_read: usize) {
        log::trace!(target: "query finished", "Read {} lines", lines_read);
        self.nvim.command(&format!("redraw | echoh Comment | echom '-- [PAGE] {} lines read; has more --' | echoh None", lines_read)).unwrap();
    }

    pub fn notify_end_of_input(&mut self) {
        log::trace!(target: "end input", "");
        self.nvim.command("redraw | echoh Comment | echom '-- [PAGE] end of input --' | echoh None").unwrap();
    }

    pub fn get_var_or(&mut self, key: &str, default: &str) -> String {
        let val = self.nvim.get_var(key)
            .map(|v| v.to_string())
            .unwrap_or_else(|e| {
                let description = e.to_string();
                if description != format!("1 - Key '{}' not found", key)
                && description != format!("1 - Key not found: {}", key) { // For newer neovim versions
                    log::error!(target: "get var", "Error when getting var: {}, {}", key, e);
                }
                String::from(default)
            });
        log::trace!(target: "get var", "Key '{}': '{}'", key, val);
        val
    }
}

/// This struct provides commands that would be run on output buffer after creation
pub struct OutputCommands {
    edit: String,
    ft: String,
    pre: String,
    post: String,
    provided_by_user: String,
}

impl OutputCommands {
    fn create_with(provided_by_user: &str, writeable: bool) -> OutputCommands {
        let mut provided_by_user = provided_by_user.replace("'", "''"); // Ecranizes viml literal string
        if !provided_by_user.is_empty() {
            provided_by_user = format!("| exe '{}'", provided_by_user);
        }
        let mut edit = String::new();
        if !writeable {
            edit.push_str(" \
                | setl nomodifiable \
                | call nvim_buf_set_keymap(0, '', 'I', '<CMD> \
                    | setl scrolloff=0 \
                    | call cursor(9999999999, 9999999999) \
                    | call search(''\\S'') \
                    | call feedkeys(\"z\\<lt>CR>M\", ''nx'') \
                    | call timer_start(100, { -> execute(''au CursorMoved <buffer> ++once echo'') }) \
                    | redraw \
                    | echohl Comment | echo ''-- [PAGE] in the beginning of scroll --'' | echohl None \
                    | setl scrolloff=999 \
                    <CR>', { 'noremap': v:true }) \
                | call nvim_buf_set_keymap(0, '', 'A', '<CMD> \
                    | setl scrolloff=0 \
                    | call cursor(1, 1) \
                    | call search(''\\S'', ''b'') \
                    | call feedkeys(\"z-M\", ''nx'') \
                    | call timer_start(100, { -> execute(''au CursorMoved <buffer> ++once echo'') }) \
                    | redraw \
                    | echohl Comment | echo ''-- [PAGE] at the end of scroll --'' | echohl None \
                    | setl scrolloff=999 \
                    <CR>', { 'noremap': v:true }) \
                | call nvim_buf_set_keymap(0, '', 'i', '<CMD> \
                    | call cursor(9999999999, 9999999999) \
                    | call search(''\\S'') \
                    | call timer_start(100, { -> execute(''au CursorMoved <buffer> ++once echo'') }) \
                    | redraw \
                    | echohl Comment | echo ''-- [PAGE] in the beginning --'' | echohl None \
                    <CR>', { 'noremap': v:true }) \
                | call nvim_buf_set_keymap(0, '', 'a', '<CMD> \
                    | call cursor(1, 1) \
                    | call search(''\\S'', ''b'') \
                    | call timer_start(100, { -> execute(''au CursorMoved <buffer> ++once echo'') }) \
                    | redraw \
                    | echohl Comment | echo ''-- [PAGE] at the end --'' | echohl None \
                    <CR>', { 'noremap': v:true }) \
            ")
        }
        OutputCommands {
            ft: String::new(),
            pre: String::new(),
            post: String::new(),
            edit,
            provided_by_user,
        }
    }

    pub fn for_file_buffer(cmd_provided_by_user: &str, writeable: bool) -> OutputCommands {
        let mut cmds = Self::create_with(cmd_provided_by_user, writeable);
        cmds.post.push_str("| exe 'silent doautocmd User PageOpenFile'");
        cmds
    }

    pub fn for_output_buffer(page_id: &str, opt: &crate::cli::OutputOptions) -> OutputCommands {
        let mut cmds = Self::create_with(opt.command.as_deref().unwrap_or_default(), opt.writable);
        cmds.ft = format!("filetype={}", opt.filetype);
        if opt.query_lines != 0usize {
            cmds.pre = format!("{prefix} \
                | exe 'command! -nargs=? Page call rpcnotify(0, ''page_fetch_lines'', ''{page_id}'', <args>)' \
                | exe 'autocmd BufEnter <buffer> command! -nargs=? Page call rpcnotify(0, ''page_fetch_lines'', ''{page_id}'', <args>)' \
                | exe 'autocmd BufDelete <buffer> call rpcnotify(0, ''page_buffer_closed'', ''{page_id}'')' \
            ",
                prefix = cmds.pre,
                page_id = page_id,
            );
        }
        if opt.pwd {
            cmds.pre = format!("{prefix} \
                | let b:page_lcd_backup = getcwd() \
                | lcd {pwd} \
                | exe 'autocmd BufEnter <buffer> lcd {pwd}' \
                | exe 'autocmd BufLeave <buffer> lcd ' .. b:page_lcd_backup \
            ",
                prefix = cmds.pre,
                pwd = std::env::var("PWD").unwrap()
            );
        }
        cmds
    }
}



/// This enum represents all notifications that could be sent from page's commands on neovim side
pub enum NotificationFromNeovim {
    FetchPart,
    FetchLines(usize),
    BufferClosed,
}

mod notifications {
    use super::NotificationFromNeovim;
    use neovim_lib::{Value, NeovimApi};
    use std::sync::mpsc;

    /// Registers handler which receives notifications from neovim side.
    /// Commands are received on separate thread and further redirected to mpsc sender
    /// associated with receiver returned from current function
    pub fn subscribe(nvim: &mut neovim_lib::Neovim, page_id: &str) -> mpsc::Receiver<NotificationFromNeovim> {
        log::trace!(target: "subscribe to notifications", "Id: {}", page_id);
        let (tx, rx) = mpsc::sync_channel(16);
        nvim.session.start_event_loop_handler(NotificationReceiver { tx, page_id: page_id.to_string() });
        nvim.subscribe("page_fetch_lines").unwrap();
        nvim.subscribe("page_buffer_closed").unwrap();
        rx
    }

    /// Receives and collects notifications from neovim side
    struct NotificationReceiver {
        pub tx: mpsc::SyncSender<NotificationFromNeovim>,
        pub page_id: String,
    }

    impl neovim_lib::Handler for NotificationReceiver {
        fn handle_notify(&mut self, notification: &str, args: Vec<Value>) {
            log::trace!(target: "notification", "{}: {:?} ", notification, args);
            let page_id = args.get(0).and_then(Value::as_str);
            if page_id.map_or(true, |page_id| page_id != self.page_id) {
                log::warn!(target: "invalid page id", "");
                return
            }
            let notification_from_neovim = match notification {
                "page_fetch_lines" => {
                    if let Some(lines_count) = args.get(1).and_then(Value::as_u64) {
                        NotificationFromNeovim::FetchLines(lines_count as usize)
                    } else {
                        NotificationFromNeovim::FetchPart
                    }
                },
                "page_buffer_closed" => {
                    NotificationFromNeovim::BufferClosed
                },
                _ => {
                    log::warn!(target: "unhandled notification", "");
                    return
                }
            };
            self.tx.send(notification_from_neovim).expect("Cannot receive notification")
        }
    }

    impl neovim_lib::RequestHandler for NotificationReceiver {
        fn handle_request(&mut self, request: &str, args: Vec<Value>) -> Result<Value, Value> {
            log::warn!(target: "unhandled request", "{}: {:?}", request, args);
            Ok(Value::from(0))
        }
    }
}



/// This struct contains all neovim-related data which is required by page
/// after connection with neovim is established
pub struct NeovimConnection {
    pub nvim_proc: Option<std::process::Child>,
    pub nvim_actions: NeovimActions,
    pub initial_win_and_buf: (neovim_lib::neovim_api::Window, neovim_lib::neovim_api::Buffer),
    pub initial_buf_number: i64,
    pub rx: mpsc::Receiver<NotificationFromNeovim>,
}

impl NeovimConnection {
    pub fn is_child_neovim_process_spawned(&self) -> bool {
        self.nvim_proc.is_some()
    }
}

pub mod connection {
    use crate::{context, cli::Options};
    use super::{notifications, NeovimConnection, NeovimActions};
    use std::{path::PathBuf, process};

    /// Connects to parent neovim session or spawns a new neovim process and connects to it through socket.
    /// Replacement for `neovim_lib::Session::new_child()`, since it uses --embed flag and steals page stdin
    pub fn open(cli_ctx: &context::UsageContext) -> NeovimConnection {
        let (nvim_session, nvim_proc) = if let Some(nvim_listen_addr) = cli_ctx.opt.address.as_deref() {
            let session_at_addr = session_at_address(nvim_listen_addr).expect("Cannot connect to parent neovim");
            (session_at_addr, None)
        } else {
            session_with_new_neovim_process(&cli_ctx)
        };
        let mut nvim = neovim_lib::Neovim::new(nvim_session);
        let rx = notifications::subscribe(&mut nvim, &cli_ctx.page_id);
        let mut nvim_actions = NeovimActions::on(nvim);
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

    /// Waits until child neovim closes. If no child neovim process spawned then it's safe to just exit from page
    pub fn close(nvim_connection: NeovimConnection) {
        if let Some(mut process) = nvim_connection.nvim_proc {
            process.wait().expect("Neovim process died unexpectedly");
        }
    }

    /// Creates a new session using UNIX socket.
    /// Also prints protection from shell redirection that could cause some harm (see --help[-W])
    fn session_with_new_neovim_process(cli_ctx: &context::UsageContext) -> (neovim_lib::Session, Option<process::Child>) {
        let context::UsageContext { opt, tmp_dir, page_id, print_protection, .. } = cli_ctx;
        if *print_protection {
            print_redirect_protection(&tmp_dir);
        }
        let p = tmp_dir.clone().join(&format!("socket-{}", page_id));
        let nvim_listen_addr = p.to_string_lossy();
        let nvim_proc = spawn_child_nvim_process(opt, &nvim_listen_addr);
        let mut i = 0;
        let e = loop {
            match session_at_address(&nvim_listen_addr) {
                Ok(nvim_session) => return (nvim_session, Some(nvim_proc)),
                Err(e) => {
                    if let std::io::ErrorKind::NotFound = e.kind() {
                        if i == 100 {
                            break e
                        } else {
                            log::trace!(target: "cannot connect to child neovim", "[attempt #{}] address '{}': {:?}", i, nvim_listen_addr, e);
                            std::thread::sleep(std::time::Duration::from_millis(16));
                            i += 1
                        }
                    } else {
                        break e
                    }
                }
            }
        };
        panic!("Cannot connect to neovim: {:?}", e);
    }

    /// This is hack to prevent behavior (or bug) in some shells (see --help[-W])
    fn print_redirect_protection(tmp_dir: &PathBuf) {
        let d = tmp_dir.clone().join("DO-NOT-REDIRECT-OUTSIDE-OF-NVIM-TERM(--help[-W])");
        if let Err(e) = std::fs::create_dir_all(&d) {
            panic!("Cannot create protection directory '{}': {:?}", d.display(), e)
        }
        println!("{}", d.to_string_lossy());
    }

    /// Spawns child neovim process on top of page, which further will be connected to page with UNIX socket.
    /// In this way neovim UI is displayed properly on top of page, and page as well is able to handle
    /// its own input to redirect it unto proper target (which is impossible with methods provided by neovim_lib).
    /// Also custom neovim config will be picked if it exists on corresponding locations.
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
        log::trace!(target: "new neovim process", "Args: {:?}", nvim_args);
        process::Command::new("nvim").args(&nvim_args)
            .stdin(process::Stdio::null())
            .spawn()
            .expect("Cannot spawn a child neovim process")
    }

    /// Returns path to custom neovim config if it's present in corresponding locations
    fn default_config_path() -> Option<String> {
        std::env::var("XDG_CONFIG_HOME").ok().and_then(|xdg_config_home| {
            let p = PathBuf::from(xdg_config_home).join("page/init.vim");
            if p.exists() {
                log::trace!(target: "default config", "Use $XDG_CONFIG_HOME: {}", p.display());
                Some(p)
            } else {
                None
            }
        })
        .or_else(|| std::env::var("HOME").ok().and_then(|home_dir| {
            let p = PathBuf::from(home_dir).join(".config/page/init.vim");
            if p.exists() {
                log::trace!(target: "default config", "Use ~/.config: {}", p.display());
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
