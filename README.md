# Page

[![Build Status](https://travis-ci.org/I60R/page.svg?branch=master)](https://travis-ci.org/I60R/page)
[![Lines Of Code](https://tokei.rs/b1/github/I60R/page)](https://github.com/I60R/page)

Allows you to redirect text into [neovim](https://github.com/neovim/neovim).  
You can set it as `$PAGER` to view logs, diffs, various command outputs.  
  
ANSI escape sequences will be interpreted by :term buffer which is noticeably faster than [vimpager](https://github.com/rkitover/vimpager) and [nvimpager](https://github.com/lucc/nvimpager).  
Text will be displayed instantly as it arrives - no need to wait until EOF.  

Also, it allows you to redirect text from shell running in :term buffer into a new buffer in parent neovim instance instead of spawning a nested instance for that purpose.  
This is by utilizing `$NVIM_LISTEN_ADDRESS` as [neovim-remote](https://github.com/mhinz/neovim-remote) does.  
  
All neovims text editing+searching+navigating facilities, all settings, mappings, plugins, etc. from your neovim config will be effectively reused.

## Usage

* *under regular terminal*

![](https://imgur.com/lxDCPpn.gif)

* *under neovim's terminal*

![](https://i.imgur.com/rcLEM6X.gif)

---

## CLI options

<details><summary> (click here to expand `page --help`)</summary>

```
USAGE:
    page [FLAGS] [OPTIONS] [FILES]...

FLAGS:
    -o               Create and use new output buffer (to display text from page stdin) [implied]
    -p               Print path to buffer pty (to redirect `command > /path/to/output`) [implied when page not piped]
    -b               Return back to current buffer
    -B               Return back to current buffer and enter INSERT mode
    -f               Follow output instead of keeping top position (like `tail -f`)
    -F               Follow output instead of keeping top position also for each of <FILES>
    -W               Flush redirecting protection that prevents from producing junk and possible corruption of files by
                     invoking commands like "unset NVIM_LISTEN_ADDRESS && ls > $(page -E q)" where "$(page -E q)" part
                     not evaluates into /path/to/sink as expected but instead into neovim UI, which consists of a bunch
                     of escape characters and strings. Many useless files could be created then and even overwriting of
                     existed file might occur. To prevent that, a path to temporary directory is printed first, which
                     causes "command > directory ..." to fail early as it's impossible to redirect text into directory.
                     [env:PAGE_REDIRECTION_PROTECT: (0 to disable)]
    -C               Enable PageConnect PageDisconnect autocommands
    -r               Split right with ratio: window_width  * 3 / (<r-provided> + 1)
    -l               Split left  with ratio: window_width  * 3 / (<l-provided> + 1)
    -u               Split above with ratio: window_height * 3 / (<u-provided> + 1)
    -d               Split below with ratio: window_height * 3 / (<d-provided> + 1)
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -a <address>                 Neovim session address [env: NVIM_LISTEN_ADDRESS=/tmp/nvimUbj1Sg/0]
    -A <arguments>               Neovim arguments for new child process [env: NVIM_PAGE_ARGS=]
    -c <config>                  Neovim config path for new child process [file:$XDG_CONFIG_HOME/page/init.vim]
    -e <command>                 Run command in output buffer after it's created
    -E <command_post>            Run command in output buffer after it's created or connected as instance
    -i <instance>                Connect or create named output buffer. When connected, new content overwrites previous
    -I <instance_append>         Connect or create named output buffer. When connected, new content appends to previous
    -x <instance_close>          Close instance buffer with this name if exist [revokes implied options]
    -n <name>                    Set output buffer name (displayed in statusline) [env: PAGE_BUFFER_NAME=page --help]
    -t <filetype>                Set output buffer filetype (for syntax highlighting) [default: pager]
    -R <split_right_cols>        Split right and resize to <split_right_cols> columns
    -L <split_left_cols>         Split left  and resize to <split_left_cols>  columns
    -U <split_above_rows>        Split above and resize to <split_above_rows> rows
    -D <split_below_rows>        Split below and resize to <split_below_rows> rows

ARGS:
    <FILES>...    Open provided files in separate buffers [revokes implied options]
```
</details>

## Viml

Change statusline appearance:

```viml
let g:page_icon_instance = '$'
let g:page_icon_redirect = '>'
let g:page_icon_pipe = '|'
```

Defaults for output buffer:

```viml
let g:page_scrolloff_backup = &scrolloff
" -f filetype not applies for buffers created for <FILES>
setl scrollback=-1 scrolloff=999 signcolumn=no nonumber nomodifiable filetype=${-f value}
exe 'autocmd BufEnter <buffer> set scrolloff=999'
exe 'autocmd BufLeave <buffer> let &scrolloff=g:page_scrolloff_backup'
exe 'silent doautocmd User PageOpen'
" -e command not runs on buffers created for <FILES>
exe '${-e value}'
```

Autocommands invoked:

```viml
 "first time when buffer created
silent doautocmd User PageOpen
" when -C command enabled (this also works on connected instance buffer)
silent doautocmd User PageConnect
silent doautocmd User PageDisconnect
```

## Shell hacks

To set as `$MANPAGER`:

```zsh
export MANPAGER="page -C -e 'au User PageDisconnect sleep 100m|%y p|enew! |bd! #|pu p|set ft=man'"
```

To override default neovim config create this file (or use -c option):

```zsh
$XDG_CONFIG_HOME/page/init.vim
```

To set output buffer name as first two words from invoked command (zsh only):

```zsh
preexec() {
    echo "${1// *|*}" | read -A words
    export PAGE_BUFFER_NAME="${words[@]:0:2}"
}
```


## How it works

* `page` connects to parent (or spawned) `nvim` process through `$NVIM_LISTEN_ADDRESS`
* Command `:term page-term-agent {pipe}` is invoked through nvim's MessagePack-RPC
* `page-term-agent` reveals (through *{pipe}*) path to PTY device associated with current terminal buffer and blocks it's own thread to keep that buffer open
* `page` redirects all data from STDIN into PTY device (opened from path read from {pipe})
* When `page` is'nt piped, PTY device path will be printed, user then can redirect into it manually


## Limitations

* Only ~100000 lines can be displayed (this is neovim terminal limit)
* `MANPAGER=page -t man` not works because `set ft=man` fails on :term buffer (other filetypes may be affected as well)


## Installation

* From binaries
  * Grab binary for your platform from [releases](https://github.com/I60R/page/releases)

* Arch Linux:
  * Make package: `git clone git@github.com:I60R/page.git && cd page && makepkg -ef`
  * Install: `sudo pacman -U page-git*.pkg.tar.xz`

* Manually:
  * Install `rustup` from your distribution package manager
  * Configure toolchain: `rustup install stable && rustup default stable`
  * `git clone git@github.com:I60R/page.git && cd page`
  * `cargo install --root / --force` (if that requires permission you must configure toolchain as root (or system wide) and re-run with `sudo`)
