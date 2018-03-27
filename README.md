# page(r) - read in neovim buffer


## Usage

![](https://i.imgur.com/hnndUHy.gif)

![](https://i.imgur.com/cktYQjY.gif)

![](https://i.imgur.com/iYDlYpj.gif)


## Settings

Defaults:

```viml
    let g:page_icon_instance = '§' "  devicon is used on gif
    let g:page_icon_redirect = '>§'
    let g:page_icon_pipe = '|§'
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
