# page(r) - read in neovim buffer

## How to use

1) Under neovim terminal

```bash
$ exa | page            # opens new buffer with exa output
```

```bash
$ exa > $(page)         # the same, but with ANSI colors
```

```bash
$ page                  # opens new buffer and prints it's pty device path
/dev/pty/$ID
$ exa > /dev/pty/$ID    # redirect ls output to that buffer
$ exa > /dev/pty/$ID    # will append to same buffer
```

 `:bd!` to close pager buffer


2) Under another terminal

* **DON'T  USE THIS WAY (!!!)**

    ```bash
    $ exa > $(page)     # this will create many useless files in current directory
    ```
    
* instead use this

    ```bash
    $ exa | page        # opens neovim instance with ls output
    ```

## How it works

* Each nvim terminal is mapped to pty device (file under /dev/pts/*)
* Each command launched from nvim terminal has it's STDOUT mapped to that device
* `page` opens new nvim terminal with `pty-agent` as shell
* `pty-agent` exposes path to pty device through named pipe and blocks thread
* `page` writes all data from it's STDIN to that pty device
* When `page` is not connected to pipe it prints pty device path, so you can redirect to it


## Limitations

* Only 100000 lines can be displayed (nvim terminal limit)
* Not well tested *(set as `$PAGER` at your own risk)*

## Installation

1. Install `rustup` from package manager 
2. Configure toolchain: `rustup install stable && rustup default stable`
3. Clone repo, cd into it
4. `cargo install --root / --force` (if that requires permission you must configure toolchain as root (or system wide) and re-run with `sudo`)
