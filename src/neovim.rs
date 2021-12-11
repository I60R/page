/// A module that extends neovim api with methods required in page


use std::{io, path::PathBuf, pin::Pin, task, convert::TryInto};
use parity_tokio_ipc::Connection;
use tokio::{io::{ReadHalf, WriteHalf}, net::TcpStream, task::JoinHandle};
use nvim_rs::{compat::tokio::Compat, error::LoopError, error::CallError};
use indoc::{indoc, formatdoc};

pub use nvim_rs::{neovim::Neovim, Buffer, Window, Value};

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

    pub async fn create_replacing_output_buffer(&mut self) -> (Buffer<IoWrite>, PathBuf) {
        let cmd = indoc! {"
            local buf = vim.api.nvim_get_current_buf()
        "};
        self.create_buffer(cmd).await.expect("Error when creating output buffer from current")
    }

    pub async fn create_switching_output_buffer(&mut self) -> (Buffer<IoWrite>, PathBuf) {
        let cmd = indoc! {"
            local buf = vim.api.nvim_create_buf(true, false)
            vim.api.nvim_set_current_buf(buf)
        "};
        self.create_buffer(cmd).await.expect("Error when creating output buffer")
    }

    pub async fn create_split_output_buffer(&mut self, opt: &crate::cli::SplitOptions) -> (Buffer<IoWrite>, PathBuf) {
        let cmd = if opt.popup {
            let ratio = |w_or_h, size| {
                format!("math.floor((({} / 2) * 3) / {})", w_or_h, size + 1)
            };
            let (width, height, row, col): (String, String, &str, &str);
            if opt.split_right != 0 {
                (width = ratio("w", opt.split_right), height = "h".into(), row = "0", col = "w")
            } else if opt.split_left != 0 {
                (width = ratio("w", opt.split_left),  height = "h".into(), row = "0", col = "0")
            } else if opt.split_below != 0 {
                (width = "w".into(), height = ratio("h", opt.split_below), row = "h", col = "0")
            } else if opt.split_above != 0 {
                (width = "w".into(), height = ratio("h", opt.split_above), row = "0", col = "0")
            } else if let Some(split_right_cols) = opt.split_right_cols {
                (width = split_right_cols.to_string(), height = "h".into(), row = "0", col = "w")
            } else if let Some(split_left_cols) = opt.split_left_cols {
                (width = split_left_cols.to_string(),  height = "h".into(), row = "0", col = "0")
            } else if let Some(split_below_rows) = opt.split_below_rows {
                (width = "w".into(), height = split_below_rows.to_string(), row = "h", col = "0")
            } else if let Some(split_above_rows) = opt.split_above_rows {
                (width = "w".into(), height = split_above_rows.to_string(), row = "0", col = "0")
            } else {
                unreachable!()
            };
            formatdoc! {"
                local w, h = vim.api.nvim_win_get_width(0), vim.api.nvim_win_get_height(0)
                local buf = vim.api.nvim_create_buf(true, false)
                local win = vim.api.nvim_open_win(buf, true, {{ relative = 'editor', width = {w}, height = {h}, row = {r}, col = {c} }})
                vim.api.nvim_set_current_win(win)
                vim.api.nvim_win_set_option(win, 'winblend', 25)
            ",
                  w = width, h = height, r = row, c = col,
            }
        } else {
            let ratio = |w_or_h, size| {
                format!("' .. tostring(math.floor((({} / 2) * 3) / {})) .. '", w_or_h, size + 1)
            };
            let (direction, size, cmd, option): (&str, String, &str, &str);
            if opt.split_right != 0 {
                (direction = "belowright", size = ratio("w", opt.split_right), cmd = "vsplit", option = "winfixwidth")
            } else if opt.split_left != 0 {
                (direction = "aboveleft",  size = ratio("w", opt.split_left),  cmd = "vsplit", option = "winfixwidth")
            } else if opt.split_below != 0 {
                (direction = "belowright", size = ratio("h", opt.split_below), cmd = "split", option = "winfixheight")
            } else if opt.split_above != 0 {
                (direction = "aboveleft",  size = ratio("h", opt.split_above), cmd = "split", option = "winfixheight")
            } else if let Some(split_right_cols) = opt.split_right_cols {
                (direction = "belowright", size = split_right_cols.to_string(), cmd = "vsplit", option = "winfixwidth")
            } else if let Some(split_left_cols) = opt.split_left_cols {
                (direction = "aboveleft",  size = split_left_cols.to_string(),  cmd = "vsplit", option = "winfixwidth")
            } else if let Some(split_below_rows) = opt.split_below_rows {
                (direction = "belowright", size = split_below_rows.to_string(), cmd = "split", option = "winfixheight")
            } else if let Some(split_above_rows) = opt.split_above_rows {
                (direction = "aboveleft",  size = split_above_rows.to_string(), cmd = "split", option = "winfixheight")
            } else {
                unreachable!()
            };
            formatdoc! {"
                local prev_win, win = vim.api.nvim_get_current_win()
                local w, h = vim.api.nvim_win_get_width(prev_win), vim.api.nvim_win_get_height(prev_win)
                local function do_nothing() end
                vim.cmd('{d} {s}{c}')
                local buf = vim.api.nvim_create_buf(true, false)
                vim.api.nvim_set_current_buf(buf)
                vim.api.nvim_win_set_option(win, '{o}', true)
            ",
                d = direction, s = size, c = cmd, o = option
            }
        };
        self.create_buffer(&cmd).await
            .expect("Error when creating split output buffer")
    }

    async fn create_buffer(&mut self, window_open_cmd: &str) -> Result<(Buffer<IoWrite>, PathBuf), Box<CallError>> {
        // Shell will be temporarily replaced with /bin/sleep that will halt for i32::MAX days
        let cmd = formatdoc! {"
            local shell, shellcmdflag = vim.o.shell, vim.o.shellcmdflag
            vim.o.shell, vim.o.shellcmdflag = 'sleep', ''
            {window_open_cmd}
            local chan = vim.api.nvim_call_function('termopen', {{ '2147483647d' }})
            vim.o.shell, vim.o.shellcmdflag = shell, shellcmdflag
            local pty = vim.api.nvim_get_chan_info(chan).pty
            if pty == '' then error 'No PTY on channel' end
            return {{ buf, pty }}
        ",
            window_open_cmd = window_open_cmd
        };
        log::trace!(target: "create buffer", "{}", cmd);
        let tup: Vec<Value> = self.nvim.exec_lua(&cmd, vec![]).await?.clone().try_into()?;
        let (buf_val, pty_val) = (
            tup.get(0).ok_or_else(|| CallError::NeovimError(None, "No buf handle".into()))?.clone(),
            tup.get(1).ok_or_else(|| CallError::NeovimError(None, "No pty handle".into()))?.clone(),
        );
        let buf = Buffer::new(buf_val, self.nvim.clone());
        let pty = PathBuf::from(pty_val.as_str().ok_or_else(|| CallError::NeovimError(None, "PTY not a string".into()))?);
        Ok((buf, pty))
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
            match inst_var.map_err(|e| *e) {
                Err(CallError::NeovimError(_, msg)) if msg == "Key not found: page_instance" => continue,
                Err(e) => panic!("Error when getting instance mark: {:?}", e),
                Ok(v) => {
                    if let Some((inst_name_found, inst_pty_path)) = v.as_array().and_then(|a| Some((a.get(0)?.as_str()?, a.get(1)?.as_str()?))) {
                        log::trace!(target: "found instance", "{}->{}", inst_name_found, inst_pty_path);
                        if inst_name == inst_name_found {
                            return Some((buf, PathBuf::from(inst_pty_path.to_string())))
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
        let options = formatdoc! {r#"
            vim.b.page_alternate_bufnr = {initial_buf_nr}
            if vim.wo.scrolloff > 999 or vim.wo.scrolloff < 0 then
                vim.g.page_scrolloff_backup = 0
            else
                vim.g.page_scrolloff_backup = vim.wo.scrolloff
            end
            vim.bo.scrollback, vim.wo.scrolloff, vim.wo.signcolumn, vim.wo.number = 100000, 999, 'no', false
            {ft}
            {cmd_edit}
            vim.cmd 'autocmd BufEnter <buffer> lua vim.wo.scrolloff = 999'
            vim.cmd 'autocmd BufLeave <buffer> lua vim.wo.scrolloff = vim.g.page_scrolloff_backup'
            {cmd_notify_closed}
            {cmd_pre}
            vim.cmd 'silent doautocmd User PageOpen | redraw'
            {cmd_provided_by_user}
            {cmd_post}
        "#,
            ft = cmds.ft,
            initial_buf_nr = initial_buf_nr,
            cmd_edit = cmds.edit,
            cmd_notify_closed = cmds.notify_closed,
            cmd_pre = cmds.pre,
            cmd_provided_by_user = cmds.provided_by_user,
            cmd_post = cmds.post,
        };
        log::trace!(target: "prepare output", "{}", options);
        if let Err(e) = self.nvim.exec_lua(&options, vec![]).await {
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
        let cmd = formatdoc! {"
            vim.cmd 'redraw'
            vim.api.nvim_echo({{ {{ '-- [PAGE] {} lines read; has more --', 'Comment' }}, }}, false, {{}})
        ",
            lines_read
        };
        self.nvim.exec_lua(&cmd, vec![]).await.expect("Cannot notify query finished");
    }

    pub async fn notify_end_of_input(&mut self) {
        log::trace!(target: "end input", "");
        let cmd = indoc! {"
            vim.cmd 'redraw'
            vim.api.nvim_echo({{ '-- [PAGE] end of input --', 'Comment' }, }, false, {})
        "};
        self.nvim.exec_lua(cmd, vec![]).await.expect("Cannot notify end of input");
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
    notify_closed: String,
    pre: String,
    provided_by_user: String,
    post: String,
}

impl OutputCommands {
    fn create_with(provided_by_user: &str, writeable: bool) -> OutputCommands {
        let mut provided_by_user = String::from(provided_by_user);
        if !provided_by_user.is_empty() {
            provided_by_user = format!("vim.cmd [====[{}]====]", provided_by_user);
        }
        let mut edit = String::new();
        if !writeable {
            edit.push_str(indoc! {r#"
                vim.bo.modifiable = false
                _G.echo_notification = function(message)
                    vim.defer_fn(function()
                        vim.api.nvim_echo({{ '-- [PAGE] ' .. message .. ' --', 'Comment' }, }, false, {})
                        vim.cmd 'au CursorMoved <buffer> ++once echo'
                    end, 64)
                end
                _G.page_bound = function(top, message, move)
                    local row, col, search
                    if top then
                        row, col, search = 1, 1, { '\\S', 'c' }
                    else
                        row, col, search = 9999999999, 9999999999, { '\\S', 'bc' }
                    end
                    vim.api.nvim_call_function('cursor', { row, col })
                    vim.api.nvim_call_function('search', search)
                    if move ~= nil then move() end
                    _G.echo_notification(message)
                end
                _G.page_scroll = function(top, message)
                    vim.wo.scrolloff = 0
                    local move
                    if top then
                        local key = vim.api.nvim_replace_termcodes('z<CR>M', true, false, true)
                        move = function() vim.api.nvim_feedkeys(key, 'nx', true) end
                    else
                        move = function() vim.api.nvim_feedkeys('z-M', 'nx', false) end
                    end
                    _G.page_bound(top, message, move)
                    vim.wo.scrolloff = 0
                end
                local map_opts = { nowait = true }
                vim.api.nvim_buf_set_keymap(0, '', 'I', '<CMD>lua _G.page_scroll(true, "in the beginning of scroll")<CR>', map_opts)
                vim.api.nvim_buf_set_keymap(0, '', 'A', '<CMD>lua _G.page_scroll(false, "at the end of scroll")<CR>', map_opts)
                vim.api.nvim_buf_set_keymap(0, '', 'i', '<CMD>lua _G.page_bound(true, "in the beginning")<CR>', map_opts)
                vim.api.nvim_buf_set_keymap(0, '', 'a', '<CMD>lua _G.page_bound(false, "at the end")<CR>', map_opts)
                vim.api.nvim_buf_set_keymap(0, '', 'u', '<C-u>', map_opts)
                vim.api.nvim_buf_set_keymap(0, '', 'd', '<C-d>', map_opts)
                vim.api.nvim_buf_set_keymap(0, '', 'x', 'G', map_opts)
            "#})
        }
        OutputCommands {
            ft: String::new(),
            pre: String::new(),
            post: String::new(),
            notify_closed: String::new(),
            edit,
            provided_by_user,
        }
    }

    pub fn for_file_buffer(cmd_provided_by_user: &str, writeable: bool) -> OutputCommands {
        let mut cmds = Self::create_with(cmd_provided_by_user, writeable);
        cmds.post.push_str("vim.cmd 'silent doautocmd User PageOpenFile'");
        cmds
    }

    pub fn for_output_buffer(page_id: &str, channel: u64, query_lines_count: usize, opt: &crate::cli::OutputOptions) -> OutputCommands {
        let mut cmds = Self::create_with(opt.command.as_deref().unwrap_or_default(), opt.writable);
        cmds.ft = format!("vim.bo.filetype = '{}'", opt.filetype);
        cmds.notify_closed = formatdoc! {r#"
            vim.cmd 'autocmd BufDelete <buffer> silent! call rpcnotify({channel}, "page_buffer_closed", "{page_id}")'
        "#,
            channel = channel,
            page_id = page_id,
        };
        if query_lines_count != 0 {
            cmds.pre = formatdoc! {r#"
                {prefix}
                vim.b.page_query_size = {query_lines_count}
                local query = 'command! -nargs=? Page call rpcnotify({channel}, "page_fetch_lines", "{page_id}", <args>)'
                vim.cmd(query)
                vim.cmd('autocmd BufEnter <buffer> ' .. query)
            "#,
                query_lines_count = query_lines_count,
                prefix = cmds.pre,
                page_id = page_id,
                channel = channel,
            };
            if !opt.writable {
                cmds.pre.push_str(&formatdoc! {r#"
                    vim.api.nvim_buf_set_keymap(0, '', 'r', '<CMD>call rpcnotify({channel}, "page_fetch_lines", "{page_id}", b:page_query_size * v:count1)<CR>', map_opts)
                    vim.api.nvim_buf_set_keymap(0, '', 'R', '<CMD>call rpcnotify({channel}, "page_fetch_lines", "{page_id}", 99999)<CR>', map_opts)
                "#,
                    page_id = page_id,
                    channel = channel,
                })
            }
        }
        if opt.pwd {
            cmds.pre = formatdoc! {r#"
                {prefix}
                vim.b.page_lcd_backup = getcwd()
                vim.cmd 'lcd {pwd}'
                vim.cmd('autocmd BufEnter <buffer> lcd {pwd}')
                vim.cmd('autocmd BufLeave <buffer> exe "lcd" . b:page_lcd_backup')
            "#,
                prefix = cmds.pre,
                pwd = std::env::var("PWD").unwrap()
            };
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
    pub channel: u64,
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
        let channel = nvim.get_api_info().await.expect("No API info").get(0).expect("No channel").as_u64().expect("Channel not a number");
        let initial_win = nvim.get_current_win().await.expect("Cannot get initial window");
        let initial_buf = nvim.get_current_buf().await.expect("Cannot get initial buffer");
        let initial_buf_number = initial_buf.get_number().await.expect("Cannot get initial buffer number");
        NeovimConnection {
            nvim_proc,
            nvim_actions: NeovimActions { nvim, join },
            initial_buf_number,
            channel,
            initial_win_and_buf: (initial_win, initial_buf),
            rx
        }
    }

    /// Waits until child neovim closes. If no child neovim process spawned then it's safe to just exit from page
    pub async fn close_and_exit(nvim_connection: &mut NeovimConnection) -> ! {
        if let Some(ref mut process) = nvim_connection.nvim_proc {
            process.await.expect("Neovim spawned with error")
                .wait().await.expect("Neovim process died unexpectedly");
        }
        nvim_connection.nvim_actions.join.abort();
        std::process::exit(0)
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
