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



<details><summary/>Click to expand `page --help`<summary/>
```
page 1.7.0
160R <160R@protonmail.com>
A pager that utilizes neovim's terminal buffer

USAGE:
    page [FLAGS] [OPTIONS] [FILES]...

FLAGS:
    -o               Open a new buffer [implied] (to show text written into page stdin)
    -p               Print /dev/pty/* of -o buffer [implied when not piped] (to redirect `ls > /dev/pty*`)
    -b               Return back to current buffer
    -B               Return back to current buffer and enter INSERT mode
    -f               Follow output instead of keeping top position (like `tail -f`)
    -F               Follow output instead of keeping top position and scroll each of <FILES> to the bottom
    -W               Flush redirecting protection that prevents from producing junk and possible corruption of files by
                     invoking commands like "unset NVIM_LISTEN_ADDRESS && ls > $(page -E q)" where "$(page -E q)" or
                     similar capture evaluates not into /dev/pty/* as expected but into whole neovim UI which consists
                     of a bunch of characters and strings. Many useless files would be created for each word and even
                     overwriting of files might occur. To prevent that, a path to temporary directory is printed first,
                     which makes "ls > directory ..." to fail early because it's impossible to redirect text into
                     directory. [env:PAGE_REDIRECTION_PROTECT: (0 to disable)]
    -r               Split right with ratio: window_width  * 3 / (<r-provided> + 1)
    -l               Split left  with ratio: window_width  * 3 / (<l-provided> + 1)
    -u               Split above with ratio: window_height * 3 / (<u-provided> + 1)
    -d               Split below with ratio: window_height * 3 / (<d-provided> + 1)
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -a <address>                 Neovim session address [env:NVIM_LISTEN_ADDRESS: ]
    -A <arguments>               Neovim arguments when a new session is started [env:NVIM_PAGE_ARGS: ]
    -c <config>                  Neovim config (-u) for a new session [file:$XDG_CONFIG_HOME/page/init.vim]
    -e <command>                 Run command in a pager buffer when neovim config is sourced and reading begins
    -E <command_post>            Run command in a pager buffer after reading was done
    -i <instance>                Use existed named buffer to read stdin or spawn new. New content overwrites previous
    -I <instance_append>         Use existed named buffer to read stdin or spawn new. New content appends to previous
    -x <instance_close>          Close named buffer if it exists and exit [revokes implied options]
    -n <name>                    Set title for -o buffer [env:PAGE_BUFFER_NAME: page -h]
    -t <filetype>                Set filetype for -o buffer (for syntax highlighting) [default: pager]
    -R <split_right_cols>        Split right and resize to <split_right_cols> columns
    -L <split_left_cols>         Split left  and resize to <split_left_cols>  columns
    -U <split_above_rows>        Split above and resize to <split_above_rows> rows
    -D <split_below_rows>        Split below and resize to <split_below_rows> rows

ARGS:
    <FILES>...    Open provided files in separate buffers [revokes implied options]
```
</details>


## Customizations

Available `init.vim` settings:
```viml
let g:page_icon_instance = '$'
let g:page_icon_redirect = '>'
let g:page_icon_pipe = '|'
```

Default settings set for each page buffer:
```viml
let g:page_scrolloff_backup = &scrolloff " to restore this global option on other buffer
setl scrollback=-1 scrolloff=999 signcolumn=no nonumber nomodifiable
autocmd BufEnter <buffer> set scrolloff=999
autocmd BufLeave <buffer> let &scrolloff=g:page_scrolloff_backup
```

Autocommands that will be invoked:
```viml
silent doautocmd User PageOpen "once when buffer created
silent doautocmd User PageRead "before write from page stdin
```

## Shell hacks

To set as `$MANPAGER`:

```
export MANPAGER="page -E 'sleep 100m|%y p|enew!|bd! #|pu p|set ft=man'"
```

To override default neovim config use this file:
```
$XDG_CONFIG_HOME/page/init.vim
```

To display buffer name as first two words from invoked command (zsh only):

```
preexec() { 
    echo "${1// *|*}" | read -A words 
    export PAGE_BUFFER_NAME="${words[@]:0:2}" 
}
```


## How it works

* `page` connects to parent (or spawned) `nvim` process through `$NVIM_LISTEN_ADDRESS`
* Command `:term pty-agent {pipe}` is invoked through nvim's MessagePack-RPC
* `pty-agent` reveals (through *{pipe}*) path to PTY device associated with current terminal buffer and blocks it's own thread to keep that buffer open
* `page` redirects all data from STDIN into PTY device (opened from path read from {pipe})
* When `page` is'nt piped, PTY device path will be printed, user then can redirect into it manually


## Limitations

* Only ~100000 lines can be displayed (this is neovim terminal limit)
* `MANPAGER=page -t man` not works because `set ft=man` fails on :term buffer (might affect other filetypes)


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
