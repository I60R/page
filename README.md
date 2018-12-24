# Page

[![Build Status](https://travis-ci.org/I60R/page.svg?branch=master)](https://travis-ci.org/I60R/page)
[![Lines Of Code](https://tokei.rs/b1/github/I60R/page)](https://github.com/I60R/page)

Allows you to redirect text directly into [neovim](https://github.com/neovim/neovim).  
You can set it as `$PAGER` to view logs, diffs, various command outputs.  
  
ANSI escape sequences will be interpreted by :term buffer, so it's noticeably faster than [vimpager](https://github.com/rkitover/vimpager) and [nvimpager](https://github.com/lucc/nvimpager).  
Text will be displayed instantly as it arrives - no need to wait until EOF.  

Also it allows to pipe/redirect from :term buffer into regular buffer, without spawning nested neovim instance for that purpose.  
This is by utilizing `$NVIM_LISTEN_ADDRESS` (like [neovim-remote](https://github.com/mhinz/neovim-remote)) does).  
  
You will have all familiar neovim keybindings and all text editing+searching+navigating facilities with your settings and plugins.   
Plus some useful options:

```
USAGE:
    page [FLAGS] [OPTIONS] [FILES]...

FLAGS:
    -o               Open new buffer [set by default, unless only <instance_close> or <FILES> provided]
    -p               Print path to /dev/pty/* for redirecting [set by default when don't reads from pipe]
    -b               Stay focused on current buffer
    -B               Stay focused on current buffer and keep INSERT mode
    -f               Follow output instead of keeping position
    -W               Flush redirecting protection, that prevents from producing junk and possible corruption of files
                     when no <address> available and "cmd > $(page)" is invoked, because $(page) here will hold nvim UI.
                     [env: PAGE_REDIRECTION_PROTECT:1]
    -r               Split right with ratio: window_width  * 3 / (<r provided> + 1)
    -l               Split left  with ratio: window_width  * 3 / (<l provided> + 1)
    -u               Split above with ratio: window_height * 3 / (<u provided> + 1)
    -d               Split below with ratio: window_height * 3 / (<d provided> + 1)
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -a <address>                 Neovim session address [env:NVIM_LISTEN_ADDRESS: /tmp/nvimfFd1Jz/0]
    -e <command>                 Run command in pager buffer when reading begins
    -E <command_post>            Run command in pager buffer after reading was done
    -i <instance>                Use named instance buffer if exist, or spawn new. New content will overwrite
    -I <instance_append>         Use named instance buffer if exist, or spawn new. New content will be appended
    -x <instance_close>          Only closes named instance buffer if exists
    -t <filetype>                Hint for syntax highlighting when reads from stdin [default: pager]
    -R <split_right_cols>        Split right and resize to <split_right_cols> columns
    -L <split_left_cols>         Split left  and resize to <split_left_cols>  columns
    -U <split_above_rows>        Split above and resize to <split_above_rows> rows
    -D <split_below_rows>        Split below and resize to <split_below_rows> rows

ARGS:
    <FILES>...    Additionally open these files in separate buffers
```
Example of usage under regular terminal:

![](https://i.imgur.com/4ulIBJi.gif)

Under nvim terminal:

![](https://i.imgur.com/mtzdoor.gif)



## Settings

Appearance:
```viml
    let g:page_icon_instance = '#'
    let g:page_icon_redirect = '>§'
    let g:page_icon_pipe = '|§'
```

This will be set by default on each buffer:
```viml
    setl scrollback=-1 scrolloff=999 signcolumn=no nonumber modified nomodifiable
```


## How it works

* `page` connects to parent (or spawned) `nvim` process through `$NVIM_LISTEN_ADDRESS`
* Command `:term pty-agent {pipe}` is invoked through nvim's MessagePack-RPC
* `pty-agent` reveals (through *{pipe}*) path to PTY device associated with current terminal buffer and blocks it's own thread to keep that buffer open
* `page` redirects all data from STDIN into PTY device (opened from path read from {pipe})
* When `page` is'nt piped, PTY device path will be printed, user then can redirect into it manually


## Limitations

* Only ~100000 lines can be displayed (this is nvim terminal limit)
* Not well tested yet *(set as `$PAGER` at your own risk)*


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
