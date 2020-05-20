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

```text

USAGE:
    page [OPTIONS] [FILES]...

OPTIONS:
    -o                           Create and use output buffer (to redirect text from page stdin into it) [implied always]
    -O <open-from>               Echo input if it has less than <open_from> lines [default: 0 (disabled); empty: term height; ignored if page isn't piped]
    -p                           Print path to pty device associated with output buffer (to redirect `command > /path/to/pty`) [implied if page isn't piped]
    -P                           Set $PWD as working dir for output buffer (allows to navigate paths with `gf`)
    -q <query-lines>             Enable on-demand stdin reading with :Page <query-lines> command on neovim side [default: 0 (disabled)]
    -f                           Cursor follows output buffer content when it appears instead of keeping top position (like `tail -f`)
    -F                           Cursor follows output buffer content when it appears instead of keeping top position also for each of <FILES>
    -t <filetype>                Set neovim filetype for output buffer (enables syntax highlighting) [default: pager]
    -b                           Return back to current buffer
    -B                           Return back to current buffer and enter into INSERT mode
    -n <name>                    Set title for output buffer (to display it in statusline) [env: PAGE_BUFFER_NAME=cargo run]
    -w                           Allow to ender into INSERT/TERMINAL mode by pressing i, I, a, A keys [ignored on connected instance output buffer]
                                  ~ ~ ~
    -a <address>                 Neovim session address [env: NVIM_LISTEN_ADDRESS=/tmp/nvim93nVQD/0]
    -A <arguments>               Neovim arguments to use when spawning its child process [env: NVIM_PAGE_ARGS=]
    -c <config>                  Neovim config path to use when spawning its child process [file:$XDG_CONFIG_HOME/page/init.vim]
    -C                           Enable PageConnect PageDisconnect autocommands
    -e <command>                 Run command in output buffer after it was created
    -E <command-post>            Run command on output buffer after it was created or connected as instance
                                  ~ ~ ~
    -i <instance>                Use named output buffer or create if it doesn't exists. Redirected from page stdin new content will replace existed
    -I <instance-append>         Use named output or create if it doesn't exists. Redirected from page stdin new content will be appendded to existed
    -x <instance-close>          Close this named output buffer if exists [revokes implied options]
                                  ~ ~ ~
    -W                           Flush redirection protection that prevents from producing junk and possible overwrite of existed files by invoking commands
                                 like "ls > $(NVIM_LISTEN_ADDRESS= page -E q)" where the RHS of > operator evaluates not into /path/to/pty as expected but
                                 into a bunch of whitespace-separated strings/escape sequences from neovim UI, and bad things happens because some shells
                                 interpret this as valid targets for text redirection. The protection is only printing of a path to the existed dummy
                                 directory always first before printing of a neovim UI might occur; this makes the first target for text redirection from
                                 page's output invalid and disrupts redirection early before any harmful write might begin.
                                 [env:PAGE_REDIRECTION_PROTECT (0 to disable)]
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
    <FILES>...    Open provided files in separate buffers [revokes implied options]

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
