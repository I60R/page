# Page

[![Build Status](https://travis-ci.org/I60R/page.svg?branch=master)](https://travis-ci.org/I60R/page)
[![Lines Of Code](https://tokei.rs/b1/github/I60R/page)](https://github.com/I60R/page)

Allows you to redirect text into neovim.  
You can set it as `$PAGER` to view logs, diffs, various command outputs.  
  
ANSI escape sequences are interpreted directly by :term buffer (this makes it faster than [vimpager](https://github.com/rkitover/vimpager) and [nvimpager](https://github.com/lucc/nvimpager)).  
No need to wait until EOF - text displayed instantly as it arrives.  
  
Uses parent neovim process when available (great fit alongside with [neovim-remote](https://github.com/mhinz/neovim-remote)).  
  
You will have familiar keybindings and all text editing, searching and navigating facilities that neovim provides (this makes it better than [less](https://en.wikipedia.org/wiki/Less_(Unix))).  



## Usage

For full list of cli options refer to [src/cli.rs](https://github.com/I60R/page/blob/master/src/cli.rs)
  

![](https://i.imgur.com/fVZqvsk.gif)

![](https://i.imgur.com/sMF9sDP.gif)

![](https://i.imgur.com/r38no3B.gif)



## Settings

Appearance:
```viml
    let g:page_icon_instance = '§'
    let g:page_icon_redirect = '>§'
    let g:page_icon_pipe = '|§'
```

Buffer defaults:
```viml
    setl scrollback=-1 scrolloff=999 signcolumn=no nonumber modifiable winfixwidth
    setl filetype=pager
```


## How it works

* `page` connects to parent (or spawned) `nvim` process through `$NVIM_LISTEN_ADDRESS`
* Command `:term pty-agent {pipe}` is invoked through nvim's MessagePack-RPC
* `pty-agent` reveals (through *{pipe}*) path to PTY device associated with current terminal buffer and blocks it's own thread to keep that buffer open
* `page` redirects all data from STDIN into PTY device (opened from path read from {pipe})
* In case when nothing to write, PTY device path is printed, so user can redirect into it manually


## Limitations

* Only 100000 lines can be displayed (nvim terminal limit)
* Not well tested *(set as `$PAGER` at your own risk)*


## Installation

* Arch Linux:
  * Make package: `git clone git@github.com:I60R/page.git && cd page && makepkg -ef`
  * Install: `sudo pacman -U page-git*.pkg.tar.xz`

* Manually:
  * Install `rustup` from your distribution package manager
  * Configure toolchain: `rustup install nightly && rustup default nightly`
  * `git clone git@github.com:I60R/page.git && cd page`
  * `cargo install --root / --force` (if that requires permission you must configure toolchain as root (or system wide) and re-run with `sudo`)
