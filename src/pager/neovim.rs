/// A module that extends neovim api with methods required in page
use nvim_rs::{neovim::Neovim, error::CallError, Buffer, Window, Value};
use indoc::{indoc, formatdoc};
use connection::IoWrite;
use std::{path::PathBuf, convert::TryFrom};


/// This struct wraps nvim_rs::Neovim and decorates it
/// with methods required in page. Results returned from underlying
/// Neovim methods are mostly unwrapped, since we anyway cannot provide
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
        self.nvim
            .get_current_buf()
            .await
    }


    pub async fn create_replacing_output_buffer(&mut self) -> OutputBuffer {
        let cmd = indoc! {"
            local buf = vim.api.nvim_get_current_buf()
        "};

        self.create_buffer(cmd)
            .await
            .expect("Error when creating output buffer from current")
    }


    pub async fn create_switching_output_buffer(&mut self) -> OutputBuffer {
        let cmd = indoc! {"
            local buf = vim.api.nvim_create_buf(true, false)
            vim.api.nvim_set_current_buf(buf)
        "};

        self.create_buffer(cmd)
            .await
            .expect("Error when creating output buffer")
    }


    pub async fn create_split_output_buffer(
        &mut self,
        opt: &crate::cli::SplitOptions
    ) -> OutputBuffer {

        let cmd = if opt.popup {

            let w_ratio = |s| format!("math.floor(((w / 2) * 3) / {})", s + 1);
            let h_ratio = |s| format!("math.floor(((h / 2) * 3) / {})", s + 1);

            let (w, h, o) = ("w".to_string(), "h".to_string(), "0".to_string());

            let (width, height, row, col);

            if opt.split_right != 0 {
                (width = w_ratio(opt.split_right), height = h, row = &o, col = &w)

            } else if opt.split_left != 0 {
                (width = w_ratio(opt.split_left),  height = h, row = &o, col = &o)

            } else if opt.split_below != 0 {
                (width = w, height = h_ratio(opt.split_below), row = &h, col = &o)

            } else if opt.split_above != 0 {
                (width = w, height = h_ratio(opt.split_above), row = &o, col = &o)

            } else if let Some(split_right_cols) = opt.split_right_cols.map(|x| x.to_string()) {
                (width = split_right_cols, height = h, row = &o, col = &w)

            } else if let Some(split_left_cols) = opt.split_left_cols.map(|x| x.to_string()) {
                (width = split_left_cols,  height = h, row = &o, col = &o)

            } else if let Some(split_below_rows) = opt.split_below_rows.map(|x| x.to_string()) {
                (width = w, height = split_below_rows, row = &h, col = &o)

            } else if let Some(split_above_rows) = opt.split_above_rows.map(|x| x.to_string()) {
                (width = w, height = split_above_rows, row = &o, col = &o)

            } else {
                unreachable!()
            };

            formatdoc! {"
                local w = vim.api.nvim_win_get_width(0)
                local h = vim.api.nvim_win_get_height(0)
                local buf = vim.api.nvim_create_buf(true, false)
                local win = vim.api.nvim_open_win(buf, true, {{
                    relative = 'editor',
                    width = {width},
                    height = {height},
                    row = {row},
                    col = {col}
                }})
                vim.api.nvim_set_current_win(win)
                vim.api.nvim_win_set_option(win, 'winblend', 25)
            "}
        } else {

            let w_ratio = |s| format!("' .. tostring(math.floor(((w / 2) * 3) / {})) .. '", s + 1);
            let h_ratio = |s| format!("' .. tostring(math.floor(((h / 2) * 3) / {})) .. '", s + 1);

            let (a, b) = ("aboveleft", "belowright");
            let (w, h) = ("winfixwidth", "winfixheight");
            let (v, z) = ("vsplit", "split");

            let (direction, size, split, fix);

            if opt.split_right != 0 {
                (direction = b, size = w_ratio(opt.split_right), split = v, fix = w)

            } else if opt.split_left != 0 {
                (direction = a,  size = w_ratio(opt.split_left), split = v, fix = w)

            } else if opt.split_below != 0 {
                (direction = b, size = h_ratio(opt.split_below), split = z, fix = h)

            } else if opt.split_above != 0 {
                (direction = a, size = h_ratio(opt.split_above), split = z, fix = h)

            } else if let Some(split_right_cols) = opt.split_right_cols.map(|x| x.to_string()) {
                (direction = b, size = split_right_cols, split = v, fix = w)

            } else if let Some(split_left_cols) = opt.split_left_cols.map(|x| x.to_string()) {
                (direction = a, size = split_left_cols,  split = v, fix = w)

            } else if let Some(split_below_rows) = opt.split_below_rows.map(|x| x.to_string()) {
                (direction = b, size = split_below_rows, split = z, fix = h)

            } else if let Some(split_above_rows) = opt.split_above_rows.map(|x| x.to_string()) {
                (direction = a, size = split_above_rows, split = z, fix = h)

            } else {
                unreachable!()
            };

            formatdoc! {"
                local prev_win, win = vim.api.nvim_get_current_win()
                local w = vim.api.nvim_win_get_width(prev_win)
                local h = vim.api.nvim_win_get_height(prev_win)
                vim.cmd('{direction} {size}{split}')
                local buf = vim.api.nvim_create_buf(true, false)
                vim.api.nvim_set_current_buf(buf)
                vim.api.nvim_win_set_option(win, '{fix}', true)
            "}
        };

        self.create_buffer(&cmd)
            .await
            .expect("Error when creating split output buffer")
    }


    async fn create_buffer(
        &mut self,
         window_open_cmd: &str
    ) -> Result<OutputBuffer, String> {
        // Shell will be temporarily replaced with /bin/sleep to halt for i32::MAX days
        let cmd = formatdoc! {"
            local shell, shellcmdflag = vim.o.shell, vim.o.shellcmdflag
            vim.o.shell, vim.o.shellcmdflag = 'sleep', ''
            {window_open_cmd}
            local chan = vim.api.nvim_call_function('termopen', {{ '2147483647d' }})
            vim.o.shell, vim.o.shellcmdflag = shell, shellcmdflag
            local pty = vim.api.nvim_get_chan_info(chan).pty
            if pty == nil or pty == '' then
                error 'No PTY on channel'
            end
            return {{ buf, pty }}
        "};
        log::trace!(target: "create buffer", "{cmd}");

        let v = self.nvim
            .exec_lua(&cmd, vec![])
            .await
            .expect("Cannot create buffer");

        OutputBuffer::try_from((v, &self.nvim))
    }


    pub async fn mark_buffer_as_instance(
        &mut self,
        buf: &Buffer<IoWrite>,
        inst_name: &str,
        inst_pty_path: &str
    ) {
        let bv = buf.get_value();
        log::trace!(target: "new instance", "{:?}->{inst_name}->{inst_pty_path}", bv);

        let v = Value::from(vec![
            Value::from(inst_name),
            Value::from(inst_pty_path)
        ]);

        if let Err(e) = buf
            .set_var("page_instance", v)
            .await
        {
            log::error!(target: "new instance", "Error when setting instance mark: {e}");
        }
    }


    pub async fn find_instance_buffer(
        &mut self,
        inst_name: &str
    ) -> Option<OutputBuffer> {
        log::trace!(target: "find instance", "{inst_name}");

        let value = self
            .on_instance(inst_name, "return { buf, pty_path }")
            .await
            .expect("Cannot find instance buffer");

        if value.is_nil() {
            return None
        }

        let buf = OutputBuffer::try_from((value, &self.nvim));
        if let Err(e) = &buf {
            log::error!(target: "find instance", "Wrong response: {e}");
        }

        buf.ok()
    }


    pub async fn close_instance_buffer(&mut self, inst_name: &str) {
        log::trace!(target: "close instance", "{inst_name}");

        if let Err(e) = self
            .on_instance(inst_name, "vim.api.nvim_buf_delete(buf, {{ force = true }})")
            .await
        {
            log::error!(
                target: "close instance",
                "Error closing instance buffer: {inst_name}, {e}"
            );
        }
    }


    pub async fn focus_instance_buffer(&mut self, inst_name: &str) {
        log::trace!(target: "focus instance", "{inst_name}");

        let cmd = indoc! {"
            local active_buf = vim.api.nvim_get_current_buf()
            if active_buf == buf then
                return
            end
            for _, win in ipairs(vim.api.nvim_list_wins()) do
                local win_buf = vim.api.nvim_win_get_buf(win)
                if win_buf == buf then
                    vim.api.nvim_set_current_win(win)
                    return
                end
            end
            vim.api.nvim_set_current_buf(buf)
        "};

        self.on_instance(inst_name, cmd)
            .await
            .expect("Cannot focus on instance buffer");
    }


    async fn on_instance(
        &mut self,
        inst_name: &str,
        action: &str
    ) -> Result<Value, Box<CallError>> {
        let cmd = formatdoc! {"
            for _, buf in ipairs(vim.api.nvim_list_bufs()) do
                local inst_name, pty_path
                local ok = pcall(function()
                    local inst_val = vim.api.nvim_buf_get_var(buf, 'page_instance')
                    inst_name, pty_path = unpack(inst_val)
                end)
                if ok and inst_name == '{inst_name}' then
                    {action}
                end
            end
        "};

        self.nvim
            .exec_lua(&cmd, vec![])
            .await
    }


    pub async fn update_buffer_title(
        &mut self,
        buf: &Buffer<IoWrite>,
        buf_title: &str
    ) {
        let bn = buf
            .get_number()
            .await;
        log::trace!(target: "update title", "{bn:?} => {buf_title}");

        let retries = (1..99)
            .map(|attempt_nr| (attempt_nr, format!("{buf_title}({attempt_nr})")));

        for (attempt_nr, name) in std::iter::once((0, buf_title.to_string()))
            .chain(retries)
        {
            if let Err(e) = buf
                .set_name(&name)
                .await
            {
                log::trace!(
                    target: "update title",
                    "{bn:?} => {buf_title}: {:?}", e.to_string()
                );

                use CallError::NeovimError;
                match *e {
                    NeovimError(_, m)
                        if m == "Failed to rename buffer" && attempt_nr < 99 => {

                        continue
                    }
                    _ => {
                        log::error!(target: "update title", "Cannot update title: {e}");

                        return
                    }
                }
            } else {
                self.nvim
                    .command("redraw!")  // To update statusline
                    .await
                    .expect("Cannot redraw");

                return
            }
        }
    }


    pub async fn prepare_output_buffer(
        &mut self,
        initial_buf_nr: i64,
        cmds: OutputCommands
    ) {
        let OutputCommands { ft, edit, notify_closed, pre, provided_by_user, after } = cmds;

        let options = formatdoc! {r#"
            vim.b.page_alternate_bufnr = {initial_buf_nr}
            if vim.wo.scrolloff > 999 or vim.wo.scrolloff < 0 then
                vim.g.page_scrolloff_backup = 0
            else
                vim.g.page_scrolloff_backup = vim.wo.scrolloff
            end
            vim.bo.scrollback, vim.wo.scrolloff, vim.wo.signcolumn, vim.wo.number =
                100000, 999, 'no', false
            {ft}
            {edit}
            vim.api.nvim_create_autocmd('BufEnter', {{
                buffer = 0,
                callback = function() vim.wo.scrolloff = 999 end
            }})
            vim.api.nvim_create_autocmd('BufLeave', {{
                buffer = 0,
                callback = function() vim.wo.scrolloff = vim.g.page_scrolloff_backup end
            }})
            {notify_closed}
            {pre}
            vim.cmd 'silent doautocmd User PageOpen | redraw'
            {provided_by_user}
            {after}
        "#};
        log::trace!(target: "prepare output", "{options}");

        if let Err(e) = self.nvim
            .exec_lua(&options, vec![])
            .await
        {
            log::error!(
                target: "prepare output",
                "Unable to set page options, text might be displayed improperly: {e}"
            );
        }
    }


    pub async fn execute_connect_autocmd_on_current_buffer(&mut self) {
        log::trace!(target: "au PageConnect", "");

        if let Err(e) = self.nvim
            .command("silent doautocmd User PageConnect")
            .await
        {
            log::error!(target: "au PageConnect", "Cannot execute PageConnect: {e}");
        }
    }


    pub async fn execute_disconnect_autocmd_on_current_buffer(&mut self) {
        log::trace!(target: "au PageDisconnect", "");
        if let Err(e) = self.nvim
            .command("silent doautocmd User PageDisconnect")
            .await
        {
            log::error!(target: "au PageDisconnect", "Cannot execute PageDisconnect: {e}");
        }
    }


    pub async fn execute_command_post(&mut self, cmd: &str) {
        log::trace!(target: "command post", "{cmd}");

        if let Err(e) = self.nvim
            .command(cmd)
            .await
        {
            log::error!(target: "command post", "Error executing post command '{cmd}': {e}");
        }
    }


    pub async fn switch_to_window_and_buffer(
        &mut self,
        (win, buf): &(Window<IoWrite>, Buffer<IoWrite>)
    ) {
        let wn = win
            .get_number()
            .await;
        let bn = buf
            .get_number()
            .await;
        log::trace!(target: "set window and buffer", "Win:{wn:?} Buf:{bn:?}");

        if let Err(e) = self.nvim
            .set_current_win(win)
            .await
        {
            log::error!(target: "set window and buffer", "Cannot switch to window: {e}");
        }

        if let Err(e) = self.nvim
            .set_current_buf(buf)
            .await
        {
            log::error!(target: "set window and buffer", "Cannot switch to buffer: {e}");
        }
    }


    pub async fn switch_to_buffer(
        &mut self,
        buf: &Buffer<IoWrite>
    ) -> Result<(), Box<CallError>> {
        log::trace!(target: "set buffer", "{:?}", buf.get_value());

        self.nvim
            .set_current_buf(buf)
            .await
    }


    pub async fn set_current_buffer_insert_mode(&mut self) {
        log::trace!(target: "set INSERT", "");

        // Fixes "can't enter normal mode from..."
        if let Err(e) = self.nvim
            .command(r###"call feedkeys("\<C-\>\<C-n>A", 'n')"###)
            .await
        {
            log::error!(target: "set INSERT", "Error when setting mode: {e}");
        }
    }


    pub async fn set_current_buffer_follow_output_mode(&mut self) {
        log::trace!(target: "set FOLLOW", "");

        if let Err(e) = self.nvim
            .command(r###"call feedkeys("\<C-\>\<C-n>G, 'n'")"###)
            .await
        {
            log::error!(target: "set FOLLOW", "Error when setting mode: {e}");
        }
    }


    pub async fn set_current_buffer_scroll_mode(&mut self) {
        log::trace!(target: "set SCROLL", "");

        if let Err(e) = self.nvim
            .command(r###"call feedkeys("\<C-\>\<C-n>ggM, 'n'")"###)
            .await
        {
            log::error!(target: "set SCROLL", "Error when setting mode: {e}");
        }
    }


    pub async fn open_file_buffer(
        &mut self,
        file_opt: &crate::cli::FileOption,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::trace!(target: "open file", "{file_opt:?}");

        self.nvim
            .command(&format!("e {}", file_opt.as_str()))
            .await?;

        Ok(())
    }


    pub async fn notify_query_finished(&mut self, lines_read_count: usize) {
        log::trace!(target: "query finished", "Read {lines_read_count} lines");

        let cmd = formatdoc! {"
            vim.cmd 'redraw'
            local msg = '-- [PAGE] {lines_read_count} lines read; has more --'
            vim.api.nvim_echo({{ {{ msg, 'Comment', }}, }}, false, {{}})
        "};

        self.nvim
            .exec_lua(&cmd, vec![])
            .await
            .expect("Cannot notify query finished");
    }


    pub async fn notify_end_of_input(&mut self) {
        log::trace!(target: "end input", "");

        let cmd = indoc! {"
            vim.cmd 'redraw'
            local msg = '-- [PAGE] end of input --'
            vim.api.nvim_echo({{ msg, 'Comment' }}, }}, false, {{}})
        "};

        self.nvim
            .exec_lua(cmd, vec![])
            .await
            .expect("Cannot notify end of input");
    }


    pub async fn get_var_or(
        &mut self,
        key: &str,
        default: &str
    ) -> String {
        let val = self.nvim
            .get_var(key)
            .await
            .map(|v| v.to_string())
            .unwrap_or_else(|e| {
                use CallError::NeovimError;
                match *e {
                    NeovimError(_, m) if m == format!("Key not found: {key}") => {},
                    _ => {
                        log::error!(target: "get var", "Error getting var: {key}, {e}")
                    }
                }

                String::from(default)
            });
        log::trace!(target: "get var", "Key '{key}': '{val}'");

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

    fn try_from(
        (val, nvim): (Value, &Neovim<IoWrite>)
    ) -> Result<Self, Self::Error> {
        let tup = val
            .as_array()
            .ok_or("Response is not an array")?;
        let buf_val = tup
            .get(0)
            .ok_or("No buf handle")?;
        let pty_val = tup
            .get(1)
            .ok_or("No pty handle")?
            .as_str()
            .ok_or("PTY not a string")?;

        let buf = Buffer::new(buf_val.clone(), nvim.clone());
        let pty_path = PathBuf::from(pty_val);

        Ok(OutputBuffer { buf, pty_path })
    }
}


/// This struct provides commands that
/// would be run on output buffer after creation
pub struct OutputCommands {
    edit: String,
    ft: String,
    notify_closed: String,
    pre: String,
    provided_by_user: String,
    after: String,
}

impl OutputCommands {
    fn create_with(
        cmd_provided_by_user: &str,
        writeable: bool
    ) -> OutputCommands {
        let mut provided_by_user = String::from(cmd_provided_by_user);
        if !provided_by_user.is_empty() {
            provided_by_user = format!("vim.cmd [====[{provided_by_user}]====]");
        }

        let mut edit = String::new();
        if !writeable {
            let cmd = indoc! {r#"
                vim.bo.modifiable = false

                _G.page_echo_notification = function(message)
                    vim.defer_fn(function()
                        local msg = '-- [PAGE] ' .. message .. ' --'
                        vim.api.nvim_echo({{ msg, 'Comment' }, }, false, {})
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
                    if buf ~= vim.b.page_alternate_bufnr and
                        vim.api.nvim_buf_is_loaded(vim.b.page_alternate_bufnr)
                    then
                        vim.api.nvim_set_current_buf(vim.b.page_alternate_bufnr)
                    end
                    vim.api.nvim_buf_delete(buf, { force = true })
                    local exit = true
                    for _, b in ipairs(vim.api.nvim_list_bufs()) do
                        local bt = vim.api.nvim_buf_get_option(b, 'buftype')
                        if bt == "" or bt == "acwrite" or bt == "terminal" or bt == "prompt"
                        then
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
            "#};

            edit.push_str(cmd);
        }

        OutputCommands {
            ft: String::new(),
            pre: String::new(),
            after: String::new(),
            notify_closed: String::new(),
            edit,
            provided_by_user,
        }
    }


    pub fn for_file_buffer(
        cmd_provided_by_user: &str,
        writeable: bool
    ) -> OutputCommands {
        let mut cmds = Self::create_with(
            cmd_provided_by_user,
            writeable
        );
        cmds.after
            .push_str("vim.cmd 'silent doautocmd User PageOpenFile'");

        cmds
    }


    pub fn for_output_buffer(
        page_id: &str,
        channel: u64,
        query_lines_count: usize,
        opt: &crate::cli::OutputOptions
    ) -> OutputCommands {
        let cmd_provided_by_user = opt.command
            .as_deref()
            .unwrap_or_default();

        let mut cmds = Self::create_with(
            cmd_provided_by_user,
            opt.writable
        );

        let ft = &opt.filetype;
        cmds.ft = format!("vim.bo.filetype = '{ft}'");

        cmds.notify_closed = formatdoc! {r#"
            local closed = 'rpcnotify({channel}, "page_buffer_closed", "{page_id}")'
            vim.api.nvim_create_autocmd('BufDelete', {{
                buffer = 0,
                command = 'silent! call ' .. closed
            }})
        "#};

        if query_lines_count != 0 {

            let prefix = cmds.pre;
            cmds.pre = formatdoc! {r#"
                {prefix}
                vim.b.page_query_size = {query_lines_count}
                local def_args = '{channel}, "page_fetch_lines", "{page_id}", '
                local def = 'command! -nargs=? Page call rpcnotify(' .. def_args .. '<args>)'
                vim.cmd(def)
                vim.api.create_autocmd('BufEnter', {{
                    buffer = 0,
                    command = def,
                }})
            "#};

            if !opt.writable {

                let prefix = cmds.pre;
                cmds.pre = formatdoc! {r#"
                    {prefix}
                    page_map(
                        'r',
                        '<CMD>call rpcnotify(' .. def_args .. 'b:page_query_size * v:count1)<CR>'
                    )
                    page_map(
                        'R',
                        '<CMD>call rpcnotify(' .. def_args .. '99999)<CR>'
                    )
                "#};
            }
        }

        if opt.pwd {
            let pwd = std::env::var("PWD")
                .unwrap();

            let prefix = cmds.pre;
            cmds.pre = formatdoc! {r#"
                {prefix}
                vim.b.page_lcd_backup = getcwd()
                vim.cmd 'lcd {pwd}'
                vim.api.nvim_create_autocmd('BufEnter', {{
                    buffer = 0,
                    command = 'lcd {pwd}'
                }})
                vim.api.nvim_create_autocmd('BufLeave', {{
                    buffer = 0,
                    command = 'exe "lcd" . b:page_lcd_backup'
                }})
            "#};
        }

        cmds
    }
}
