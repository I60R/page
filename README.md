# Page

[![Rust Build](https://github.com/I60R/page/actions/workflows/rust_build.yml/badge.svg)](https://github.com/I60R/page/actions/workflows/rust_build.yml)
[![Lines Of Code](https://tokei.rs/b1/github/I60R/page)](https://github.com/I60R/page)

Allows you to redirect text into [neovim](https://github.com/neovim/neovim).
You can set it as `$PAGER` to view logs, diffs, various command outputs.

ANSI escape sequences will be interpreted by :term buffer, which makes `page` noticeably faster than [vimpager](https://github.com/rkitover/vimpager) and [nvimpager](https://github.com/lucc/nvimpager).
And text will be displayed instantly as it arrives - no need to wait until EOF.

Also, text from neovim :term buffer will be redirected directly into a new buffer in the same neovim instance - no nested neovim will be spawned.
That's by utilizing `$NVIM` variable like [neovim-remote](https://github.com/mhinz/neovim-remote) does.

**Bonus**: another binary named `nv` is included, which reimplements `neovim-remote` but with interface similar to `page`. There's no intention to have all `nvim --remote` features — it should be only a simple file picker that prevents spawning nested neovim instance. Also, in contrast with `neovim-remote` there are some safeguards e.g. it won't open non-text files unless explicit flag is provided for that so `nv *` opens only text files in current directory. I recommend to read `--help` output and experiment with options a bit.

Ultimately, `page` and `nv` reuses all of neovim's text editing+navigating+searching facilities and will either facilitate all of plugins+mappings+options set in your neovim config.

## Usage

* *under regular terminal*

![usage under regular terminal](https://imgur.com/lxDCPpn.gif)

* *under neovim's terminal*

![usage under neovim's terminal](https://i.imgur.com/rcLEM6X.gif)

---

## CLI

<details><summary> expand <code>page --help</code></summary>

```xml
Usage: page [OPTIONS] [FILE]...

Arguments:
  [FILE]...  Open provided file in separate buffer [without other flags revokes implied by default -o or -p
             option]

Options:
  -o                         Create and use output buffer (to redirect text from page's stdin) [implied by
                             default unless -x and/or <FILE> provided without other flags]
  -O [<NOOPEN_LINES>]        Prefetch <NOOPEN_LINES> from page's stdin: if all input fits then print it to
                             stdout and exit without neovim usage (to emulate `less --quit-if-one-screen`)
                             [empty: term height - 3 (space for prompt); negative: term height -
                             <NOOPEN_LINES>; 0: disabled and default; ignored with -o, -p, -x and when page
                             isn't piped]
  -p                         Print path of pty device associated with output buffer (to redirect text from
                             commands respecting output buffer size and preserving colors) [implied if page
                             isn't piped unless -x and/or <FILE> provided without other flags]
  -P                         Set $PWD as working directory at output buffer (to navigate paths with `gf`)
  -q [<QUERY_LINES>]         Read no more than <QUERY_LINES> from page's stdin: next lines should be
                             fetched by invoking :Page <QUERY> command or 'r'/'R' keypress on neovim side
                             [empty: term height - 2 (space for tab and buffer lines); negative: term
                             height - <QUERY_LINES>; 0: disabled and default; <QUERY> is optional and
                             defaults to <QUERY_LINES>; doesn't take effect on <FILE> buffers]
  -f                         Cursor follows content of output buffer as it appears instead of keeping top
                             position (like `tail -f`)
  -F                         Cursor follows content of output and <FILE> buffers as it appears instead of
                             keeping top position
  -t <FILETYPE>              Set filetype on output buffer (to enable syntax highlighting) [pager: default;
                             not works with text echoed by -O]
  -b                         Return back to current buffer
  -B                         Return back to current buffer and enter into INSERT/TERMINAL mode
  -n <NAME>                  Set title for output buffer (to display it in statusline) [env:
                             PAGE_BUFFER_NAME=]
  -w                         Do not remap i, I, a, A, u, d, x, q (and r, R with -q) keys [wouldn't unmap on
                             connected instance output buffer]
                              ~ ~ ~
  -a <ADDRESS>               TCP/IP socked address or path to named pipe listened by running host neovim
                             process [env: NVIM=/run/user/1000/nvim.9389.0]
  -A <ARGUMENTS>             Arguments that will be passed to child neovim process spawned when <ADDRESS>
                             is missing [env: NVIM_PAGE_ARGS=]
  -c <CONFIG>                Config that will be used by child neovim process spawned when <ADDRESS> is
                             missing [file:$XDG_CONFIG_HOME/page/init.vim]
  -C                         Enable PageConnect PageDisconnect autocommands
  -e <COMMAND>               Run command  on output buffer after it was created
      --e <LUA>              Run lua expr on output buffer after it was created
  -E <COMMAND_POST>          Run command  on output buffer after it was created or connected as instance
      --E <LUA_POST>         Run lua expr on output buffer after it was created or connected as instance
                              ~ ~ ~
  -i <INSTANCE>              Create output buffer with <INSTANCE> tag or use existed with replacing its
                             content by text from page's stdin
  -I <INSTANCE_APPEND>       Create output buffer with <INSTANCE_APPEND> tag or use existed with appending
                             to its content text from page's stdin
  -x <INSTANCE_CLOSE>        Close  output buffer with <INSTANCE_CLOSE> tag if it exists [without other
                             flags revokes implied by defalt -o or -p option]
                              ~ ~ ~
  -W                         Flush redirection protection that prevents from producing junk and possible
                             overwriting of existed files by invoking commands like `ls > $(NVIM= page -E
                             q)` where the RHS of > operator evaluates not into /path/to/pty as expected
                             but into a bunch of whitespace-separated strings/escape sequences from neovim
                             UI; bad things happens when some shells interpret this as many valid targets
                             for text redirection. The protection is only printing of a path to the existed
                             dummy directory always first before printing of a neovim UI might occur; this
                             makes the first target for text redirection from page's output invalid and
                             disrupts the whole redirection early before other harmful writes might occur.
                             [env:PAGE_REDIRECTION_PROTECT; (0 to disable)]
                              ~ ~ ~
  -l...                      Split left  with ratio: window_width  * 3 / (<l-PROVIDED> + 1)
  -r...                      Split right with ratio: window_width  * 3 / (<r-PROVIDED> + 1)
  -u...                      Split above with ratio: window_height * 3 / (<u-PROVIDED> + 1)
  -d...                      Split below with ratio: window_height * 3 / (<d-PROVIDED> + 1)
  -L <SPLIT_LEFT_COLS>       Split left  and resize to <SPLIT_LEFT_COLS>  columns
  -R <SPLIT_RIGHT_COLS>      Split right and resize to <SPLIT_RIGHT_COLS> columns
  -U <SPLIT_ABOVE_ROWS>      Split above and resize to <SPLIT_ABOVE_ROWS> rows
  -D <SPLIT_BELOW_ROWS>      Split below and resize to <SPLIT_BELOW_ROWS> rows
                              ^
  -+                         With any of -r -l -u -d -R -L -U -D open floating window instead of split [to
                             not overwrite data in the current terminal]
                              ~ ~ ~
  -h, --help                 Print help information
```

</details>

<details><summary> expand <code>nv --help</code></summary>

```xml
Usage: nv [OPTIONS] [FILE]...

Arguments:
  [FILE]...  Open provided files as editable [if none provided nv opens last modified file in currend
             directory]

Options:
  -o                          Open non-text files including directories, binaries, images etc
  -O [<RECURSE_DEPTH>]        Ignoring [FILE] open all text files in the current directory and recursively
                              open all text files in its subdirectories [0: disabled and default; empty:
                              defaults to 1 and implied if no <RECURSE_DEPTH> provided; <RECURSE_DEPTH>:
                              also opens in subdirectories at this level of depth]
  -v                          Open in `page` instead (just postfix shortcut)
                               ~ ~ ~
  -f                          Open each [FILE] at last line
  -p <PATTERN>                Open and search for a specified <PATTERN>
  -P <PATTERN_BACKWARDS>      Open and search backwars for a specified <PATTERN_BACKWARDS>
  -b                          Return back to current buffer
  -B                          Return back to current buffer and enter into INSERT/TERMINAL mode
  -k                          Keep `nv` process until buffer is closed (for editing git commit message)
  -K                          Keep `nv` process until first write occur, then close buffer and neovim if
                              it was spawned by `nv`
                               ~ ~ ~
  -a <ADDRESS>                TCP/IP socket address or path to named pipe listened by running host neovim
                              process [env: NVIM=/run/user/1000/nvim.604327.0]
  -A <ARGUMENTS>              Arguments that will be passed to child neovim process spawned when <ADDRESS>
                              is missing [env: NVIM_PAGE_PICKER_ARGS=]
  -c <CONFIG>                 Config that will be used by child neovim process spawned when <ADDRESS> is
                              missing [file: $XDG_CONFIG_HOME/page/init.vim]
  -t <FILETYPE>               Override filetype on each [FILE] buffer (to enable custom syntax highlighting
                              [text: default]
                               ~ ~ ~
  -e <COMMAND>                Run command  on each [FILE] buffer after it was created
      --e <LUA>               Run lua expr on each [FILE] buffer after it was created
  -x <COMMAND_ONLY>           Just run command  with ignoring all other options
      --x <LUA_ONLY>          Just run lua expr with ignoring all other options
                               ~ ~ ~
  -l...                       Split left  with ratio: window_width  * 3 / (<l-PROVIDED> + 1)
  -r...                       Split right with ratio: window_width  * 3 / (<r-PROVIDED> + 1)
  -u...                       Split above with ratio: window_height * 3 / (<u-PROVIDED> + 1)
  -d...                       Split below with ratio: window_height * 3 / (<d-PROVIDED> + 1)
  -L <SPLIT_LEFT_COLS>        Split left  and resize to <SPLIT_LEFT_COLS>  columns
  -R <SPLIT_RIGHT_COLS>       Split right and resize to <SPLIT_RIGHT_COLS> columns
  -U <SPLIT_ABOVE_ROWS>       Split above and resize to <SPLIT_ABOVE_ROWS> rows
  -D <SPLIT_BELOW_ROWS>       Split below and resize to <SPLIT_BELOW_ROWS> rows
                               ^
  -+                          With any of -r -l -u -d -R -L -U -D open floating window instead of split
                              [to not overwrite data in the current terminal]
                               ~ ~ ~
  -h, --help                  Print help information
```

</details>

**Note**: `page` and `nv` may be unergonomic to type so I suggest users to create alias like `p` and `v`

## `nvim/init.lua` customizations

```lua
-- Opacity of popup window spawned with -+ option
vim.g.page_popup_winblend = 25
```

## `nvim/init.lua` customizations (pager only)

Statusline appearance:

```lua
-- String that will append to buffer name
vim.g.page_icon_pipe = '|' -- When piped
vim.g.page_icon_redirect = '>' -- When exposes pty device
vim.g.page_icon_instance = '$' -- When `-i, -I` flags provided
```

Autocommand hooks:

```lua
-- Will run once when output buffer is created
vim.api.create_autocmd('User', {
    pattern = 'PageOpen',
    callback = lua_function,
})

-- Will run once when file buffer is created
vim.api.create_autocmd('User', {
    pattern = 'PageOpenFile',
    callback = lua_function,
})

-- Only with -C option provided: --

-- will run always when output buffer is created
-- and also when `page` connects to instance `-i, -I` buffers:
vim.api.create_autocmd('User', {
    pattern = 'PageConnect',
    callback = lua_function,
})

-- Will run when page process exits
vim.api.create_autocmd('User', {
    pattern = 'PageDisconnect',
    callback = lua_function,
})
```

---

Example: close `page` buffer on `<C-c>` hotkey:

```lua
_G.page_close = function(page_alternate_buf)
    local current_buf = vim.api.nvim_get_current_buf()
    vim.api.nvim_buf_delete(current_buf, { force = true })
    -- reenter into terminal mode
    if current_buf == page_alternate_buf and
        vim.api.nvim_get_mode() == 'n'
    then
        vim.cmd 'norm a'
    end
end

vim.api.nvim_create_autocmd('User', {
    pattern = 'PageOpen',
    callback = function()
        vim.api.nvim_set_keymap('n', '<C-c>', function()
            page_close(vim.b.page_alternate_bufnr)
        end, { buffer = 0 })
        vim.api.nvim_set_keymap('t', '<C-c>', function()
            page_close(vim.b.page_alternate_bufnr)
        end, { buffer = 0 })
    end
})
```

## Shell hacks

To use as `$PAGER` without [scrollback overflow](https://github.com/I60R/page/issues/7):

```zsh
export PAGER="page -q 90000"
```

To use as `$MANPAGER`:

```zsh
export MANPAGER="page -t man"
```

To pick a bit better neovim's native `man` highlighting:

```zsh
man () {
    PROGRAM="${@[-1]}"
    SECTION="${@[-2]}"
    page "man://$PROGRAM${SECTION:+($SECTION)}"
}
```

To set `nv` as popup `git` commit message editor:

```zsh
 git config --global core.editor "nv -K -+-R 80 -B"
```

To cd into directory passed to `nv`

```zsh
nv() {
    #stdin_is_term one_argument    it's_dir
    if [ -t 1 ] && [ 1 -eq $# ] && [ -d $1 ]; then
        cd $1
    else
        nv $*
    fi
}

compdef _nv nv # if you have completions installed
```

To circumvent neovim config picking:

```zsh
page -c NONE
```

To override neovim config (create this file or use -c option):

```zsh
$XDG_CONFIG_HOME/page/init.lua # init.vim is also supported
```

To set output buffer name as first two words from invoked command (zsh only):

```zsh

preexec () {
    [ -z "$NVIM" ] && return
    WORDS=(${1// *|*})
    export PAGE_BUFFER_NAME="${WORDS[@]:0:2}"
}
```

## Buffer defaults (pager)

These commands are run on each `page` buffer creation:

```lua
vim.b.page_alternate_bufnr = {$initial_buf_nr}
if vim.wo.scrolloff > 999 or vim.wo.scrolloff < 0 then
    vim.g.page_scrolloff_backup = 0
else
    vim.g.page_scrolloff_backup = vim.wo.scrolloff
end
vim.bo.scrollback, vim.wo.scrolloff, vim.wo.signcolumn, vim.wo.number =
    100000, 999, 'no', false
{$filetype}
{$edit}
vim.api.nvim_create_autocmd('BufEnter', {
    buffer = 0,
    callback = function() vim.wo.scrolloff = 999 end
})
vim.api.nvim_create_autocmd('BufLeave', {
    buffer = 0,
    callback = function() vim.wo.scrolloff = vim.g.page_scrolloff_backup end
})
{$notify_closed}
{$pre}
vim.cmd 'silent doautocmd User PageOpen | redraw'
{$lua_provided_by_user}
{$cmd_provided_by_user}
{$after}
```

Where:

```lua
--{$initial_buf_nr}
-- Is always set on all buffers created by page

'number of parent :term buffer or -1 when page isn't spawned from :term'
```

```lua
--{$filetype}
-- Is set only on output buffers.
-- On files buffers filetypes are detected automatically.

vim.bo.filetype='value of -t argument or "pager"'
```

```lua
--{$edit}
-- Is appended when no -w option provided

vim.bo.modifiable = false
_G.page_echo_notification = function(message)
    vim.defer_fn(function()
        local msg = "-- [PAGE] " .. message .. " --"
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
--{$notify_closed}
-- Is set only on output buffers

local closed = 'rpcnotify({channel}, "page_buffer_closed", "{page_id}")'
vim.api.nvim_create_autocmd('BufDelete', {
    buffer = 0,
    command = 'silent! call ' .. closed
})
```

```lua
--{$pre}
-- Is appended when -q provided

vim.b.page_query_size = {$query_lines_count}
local def_args = '{channel}, "page_fetch_lines", "{page_id}", '
local def = 'command! -nargs=? Page call rpcnotify(' .. def_args .. '<args>)'
vim.cmd(def)
vim.api.create_autocmd('BufEnter', {
    buffer = 0,
    command = def,
})

-- Also if -q provided and no -w provided

page_map('r', '<CMD>call rpcnotify(' .. def_args .. 'b:page_query_size * v:count1)<CR>')
page_map('R', '<CMD>call rpcnotify(' .. def_args .. '99999)<CR>')

-- If -P provided ({pwd} is $PWD value)

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
--{$lua_provided_by_user}
-- Is appended when --e provided

'value of --e flag'
```

```lua
--{$cmd_provided_by_user}
-- Is appended when -e provided

vim.cmd [====[{$command}]====]
```

```lua
--{$after}
-- Is appended only on file buffers

vim.api.nvim_exec_autocmds('User', {
    pattern = 'PageOpenFile',
})
```

## Limitations (pager)

* Only ~100000 lines can be displayed (that's neovim terminal limit)
* No reflow: text that doesnt't fit into window will be lost on resize  ([due to data structures inherited from vim](https://github.com/neovim/neovim/issues/2514#issuecomment-580035346))

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
