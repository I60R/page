# Page

[![Build Status](https://travis-ci.org/I60R/page.svg?branch=master)](https://travis-ci.org/I60R/page)
[![Lines Of Code](https://tokei.rs/b1/github/I60R/page)](https://github.com/I60R/page)

Allows you to redirect text into [neovim](https://github.com/neovim/neovim).
You can set it as `$PAGER` to view logs, diffs, various command outputs.

ANSI escape sequences will be interpreted by :term buffer, which makes it noticeably faster than [vimpager](https://github.com/rkitover/vimpager) and [nvimpager](https://github.com/lucc/nvimpager).
Also, text will be displayed instantly as it arrives - no need to wait until EOF.

Text from neovim :term buffer will be redirected directly into a new buffer in the same neovim instance - no nested neovim will be spawned.
That's by utilizing `$NVIM_LISTEN_ADDRESS` as [neovim-remote](https://github.com/mhinz/neovim-remote) does.

Ultimately, `page` will reuse all of neovim text editing+navigating+searching facilities and will pick all of plugins+mappings+options set in your neovim config.

## Usage

* *under regular terminal*

![usage under regular terminal](https://imgur.com/lxDCPpn.gif)

* *under neovim's terminal*

![usage under neovim's terminal](https://i.imgur.com/rcLEM6X.gif)

---

## CLI

<details><summary> expand `page --help`</summary>

```xml
USAGE:
    page [OPTIONS] [FILE]...

OPTIONS:
    -o                           Create and use output buffer (to redirect text from page's stdin)
                                 [implied by default unless -x and/or <FILE> provided without
                                 other flags]
    -O <open-lines>              Prefetch <open-lines> from page's stdin: if input is smaller then
                                 print it to stdout and exit without neovim usage [empty: term
                                 height; 0: disabled and default; ignored with -o, -p, -x and when
                                 page isn't piped]
    -p                           Print path of pty device associated with output buffer (to
                                 redirect text from commands respecting output buffer size and
                                 preserving colors) [implied if page isn't piped unless -x and/or
                                 <FILE> provided without other flags]
    -P                           Set $PWD as working directory at output buffer (to navigate paths
                                 with `gf`)
    -q <query-lines>             Read no more than <query-lines> from page's stdin: next lines
                                 should be fetched by invoking :Page <query> command on neovim
                                 side [0: disabled and default; <query> is optional and defaults
                                 to <query-lines>]
    -f                           Cursor follows content of output buffer as it appears instead of
                                 keeping top position (like `tail -f`)
    -F                           Cursor follows content of output and <FILE> buffers as it appears
                                 instead of keeping top position
    -t <filetype>                Set filetype on output buffer (to enable syntax highlighting)
                                 [pager: default; not works with text echoed by -O]
    -b                           Return back to current buffer
    -B                           Return back to current buffer and enter into INSERT/TERMINAL mode
    -n <name>                    Set title for output buffer (to display it in statusline) [env:
                                 PAGE_BUFFER_NAME=./page -h]
    -w                           Allow to ender into INSERT/TERMINAL mode by pressing i, I, a, A
                                 keys [ignored on connected instance output buffer]
                                  ~ ~ ~
    -a <address>                 TCP/IP socked address or path to named pipe listened by running
                                 host neovim process [env: NVIM_LISTEN_ADDRESS=/tmp/nvimPwYcjt/0]
    -A <arguments>               Arguments that will be passed to child neovim process spawned
                                 when <address> is missing [env: NVIM_PAGE_ARGS=]
    -c <config>                  Config that will be used by child neovim process spawned when
                                 <address> is missing [file:$XDG_CONFIG_HOME/page/init.vim]
    -C                           Enable PageConnect PageDisconnect autocommands
    -e <command>                 Run command in output buffer after it was created
    -E <command-post>            Run command on output buffer after it was created or connected as
                                 instance
                                  ~ ~ ~
    -i <instance>                Create output buffer with <instance> tag or use existed with
                                 replacing its content by text from page's stdin
    -I <instance-append>         Create output buffer with <instance_append> tag or use existed
                                 with appending to its content text from page's stdin
    -x <instance-close>          Close output buffer with <instance_close> tag if it exists
                                 [without other flags revokes implied by defalt -o or -p option]
                                  ~ ~ ~
    -W                           Flush redirection protection that prevents from producing junk
                                 and possible overwriting of existed files by invoking commands
                                 like `ls > $(NVIM_LISTEN_ADDRESS= page -E q)` where the RHS of >
                                 operator evaluates not into /path/to/pty as expected but into a
                                 bunch of whitespace-separated strings/escape sequences from
                                 neovim UI; bad things happens when some shells interpret this as
                                 many valid targets for text redirection. The protection is only
                                 printing of a path to the existed dummy directory always first
                                 before printing of a neovim UI might occur; this makes the first
                                 target for text redirection from page's output invalid and
                                 disrupts the whole redirection early before other harmful writes
                                 might occur. [env:PAGE_REDIRECTION_PROTECT; (0 to disable)]
                                  ~ ~ ~
    -l                           Split left  with ratio: window_width  * 3 / (<l-provided> + 1)
    -r                           Split right with ratio: window_width  * 3 / (<r-provided> + 1)
    -u                           Split above with ratio: window_height * 3 / (<u-provided> + 1)
    -d                           Split below with ratio: window_height * 3 / (<d-provided> + 1)
    -L <split-left-cols>         Split left  and resize to <split-left-cols>  columns
    -R <split-right-cols>        Split right and resize to <split-right-cols> columns
    -U <split-above-rows>        Split above and resize to <split-above-rows> rows
    -D <split-below-rows>        Split below and resize to <split-below-rows> rows
                                  ^
    -+                           With any of -r -l -u -d -R -L -U -D open floating window instead
                                 of split [to not override data in the current terminal]
                                  ~ ~ ~
    -h, --help                   Prints help information
    -V, --version                Prints version information

ARGS:
    <FILE>...    Open provided file in separate buffer [without other flags revokes implied by
                 default -o or -p option]
```

</details>

## Customization

Statusline appearance settings:

```lua
vim.g.page_icon_instance = '$'
vim.g.page_icon_redirect = '>'
vim.g.page_icon_pipe = '|'
```

Autocommand hooks:

```lua
-- Will be run once when output buffer is created
vim.api.create_autocmd('User', {
    pattern = 'PageOpen',
    callback = lua_function,
})

-- Will be run once when file buffer is created
vim.api.create_autocmd('User', {
    pattern = 'PageOpenFile',
    callback = lua_function,
})

-- Only if -C option provided.
-- Will be run always when output buffer is created
-- and also when `page` connects to instance `-i, -I` buffers:
vim.api.create_autocmd('User', {
    pattern = 'PageConnect',
    callback = lua_function,
})
vim.api.create_autocmd('User', {
    pattern = 'PageDisconnect',
    callback = lua_function,
})
```

Hotkey for closing `page` buffers on `<C-c>`:

```lua
_G.page_close = function(page_alternate_bufnr)
  local current_buffer_num = vim.api.nvim_get_current_buf()
  vim.api.nvim_buf_delete(current_buffer_num, { force = true })
  if current_buffer_num == page_alternate_bufnr and vim.api.nvim_get_mode() == 'n' then
    vim.cmd 'norm a'
  end
end

vim.api.nvim_create_autocmd('User', {
  pattern = 'PageOpen',
  command = [[
    map <buffer> <C-c> :lua page_close(vim.b.page_alternate_bufnr)<CR>
    tmap <buffer> <C-c> :lua page_close(vim.b.page_alternate_bufnr)<CR>
  ]]
})
```

## Shell hacks

To use as `$PAGER` without scrollback overflow:

```zsh
export PAGER="page -q 90000"
```

To use as `$MANPAGER` without errors:

```zsh
export MANPAGER="page -C -e 'au User PageDisconnect sleep 100m|%y p|enew! |bd! #|pu p|set ft=man'"
```

To override neovim config (create this file or use -c option):

```zsh
$XDG_CONFIG_HOME/page/init.vim
```

To circumvent neovim config picking:

```zsh
page -c NONE
```

To set output buffer name as first two words from invoked command (zsh only):

```zsh
preexec() {
    echo "${1// *|*}" | read -A words
    export PAGE_BUFFER_NAME="${words[@]:0:2}"
}
```

## Buffer defaults

These commands are run on each `page` buffer creation:

```lua
vim.b.page_alternate_bufnr = {initial_buf_nr}
if vim.wo.scrolloff > 999 or vim.wo.scrolloff < 0 then
    vim.g.page_scrolloff_backup = 0
else
    vim.g.page_scrolloff_backup = vim.wo.scrolloff
end
vim.bo.scrollback, vim.wo.scrolloff, vim.wo.signcolumn, vim.wo.number = 100000, 999, 'no', false
{filetype}
{cmd_edit}
vim.api.nvim_create_autocmd('BufEnter', {
    buffer = 0,
    callback = function() vim.wo.scrolloff = 999 end
})
vim.api.nvim_create_autocmd('BufLeave', {
    buffer = 0,
    callback = function() vim.wo.scrolloff = vim.g.page_scrolloff_backup end
})
{cmd_notify_closed}
{cmd_pre}
vim.cmd 'silent doautocmd User PageOpen | redraw'
{cmd_provided_by_user}
{cmd_after}
```

Where:

```lua
---{initial_buf_nr}
-- Is always set on all buffers created by page

'number of parent :term buffer or -1 when page isn't spawned from neovim terminal'
```

```lua
---{filetype}
-- Is set only on output buffers.
-- On files buffers filetypes are detected automatically.

vim.bo.filetype='value of -t argument or "pager"'
```

```lua
---{cmd_edit}
-- Is appended when no -w option provided

vim.bo.modifiable = false
_G.page_echo_notification = function(message)
    vim.defer_fn(function()
        vim.api.nvim_echo({{ "-- [PAGE] " .. message .. " --", 'Comment' }, }, false, {})
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
```

```lua
---{cmd_notify_closed}
-- Is set only on output buffers
vim.api.nvim_create_autocmd('BufDelete' {
    buffer = 0,
    command = 'silent! call rpcnotify({channel}, "page_buffer_closed", "{page_id}")'
})
```

```lua
---{cmd_pre}
-- Is appended when -q is provided

vim.b.page_query_size = {query_lines_count}
local query = 'command! -nargs=? Page call rpcnotify({channel}, "page_fetch_lines", "{page_id}", <args>)'
vim.cmd(query)
vim.api.create_autocmd('BufEnter' {
    buffer = 0,
    command = query,
})

-- Also if -q is provided and no -w provided

page_map('r', '<CMD>call rpcnotify({channel}, "page_fetch_lines", "{page_id}", b:page_query_size * v:count1)<CR>')
page_map('R', '<CMD>call rpcnotify({channel}, "page_fetch_lines", "{page_id}", 99999)<CR>')

-- If -P is provided ({pwd} is $PWD value)

vim.b.page_lcd_backup = getcwd()
vim.cmd 'lcd {pwd}'
vim.api.nvim_create_autocmd('BufEnter', {
    buffer = 0,
    command = 'lcd {pwd}'
})
vim.api.nvim_create_autocmd('BufLeave', {
    buffer = 0,
    command = 'exe "lcd" . b:page_lcd_backup'
})
```

```lua
---{cmd_provided_by_user}
-- Is appended when -e is provided

vim.cmd [====[{value of -e argument}]====]
```

```lua
---{cmd_after}
-- Is appended only on file buffers

vim.cmd 'silent doautocmd User PageOpenFile'
```

## Limitations

* Only ~100000 lines can be displayed (it's neovim terminal limit)
* Text that doesn't fit in window width on resize will be lost ([due to data structures inherited from vim](https://github.com/neovim/neovim/issues/2514#issuecomment-580035346))
* `MANPAGER=page -t man` not works because `set ft=man` fails on :term buffer (other filetypes may be affected as well)

## Installation

* From binaries
  * Grab binary for your platform from [releases](https://github.com/I60R/page/releases) (currently Linux and OSX are supported)

* Arch Linux:
  * Package [page-git](https://aur.archlinux.org/packages/page-git/) is available on AUR
  * Or: `git clone git@github.com:I60R/page.git && cd page && makepkg -ef && sudo pacman -U page-git*.pkg.tar.xz`

* Manually:
  * Install `rustup` from your distribution package manager
  * Configure toolchain: `rustup install stable && rustup default stable`
  * `git clone git@github.com:I60R/page.git && cd page && cargo install --path .`
