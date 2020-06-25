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
    -F                           Cursor follows content of output and <FIlE> buffers as it appears
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

```viml
let g:page_icon_instance = '$'
let g:page_icon_redirect = '>'
let g:page_icon_pipe = '|'
```

Autocommand hooks:

```viml
" Will be run once when output buffer is created
autocmd User PageOpen { your settings }

" Will be run once when file buffer is created
autocmd User PageOpenFile { your settings }

" Only if -C option provided.
" Will be run always when output buffer is created
" and also when `page` connects to instance `-i, -I` buffers:
autocmd User PageConnect { your settings }
autocmd User PageDisconnect { your settings }
```

Hotkey for closing `page` buffers on `<C-c>`:

```viml
function! PageClose(page_alternate_bufnr)
    bd!
    if bufnr('%') == a:page_alternate_bufnr && mode('%') == 'n'
        norm a
    endif
endfunction
autocmd User PageOpen
    \| exe 'map  <buffer> <C-c> :call PageClose(b:page_alternate_bufnr)<CR>'
    \| exe 'tmap <buffer> <C-c> :call PageClose(b:page_alternate_bufnr)<CR>'
```

## Shell hacks

To use as `$PAGER` without scrollback overflow:

```zsh
export PAGER="page -q 90000"
```

To use as `$MANPAGER` without error:

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

```viml
let b:page_alternate_bufnr={initial_buf_nr}
let b:page_scrolloff_backup=&scrolloff
setl scrollback=100000 scrolloff=999 signcolumn=no nonumber nomodifiable {filetype}
exe 'au BufEnter <buffer> set scrolloff=999'
exe 'au BufLeave <buffer> let &scrolloff=b:page_scrolloff_backup'
{cmd_pre}
exe 'silent doautocmd User PageOpen'
redraw
{cmd_provided_by_user}
{cmd_post}
```

Where:

```viml
{initial_buf_nr}
 number of parent :term buffer or -1 when page isn't spawned from neovim terminal

  Is always set on all buffers created by page
```

```viml
{filetype}
 filetype=value of -t argument or "pager"

  Is set only on output buffers.
  On files buffers filetypes are detected automatically.
```

```viml
{cmd_pre}
 exe 'com! -nargs=? Page call rpcnotify(0, ''page_fetch_lines'', ''{page_id}'', <args>)'
 exe 'au BufEnter <buffer> com! -nargs=? Page call rpcnotify(0, ''page_fetch_lines'', ''{page_id}'<args>)'
 exe 'au BufDelete <buffer> call rpcnotify(0, ''page_buffer_closed'', ''{page_id}'')'

  Is appended when -q is provided


{cmd_pre}
 let b:page_lcd_backup = getcwd()
 lcd {pwd}
 exe 'au BufEnter <buffer> lcd {pwd}'
 exe 'au BufLeave <buffer> lcd ' .. b:page_lcd_backup

  Is also appended when -P is provided.
  {pwd} is $PWD value
```

```viml
{cmd_provided_by_user}
 value of -e argument

  Is appended when -e is provided
```

```viml
{cmd_post}
 this executes PageOpenFile autocommand

  Is appended only on file buffers
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
