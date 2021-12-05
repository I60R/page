/// A module that extends neovim api with methods required in page


use std::{io, path::PathBuf, pin::Pin, task};
use parity_tokio_ipc::Connection;
use tokio::{io::{ReadHalf, WriteHalf}, net::TcpStream, task::JoinHandle};
use nvim_rs::{compat::tokio::Compat, error::LoopError, error::CallError};

pub use nvim_rs::{neovim::Neovim, Buffer, Window, Value};


const TERM_URI: &str = concat!(
    "term://",     // Shell will be temporarily replaced with /bin/sleep
    "2147483647d"  // This will sleep for i32::MAX days
);

pub enum IoRead {
    Ipc(Compat<ReadHalf<Connection>>),
    Tcp(Compat<ReadHalf<TcpStream>>),
}

pub enum IoWrite {
    Ipc(Compat<WriteHalf<Connection>>),
    Tcp(Compat<WriteHalf<TcpStream>>),

}

macro_rules! delegate {
    ($self:ident => $method:ident($($args:expr),*)) => {
        match $self.get_mut() {
            Self::Ipc(rw) => Pin::new(rw).$method($($args),*),
            Self::Tcp(rw) => Pin::new(rw).$method($($args),*),
        }
    };
}

impl futures::AsyncRead for IoRead {
    fn poll_read(self: Pin<&mut Self>, cx: &mut task::Context<'_>, buf: &mut [u8]) -> task::Poll<Result<usize, io::Error>> {
        delegate!(self => poll_read(cx, buf))
    }
}

