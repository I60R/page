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

![](https://imgur.com/lxDCPpn.gif)

* *under neovim's terminal*

![](https://i.imgur.com/rcLEM6X.gif)

---

## CLI

<details><summary> expand `page --help`</summary>

```text

USAGE:
    page [FLAGS] [OPTIONS] [files]...

FLAGS:
    -o               Create and use new output buffer (to display text from page stdin) [implied]
    -p               Print path to buffer pty (to redirect `command > /path/to/output`) [implied when page not piped]
    -P               Set $PWD as working dir for output buffer (to navigate paths with `gf`)
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
    -a <address>                 Neovim session address [env: NVIM_LISTEN_ADDRESS=/tmp/nvimycgkAf/0]
    -A <arguments>               Neovim arguments for new child process [env: NVIM_PAGE_ARGS=]
    -c <config>                  Neovim config path for new child process [file:$XDG_CONFIG_HOME/page/init.vim]
    -e <command>                 Run command in output buffer after it's created
    -E <command-post>            Run command in output buffer after it's created or connected as instance
    -i <instance>                Connect or create named output buffer. When connected, new content overwrites previous
    -I <instance-append>         Connect or create named output buffer. When connected, new content appends to previous
    -x <instance-close>          Close instance buffer with this name if exist [revokes implied options]
    -n <name>                    Set output buffer name (displayed in statusline) [env: PAGE_BUFFER_NAME=page -h]
    -t <filetype>                Set output buffer filetype (for syntax highlighting) [default: pager]
    -q <query-lines>             Enable on-demand stdin reading with :Page <query_lines> command [default: 0]
    -R <split-right-cols>        Split right and resize to <split_right_cols> columns
    -L <split-left-cols>         Split left  and resize to <split_left_cols>  columns
    -U <split-above-rows>        Split above and resize to <split_above_rows> rows
    -D <split-below-rows>        Split below and resize to <split_below_rows> rows

ARGS:
    <files>...    Open provided files in separate buffers [revokes implied options]
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
setl scrollback=-1 scrolloff=999 signcolumn=no nonumber nomodifiable {filetype}
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
