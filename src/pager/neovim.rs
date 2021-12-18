/// A module that extends neovim api with methods required in page
use nvim_rs::{neovim::Neovim, error::CallError, Buffer, Window, Value};
use indoc::{indoc, formatdoc};
use page::connection::IoWrite;
use std::{path::PathBuf, convert::TryFrom};


/// This struct wraps nvim_rs::Neovim and decorates it with methods required in page.
/// Results returned from underlying Neovim methods are mostly unwrapped, since we anyway cannot provide
/// any meaningful falback logic on call side
pub struct NeovimActions {
    nvim: Neovim<IoWrite>,
}

impl From<Neovim<IoWrite>> for NeovimActions {
    fn from(nvim: Neovim<IoWrite>) -> Self {
        NeovimActions { nvim }
    }
}

impl NeovimActions {
    pub async fn get_current_buffer(&mut self) -> Result<Buffer<IoWrite>, Box<CallError>> {
        self.nvim.get_current_buf().await
    }

    pub async fn create_replacing_output_buffer(&mut self) -> OutputBuffer {
        let cmd = indoc! {"
            local buf = vim.api.nvim_get_current_buf()
        "};
        self.create_buffer(cmd).await.expect("Error when creating output buffer from current")
    }

    pub async fn create_switching_output_buffer(&mut self) -> OutputBuffer {
        let cmd = indoc! {"
            local buf = vim.api.nvim_create_buf(true, false)
            vim.api.nvim_set_current_buf(buf)
        "};
        self.create_buffer(cmd).await.expect("Error when creating output buffer")
    }

    pub async fn create_split_output_buffer(&mut self, opt: &crate::cli::SplitOptions) -> OutputBuffer {
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

    async fn create_buffer(&mut self, window_open_cmd: &str) -> Result<OutputBuffer, String> {
        // Shell will be temporarily replaced with /bin/sleep that will halt for i32::MAX days
        let cmd = formatdoc! {"
            local shell, shellcmdflag = vim.o.shell, vim.o.shellcmdflag
            vim.o.shell, vim.o.shellcmdflag = 'sleep', ''
            {window_open_cmd}
            local chan = vim.api.nvim_call_function('termopen', {{ '2147483647d' }})
            vim.o.shell, vim.o.shellcmdflag = shell, shellcmdflag
            local pty = vim.api.nvim_get_chan_info(chan).pty
            if pty == nil or pty == '' then error 'No PTY on channel' end
            return {{ buf, pty }}
        ",
            window_open_cmd = window_open_cmd
        };
        log::trace!(target: "create buffer", "{}", cmd);
        let v = self.nvim.exec_lua(&cmd, vec![]).await.expect("Cannot create buffer");
        OutputBuffer::try_from((v, &self.nvim))
    }


    pub async fn mark_buffer_as_instance(&mut self, buf: &Buffer<IoWrite>, inst_name: &str, inst_pty_path: &str) {
        log::trace!(target: "new instance", "{:?}->{}->{}", buf.get_value(), inst_name, inst_pty_path);
        let v = Value::from(vec![Value::from(inst_name), Value::from(inst_pty_path)]);
        if let Err(e) = buf.set_var("page_instance", v).await {
            log::error!(target: "new instance", "Error when setting instance mark: {}", e);
        }
    }

    pub async fn find_instance_buffer(&mut self, inst_name: &str) -> Option<OutputBuffer> {
        log::trace!(target: "find instance", "{}", inst_name);
        let value = self.on_instance(inst_name, "return { buf, pty_path }").await.expect("Cannot find instance buffer");
        if value.is_nil() {
            return None
        }
        match OutputBuffer::try_from((value, &self.nvim)) {
            Ok(b) => Some(b),
            Err(e) => {
                log::error!(target: "find instance", "Wrong response: {}", e);
                None
            }
        }
    }

    pub async fn close_instance_buffer(&mut self, inst_name: &str) {
        log::trace!(target: "close instance", "{}", inst_name);
        if let Err(e) = self.on_instance(inst_name, "vim.api.nvim_buf_delete(buf, {{ force = true }})").await {
            log::error!(target: "close instance", "Error when closing instance buffer: {}, {}", inst_name, e);
        }
    }

    pub async fn focus_instance_buffer(&mut self, inst_name: &str) {
        log::trace!(target: "focus instance", "{}", inst_name);
        let cmd = indoc! {"
            local active_buf = vim.api.nvim_get_current_buf()
            if active_buf == buf then return end
            for _, win in ipairs(vim.api.nvim_list_wins()) do
                local win_buf = vim.api.nvim_win_get_buf(win)
                if win_buf == buf then
                    vim.api.nvim_set_current_win(win)
                    return
                end
            end
            vim.api.nvim_set_current_buf(buf)
        "};
        self.on_instance(inst_name, cmd).await.expect("Cannot focus on instance buffer");
    }

    async fn on_instance(&mut self, inst_name: &str, action: &str) -> Result<Value, Box<CallError>> {
        let cmd = formatdoc! {"
            for _, buf in ipairs(vim.api.nvim_list_bufs()) do
                local inst_name, pty_path
                local ok = pcall(function() inst_name, pty_path = unpack(vim.api.nvim_buf_get_var(buf, 'page_instance')) end)
                if ok and inst_name == '{inst_name}' then
                    {action}
                end
            end
        ",
            inst_name = inst_name,
            action = action,
        };
        self.nvim.exec_lua(&cmd, vec![]).await
    }


    pub async fn update_buffer_title(&mut self, buf: &Buffer<IoWrite>, buf_title: &str) {
        log::trace!(target: "update title", "{:?} => {}", buf.get_value(), buf_title);
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
        log::trace!(target: "set buffer", "{:?}", buf.get_value());
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


/// This struct holds output buffer together with path to its PTY
pub struct OutputBuffer {
    pub buf: Buffer<IoWrite>,
    pub pty_path: PathBuf,
}

impl TryFrom<(Value, &Neovim<IoWrite>)> for OutputBuffer {
    type Error = String;

    fn try_from((val, nvim): (Value, &Neovim<IoWrite>)) -> Result<Self, Self::Error> {
        let tup = val.as_array().ok_or("Response is not an array")?;
        let (buf_val, pty_val) = (tup.get(0).ok_or("No buf handle")?, tup.get(1).ok_or("No pty handle")?.as_str().ok_or("PTY not a string")?);
        let (buf, pty) = (Buffer::new(buf_val.clone(), nvim.clone()), PathBuf::from(pty_val));
        Ok(OutputBuffer { buf, pty_path: pty })
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
                _G.page_echo_notification = function(message)
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
                    _G.page_echo_notification(message)
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
                    vim.wo.scrolloff = 999
                end
                _G.page_close = function()
                    local buf = vim.api.nvim_get_current_buf()
                    if buf ~= vim.b.page_alternate_bufnr and vim.api.nvim_buf_is_loaded(vim.b.page_alternate_bufnr) then
                        vim.api.nvim_set_current_buf(vim.b.page_alternate_bufnr)
                    end
                    vim.api.nvim_buf_delete(buf, { force = true })
                    local exit = true
                    for _, b in ipairs(vim.api.nvim_list_bufs()) do
                        local bt = vim.api.nvim_buf_get_option(b, 'buftype')
                        if bt == "" or bt == "acwrite" or bt == "terminal" or bt == "prompt" then
                            local bm = vim.api.nvim_buf_get_option(b, 'modified')
                            if bm then
                                exit = false
                                break
                            end
                            local bl = vim.api.nvim_buf_get_lines(b, 0, -1, false)
                            if #bl ~= 0 and bl[1] ~= "" and #bl > 1 then
                                exit = false
                                break
                            end
                        end
                    end
                    if exit then
                        vim.cmd "qa!"
                    end
                end
                local function page_map(key, expr)
                    vim.api.nvim_buf_set_keymap(0, '', key, expr, { nowait = true })
                end
                page_map('I', '<CMD>lua _G.page_scroll(true, "in the beginning of scroll")<CR>')
                page_map('A', '<CMD>lua _G.page_scroll(false, "at the end of scroll")<CR>')
                page_map('i', '<CMD>lua _G.page_bound(true, "in the beginning")<CR>')
                page_map('a', '<CMD>lua _G.page_bound(false, "at the end")<CR>')
                page_map('q', '<CMD>lua _G.page_close()<CR>')
                page_map('u', '<C-u>')
                page_map('d', '<C-d>')
                page_map('x', 'G')
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
                    page_map('r', '<CMD>call rpcnotify({channel}, "page_fetch_lines", "{page_id}", b:page_query_size * v:count1)<CR>')
                    page_map('R', '<CMD>call rpcnotify({channel}, "page_fetch_lines", "{page_id}", 99999)<CR>')
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