impl futures::AsyncWrite for IoWrite {
    fn poll_write(self: Pin<&mut Self>, cx: &mut task::Context<'_>, buf: &[u8]) -> task::Poll<io::Result<usize>> {
        delegate!(self => poll_write(cx, buf))
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<io::Result<()>> {
        delegate!(self => poll_flush(cx))
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Result<(), io::Error>> {
        delegate!(self => poll_close(cx))
    }
}


/// This struct wraps nvim_rs::Neovim and decorates it with methods required in page.
/// Results returned from underlying Neovim methods are mostly unwrapped, since we anyway cannot provide
/// any meaningful falback logic on call side
pub struct NeovimActions {
    nvim: Neovim<IoWrite>,
    join: JoinHandle<Result<(), Box<LoopError>>>,
}

impl NeovimActions {
    pub async fn get_current_buffer(&mut self) -> Result<Buffer<IoWrite>, Box<CallError>> {
        self.nvim.get_current_buf().await
    }

    pub async fn create_substituting_output_buffer(&mut self) -> Buffer<IoWrite> {
        self.create_buffer(&format!("e {}", TERM_URI)).await
            .expect("Error when creating output buffer")
    }

    pub async fn create_split_output_buffer(&mut self, opt: &crate::cli::SplitOptions) -> Buffer<IoWrite> {
        let cmd = if opt.popup {
            let (w, h, r, c): (String, String, String, String);
            if opt.split_right != 0u8 {
                (w = format!("(((winwidth(0) / 2) * 3) / {})", opt.split_right + 1), h = "winheight(0)".into(), r = "0".into(), c = "winwidth(0)".into())
            } else if opt.split_left != 0u8 {
                (w = format!("(((winwidth(0) / 2) * 3) / {})", opt.split_left + 1), h = "winheight(0)".into(), r = "0".into(), c = "0".into())
            } else if opt.split_below != 0u8 {
                (h = format!("(((winheight(0) / 2) * 3) / {})", opt.split_below + 1), w = "winwidth(0)".into(), c = "0".into(), r = "winheight(0)".into())
            } else if opt.split_above != 0u8 {
                (h = format!("(((winheight(0) / 2) * 3) / {})", opt.split_above + 1), w = "winwidth(0)".into(), c = "0".into(), r = "0".into())
            } else if let Some(split_right_cols) = opt.split_right_cols {
                (w = split_right_cols.to_string(), h = "winheight(0)".into(), r = "0".into(), c = "winwidth(0)".into())
            } else if let Some(split_left_cols) = opt.split_left_cols {
                (w = split_left_cols.to_string(), h = "winheight(0)".into(), r = "0".into(), c = "0".into())
            } else if let Some(split_below_rows) = opt.split_below_rows {
                (h = split_below_rows.to_string(), w = "winwidth(0)".into(), c = "0".into(), r = "winheight(0)".into())
            } else if let Some(split_above_rows) = opt.split_above_rows {
                (h = split_above_rows.to_string(), w = "winwidth(0)".into(), c = "0".into(), r = "0".into())
            } else {
                unreachable!()
            };
            format!(" \
                  call nvim_open_win(nvim_create_buf(0, 0), 1, {{ 'relative': 'editor', 'width': {w}, 'height': {h}, 'row': {r}, 'col': {c} }}) \
                | e {t} \
                | setl winblend=25 \
            ",
                  w = w, h = h, r = r, c = c, t = TERM_URI,
            )
        } else {
            let (ver_ratio, hor_ratio) = ("(winwidth(0) / 2) * 3", "(winheight(0) / 2) * 3");
            if opt.split_right != 0u8 {
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
            }
        };
        self.create_buffer(&cmd).await
            .expect("Error when creating split output buffer")
    }

    async fn create_buffer(&mut self, term_open_cmd: &str) -> Result<Buffer<IoWrite>, Box<CallError>> {
        let cmd = format!(" \
              let g:page_shell_backup = [&shell, &shellcmdflag] \
            | let [&shell, &shellcmdflag] = ['/bin/sleep', ''] \
            | {term_open_cmd} \
            | let [&shell, &shellcmdflag] = g:page_shell_backup \
        ",
            term_open_cmd = term_open_cmd
        );
        log::trace!(target: "create buffer", "{}", cmd);
        self.nvim.command(&cmd).await?;
        let buf = self.get_current_buffer().await?;
        log::trace!(target: "create buffer", "created: {:?}", buf.get_number().await);
        Ok(buf)
    }

    pub async fn get_current_buffer_pty_path(&mut self) -> PathBuf {
        let pty_fetch_cmd = "\
            local i = 0
            while i < 256 do
                local pty = vim.api.nvim_get_chan_info(vim.bo.channel).pty
                if pty ~= '' then
                    return { i, pty }
                end
                vim.wait(16, function() end)
                i = i + 1
            end
            error 'No PTY on channel info' \
        ".replace("\n            ", "\n");
        log::trace!(target: "use pty", "{}", pty_fetch_cmd);
        let pty_fetch = self.nvim.execute_lua(&pty_fetch_cmd, vec![]).await.expect("Cannot fetch PTY info");
        let (i, buf_pty_path) = pty_fetch.as_array().and_then(|a| Some((a.get(0)?.as_u64()?, a.get(1)?.as_str()?))).expect("Wrong PTY info types");
        log::trace!(target: "use pty", "attempts: {} => {}", i, buf_pty_path);
        PathBuf::from(buf_pty_path)
    }

    pub async fn mark_buffer_as_instance(&mut self, buffer: &Buffer<IoWrite>, inst_name: &str, inst_pty_path: &str) {
        log::trace!(target: "new instance", "{:?}->{}->{}", buffer.get_number().await, inst_name, inst_pty_path);
        let v = Value::from(vec![Value::from(inst_name), Value::from(inst_pty_path)]);
        if let Err(e) = buffer.set_var("page_instance", v).await {
            log::error!(target: "new instance", "Error when setting instance mark: {}", e);
        }
    }

    pub async fn find_instance_buffer(&mut self, inst_name: &str) -> Option<(Buffer<IoWrite>, PathBuf)> {
        let all_bufs = self.nvim.list_bufs().await.expect("Cannot list all buffers");
        for buf in all_bufs {
            let inst_var = buf.get_var("page_instance").await;
            log::trace!(target: "instances", "{:?} => {}: {:?}", buf.get_number().await, inst_name, inst_var);
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

    pub async fn close_instance_buffer(&mut self, inst_name: &str) {
        log::trace!(target: "close instance", "{}", inst_name);
        if let Some((buf, _)) = self.find_instance_buffer(&inst_name).await {
            let inst_id = buf.get_number().await.expect("Cannot get instance id");
            if let Err(e) = self.nvim.command(&format!("exe 'bd!' . {}", inst_id)).await {
                log::error!(target: "close instance", "Error when closing instance buffer: {}, {}", inst_name, e);
            }
        }
    }

    pub async fn focus_instance_buffer(&mut self, inst_buf: &Buffer<IoWrite>) {
        log::trace!(target: "focus instance", "{:?}", inst_buf.get_number().await);
        let active_buf = self.get_current_buffer().await.expect("Cannot get currently active buffer");
        if &active_buf != inst_buf {
            let all_wins = self.nvim.list_wins().await.expect("Cannot get all active windows");
            log::trace!(target: "focus instance", "Winows open: {}", all_wins.len());
            for win in all_wins {
                let buf = win.get_buf().await.expect("Cannot get buffer");
                if &buf == inst_buf {
                    log::trace!(target: "focus instance", "Use window: {:?}", win.get_number().await);
                    self.nvim.set_current_win(&win).await.expect("Cannot set active window");
                    return
                }
            }
        } else {
            log::trace!(target: "focus instance", "Not in window");
        }
        self.nvim.set_current_buf(inst_buf).await.expect("Cannot set active buffer");
    }


    pub async fn update_buffer_title(&mut self, buf: &Buffer<IoWrite>, buf_title: &str) {
        log::trace!(target: "update title", "{:?} => {}", buf.get_number().await, buf_title);
        let a = std::iter::once((0, buf_title.to_string()));
        let b = (1..99).map(|attempt_nr| (attempt_nr, format!("{}({})", buf_title, attempt_nr)));
        for (attempt_nr, name) in a.chain(b) {
            match buf.set_name(&name).await {
                Err(e) => {
                    log::trace!(target: "update title", "{:?} => {}: {:?}", buf.get_number().await, buf_title, e.to_string());
                    match *e {
                        CallError::NeovimError(_, msg) if msg.as_str() == "Failed to rename buffer" && attempt_nr < 99 => continue,
                        _ => {
                            log::error!(target: "update title", "Cannot update title: {}", e);
                            return
                        }
                    }
                }
                _ => {
                    self.nvim.command("redraw!").await.expect("Cannot redraw"); // To update statusline
                    return
                }
            }
        }
    }

    pub async fn prepare_output_buffer(&mut self, initial_buf_nr: i64, cmds: OutputCommands) {
        let options = format!(" \
            | let b:page_alternate_bufnr={initial_buf_nr} \
            | let b:page_scrolloff_backup=&scrolloff \
            | set scrolloff=999 \
            | setl scrollback=100000 signcolumn=no nonumber {ft} \
            {cmd_edit} \
            | exe 'autocmd BufEnter <buffer> set scrolloff=999' \
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
        if let Err(e) = self.nvim.command(&options).await {
            log::error!(target: "prepare output", "Unable to set page options, text might be displayed improperly: {}", e);
        }
    }

    pub async fn execute_connect_autocmd_on_current_buffer(&mut self) {
        log::trace!(target: "au PageConnect", "");
        if let Err(e) = self.nvim.command("silent doautocmd User PageConnect").await {
            log::error!(target: "au PageConnect", "Cannot execute PageConnect: {}", e);
        }
    }

    pub async fn execute_disconnect_autocmd_on_current_buffer(&mut self) {
        log::trace!(target: "au PageDisconnect", "");
        if let Err(e) = self.nvim.command("silent doautocmd User PageDisconnect").await {
            log::error!(target: "au PageDisconnect", "Cannot execute PageDisconnect: {}", e);
        }
    }

    pub async fn execute_command_post(&mut self, cmd: &str) {
        log::trace!(target: "command post", "{}", cmd);
        if let Err(e) = self.nvim.command(cmd).await {
            log::error!(target: "command post", "Error when executing post command '{}': {}", cmd, e);
        }
    }

    pub async fn switch_to_window_and_buffer(&mut self, (win, buf): &(Window<IoWrite>, Buffer<IoWrite>)) {
        log::trace!(target: "set window and buffer", "Win:{:?} Buf:{:?}",  win.get_number().await, buf.get_number().await);
        if let Err(e) = self.nvim.set_current_win(win).await {
            log::error!(target: "set window and buffer", "Can't switch to window: {}", e);
        }
        if let Err(e) = self.nvim.set_current_buf(buf).await {
            log::error!(target: "set window and buffer", "Can't switch to buffer: {}", e);
        }
    }

    pub async fn switch_to_buffer(&mut self, buf: &Buffer<IoWrite>) -> Result<(), Box<CallError>>{
        log::trace!(target: "set buffer", "{:?}", buf.get_number().await);
        self.nvim.set_current_buf(buf).await
    }

    pub async fn set_current_buffer_insert_mode(&mut self) {
        log::trace!(target: "set INSERT", "");
        if let Err(e) = self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>A", 'n')"###).await {// Fixes "can't enter normal mode from..."
            log::error!(target: "set INSERT", "Error when setting mode: {}", e);
        }
    }

    pub async fn set_current_buffer_follow_output_mode(&mut self) {
        log::trace!(target: "set FOLLOW", "");
        if let Err(e) = self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>G, 'n'")"###).await {
            log::error!(target: "set FOLLOW", "Error when setting mode: {}", e);
        }
    }

    pub async fn set_current_buffer_scroll_mode(&mut self) {
        log::trace!(target: "set SCROLL", "");
        if let Err(e) = self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>ggM, 'n'")"###).await {
            log::error!(target: "set SCROLL", "Error when setting mode: {}", e);
        }
    }

    pub async fn open_file_buffer(&mut self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        log::trace!(target: "open file", "{}", file_path);
        self.nvim.command(&format!("e {}", std::fs::canonicalize(file_path)?.to_string_lossy())).await?;
        Ok(())
    }

    pub async fn notify_query_finished(&mut self, lines_read: usize) {
        log::trace!(target: "query finished", "Read {} lines", lines_read);
        let cmd = format!("redraw | echoh Comment | echom '-- [PAGE] {} lines read; has more --' | echoh None", lines_read);
        self.nvim.command(&cmd).await.expect("Cannot notify query finished");
    }

    pub async fn notify_end_of_input(&mut self) {
        log::trace!(target: "end input", "");
        self.nvim.command("redraw | echoh Comment | echom '-- [PAGE] end of input --' | echoh None").await.expect("Cannot notify end of input");
    }

    pub async fn get_var_or(&mut self, key: &str, default: &str) -> String {
        let val = self.nvim.get_var(key).await
            .map(|v| v.to_string())
            .unwrap_or_else(|e| {
                match *e {
                    CallError::NeovimError(_, msg) if msg == format!("Key not found: {}", key) => {},
                    _ => log::error!(target: "get var", "Error when getting var: {}, {}", key, e),
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
                | let map_opts = { 'noremap': v:true, 'nowait': v:true } \
                | call nvim_buf_set_keymap(0, '', 'I', '<CMD> \
                    | setl scrolloff=0 \
                    | call cursor(9999999999, 9999999999) \
                    | call search(''\\S'') \
                    | call feedkeys(\"z\\<lt>CR>M\", ''nx'') \
                    | call timer_start(100, { -> execute(''au CursorMoved <buffer> ++once echo'') }) \
                    | redraw \
                    | echohl Comment | echo ''-- [PAGE] in the beginning of scroll --'' | echohl None \
                    | setl scrolloff=999 \
                    <CR>', map_opts) \
                | call nvim_buf_set_keymap(0, '', 'A', '<CMD> \
                    | setl scrolloff=0 \
                    | call cursor(1, 1) \
                    | call search(''\\S'', ''b'') \
                    | call feedkeys(\"z-M\", ''nx'') \
                    | call timer_start(100, { -> execute(''au CursorMoved <buffer> ++once echo'') }) \
                    | redraw \
                    | echohl Comment | echo ''-- [PAGE] at the end of scroll --'' | echohl None \
                    | setl scrolloff=999 \
                    <CR>', map_opts) \
                | call nvim_buf_set_keymap(0, '', 'i', '<CMD> \
                    | call cursor(9999999999, 9999999999) \
                    | call search(''\\S'') \
                    | call timer_start(100, { -> execute(''au CursorMoved <buffer> ++once echo'') }) \
                    | redraw \
                    | echohl Comment | echo ''-- [PAGE] in the beginning --'' | echohl None \
                    <CR>', map_opts) \
                | call nvim_buf_set_keymap(0, '', 'a', '<CMD> \
                    | call cursor(1, 1) \
                    | call search(''\\S'', ''b'') \
                    | call timer_start(100, { -> execute(''au CursorMoved <buffer> ++once echo'') }) \
                    | redraw \
                    | echohl Comment | echo ''-- [PAGE] at the end --'' | echohl None \
                    <CR>', map_opts) \
                | call nvim_buf_set_keymap(0, '', 'u', '<C-u>', map_opts) \
                | call nvim_buf_set_keymap(0, '', 'd', '<C-d>', map_opts) \
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
#[derive(Debug)]
pub enum NotificationFromNeovim {
    FetchPart,
    FetchLines(usize),
    BufferClosed,
}

mod handler {
    use super::{Neovim, IoWrite, NotificationFromNeovim, Value};
    /// Receives and collects notifications from neovim side over IPC or TCP/IP
    #[derive(Clone)]
    pub struct IoHandler {
        pub tx: tokio::sync::mpsc::Sender<NotificationFromNeovim>,
        pub page_id: String,
    }

    #[async_trait::async_trait]
    impl nvim_rs::Handler for IoHandler {
        type Writer = IoWrite;

        async fn handle_request(&self, request: String, args: Vec<Value>, _neovim: Neovim<IoWrite>) -> Result<Value, Value> {
            log::warn!(target: "unhandled request", "{}: {:?}", request, args);
            Ok(Value::from(0))
        }

        async fn handle_notify(&self, notification: String, args: Vec<Value>, _neovim: Neovim<IoWrite>) {
            log::trace!(target: "notification", "{}: {:?} ", notification, args);
            let page_id = args.get(0).and_then(Value::as_str);
            if page_id.map_or(true, |page_id| page_id != self.page_id) {
                log::warn!(target: "invalid page id", "");
                return
            }
            let notification_from_neovim = match notification.as_str() {
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
            self.tx.send(notification_from_neovim).await.expect("Cannot receive notification")
        }
    }
}

/// This struct contains all neovim-related data which is required by page
/// after connection with neovim is established
pub struct NeovimConnection {
    pub nvim_proc: Option<JoinHandle<tokio::process::Child>>,
    pub nvim_actions: NeovimActions,
    pub initial_buf_number: i64,
    pub initial_win_and_buf: (Window<IoWrite>, Buffer<IoWrite>),
    pub rx: tokio::sync::mpsc::Receiver<NotificationFromNeovim>,
}


pub mod connection {
    use super::{Neovim, IoRead, IoWrite, handler::IoHandler, NeovimConnection, NeovimActions};
    use crate::context;

    use tokio::task::JoinHandle;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    use std::{path::{Path, PathBuf}, fs};

    /// Connects to parent neovim session or spawns a new neovim process and connects to it through socket.
    /// Replacement for `nvim_rs::Session::new_child()`, since it uses --embed flag and steals page stdin
    pub async fn open(cli_ctx: &context::UsageContext) -> NeovimConnection {
        let page_id = cli_ctx.page_id.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let handler = IoHandler { page_id, tx };
        let mut nvim_proc = None;
        let (nvim, join) = match cli_ctx.opt.address.as_deref() {
            Some(nvim_listen_addr) if nvim_listen_addr.parse::<std::net::SocketAddr>().is_ok() => {
                let tcp = tokio::net::TcpStream::connect(nvim_listen_addr).await
                    .expect("Cannot connect to neoim at TCP/IP address");
                let (rx, tx) = tokio::io::split(tcp);
                let (rx, tx) = (IoRead::Tcp(rx.compat()), IoWrite::Tcp(tx.compat_write()));
                let (nvim, io) = Neovim::<IoWrite>::new(rx, tx, handler);
                let io_handle = tokio::task::spawn(io);
                (nvim, io_handle)
            }
            Some(nvim_listen_addr) => {
                let ipc = parity_tokio_ipc::Endpoint::connect(nvim_listen_addr).await
                    .expect("Cannot connect to neovim at path");
                let (rx, tx) = tokio::io::split(ipc);
                let (rx, tx) = (IoRead::Ipc(rx.compat()), IoWrite::Ipc(tx.compat_write()));
                let (nvim, io) = Neovim::<IoWrite>::new(rx, tx, handler);
                let io_handle = tokio::task::spawn(io);
                (nvim, io_handle)
            }
            None => {
                let (nvim, io_handle, child) = create_new_neovim_process_ipc(cli_ctx, handler).await;
                nvim_proc = Some(child);
                (nvim, io_handle)
            }
        };
        let initial_win = nvim.get_current_win().await.expect("Cannot get initial window");
        let initial_buf = nvim.get_current_buf().await.expect("Cannot get initial buffer");
        let initial_buf_number = initial_buf.get_number().await.expect("Cannot get initial buffer number");
        NeovimConnection {
            nvim_actions: NeovimActions { nvim, join },
            initial_buf_number,
            initial_win_and_buf: (initial_win, initial_buf),
            rx,
            nvim_proc
        }
    }

    /// Waits until child neovim closes. If no child neovim process spawned then it's safe to just exit from page
    pub async fn close(nvim_connection: NeovimConnection) {
        if let Some(process) = nvim_connection.nvim_proc {
            process.await.expect("Neovim spawned with error")
                .wait().await.expect("Neovim process died unexpectedly");
        }
        nvim_connection.nvim_actions.join.abort();
    }

    /// Creates a new session using UNIX socket.
    /// Also prints protection from shell redirection that could cause some harm (see --help[-W])
    async fn create_new_neovim_process_ipc(
        cli_ctx: &context::UsageContext,
        handler: IoHandler
    ) -> (
        Neovim<IoWrite>,
        JoinHandle<Result<(), Box<nvim_rs::error::LoopError>>>,
        JoinHandle<tokio::process::Child>
    ) {
        let context::UsageContext { opt, tmp_dir, page_id, print_protection, .. } = cli_ctx;
        if *print_protection {
            print_redirect_protection(&tmp_dir);
        }
        let nvim_listen_addr = tmp_dir.join(&format!("socket-{}", page_id));
        let nvim_proc = tokio::task::spawn_blocking({
            let (config, custom_args, nvim_listen_addr) = (opt.config.clone(), opt.arguments.clone(), nvim_listen_addr.clone());
            move || spawn_child_nvim_process(config, custom_args, &nvim_listen_addr)
        });
        tokio::time::sleep(std::time::Duration::from_millis(128)).await;
        let mut i = 0;
        let e = loop {
            match parity_tokio_ipc::Endpoint::connect(&nvim_listen_addr).await {
                Ok(ipc) => {
                    let (rx, tx) = tokio::io::split(ipc);
                    let (rx, tx) = (IoRead::Ipc(rx.compat()), IoWrite::Ipc(tx.compat_write()));
                    let (neovim, io) = Neovim::<IoWrite>::new(rx, tx, handler);
                    let io_handle = tokio::task::spawn(io);
                    return (neovim, io_handle, nvim_proc)
                },
                Err(e) => {
                    if let std::io::ErrorKind::NotFound = e.kind() {
                        if i == 256 {
                            break e
                        } else {
                            log::trace!(target: "cannot connect to child neovim", "[attempt #{}] address '{:?}': {:?}", i, nvim_listen_addr, e);
                            tokio::time::sleep(std::time::Duration::from_millis(8)).await;
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
    fn spawn_child_nvim_process(config: Option<String>, custom_args: Option<String>, nvim_listen_addr: &Path) -> tokio::process::Child {
        let nvim_args = {
            let mut a = String::new();
            a.push_str("--cmd 'set shortmess+=I' ");
            a.push_str("--listen ");
            a.push_str(&nvim_listen_addr.to_string_lossy());
            if let Some(config) = config.or_else(default_config_path) {
                a.push(' ');
                a.push_str("-u ");
                a.push_str(&config);
            }
            if let Some(custom_args) = custom_args.as_ref() {
                a.push(' ');
                a.push_str(custom_args);
            }
            shell_words::split(&a).expect("Cannot parse neovim arguments")
        };
        log::trace!(target: "new neovim process", "Args: {:?}", nvim_args);
        let tty = fs::OpenOptions::new().read(true)
            .open("/dev/tty")
            .expect("Cannot open /dev/tty");
        tokio::process::Command::new("nvim").args(&nvim_args)
            .env_remove("RUST_LOG")
            .stdin(tty)
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
}
