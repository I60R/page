# page(r) - read in neovim buffer


## How to use

1) Make sure your terminal is neovim terminal
2) Make sure `$NVIM_LISTEN_ADDRESS` is actual
```bash
$ /bin/ls | page       # will open new buffer with command output
$ /bin/ls > $(page)    # will do the same also with colors
```

## How it works

1) Each nvim terminal is mapped to pty device (file under /dev/pts/*)
2) Each command launched from nvim terminal has it's STDOUT mapped to that device
3) `page` opens new nvim terminal with `pty-agent` as shell
4) `pty-agent` exposes path to pty device through named pipe and blocks thread
5) `page` writes all data from it's STDIN to that pty device
6) When `page` is not connected to pipe it prints pty device path, so you can redirect to it


## Limitations

1) Only 100000 lines can be displayed (nvim terminal limit)
2) Will not work under another terminal session
