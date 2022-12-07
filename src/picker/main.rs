pub(crate) mod cli;
pub(crate) mod context;

pub type NeovimConnection = connection::NeovimConnection<connection::Neovim<connection::IoWrite>>;
pub type NeovimBuffer = connection::Buffer<connection::IoWrite>;


#[tokio::main(worker_threads=2)]
async fn main() {
    connection::init_logger();

    let env_ctx = context::env_context::enter();

    main::warn_if_incompatible_options(&env_ctx.opt);

    if env_ctx.opt.view_only {
        let mut page_args = std::env::args();
        page_args.next(); // skip `nv`
        let page_args = page_args
            .filter(|arg| arg != "-v");

        let exit_code = std::process::Command::new("page")
            .args(page_args)
            .spawn()
            .expect("Cannot spawn `page`")
            .wait()
            .expect("`page` died unexpectedly")
            .code()
            .unwrap_or(0);

        std::process::exit(exit_code)
    }

    connect_neovim(env_ctx).await;
}

mod main {
    // Some options takes effect only when page would be
    // spawned from neovim's terminal
    pub fn warn_if_incompatible_options(opt: &crate::cli::Options) {
        if opt.address.is_some() {
            return
        }

        if opt.is_split_implied() {
            log::warn!(
                target: "usage",
                "Split (-r -l -u -d -R -L -U -D) is ignored \
                if address (-a or $NVIM) isn't set"
            );
        }
        if opt.back || opt.back_restore {
            log::warn!(
                target: "usage",
                "Switch back (-b -B) is ignored \
                if address (-a or $NVIM) isn't set"
            );
        }
    }
}


async fn connect_neovim(env_ctx: context::EnvContext) {
    log::info!(target: "context", "{env_ctx:#?}");

    connection::init_panic_hook();

    let nvim_conn = connection::open(
        &env_ctx.tmp_dir,
        &env_ctx.page_id,
        &env_ctx.opt.address,
        &env_ctx.opt.config,
        &env_ctx.opt.config,
        false
    ).await;

    split_current_buffer(env_ctx, nvim_conn).await;
}


async fn split_current_buffer(env_ctx: context::EnvContext, conn: NeovimConnection) {
    use context::env_context::SplitUsage;
    if let SplitUsage::Enabled = env_ctx.split_usage {
        let cmd = split_current_buffer::create_split_command(&env_ctx.opt.split);
        conn.nvim_actions
            .exec_lua(&cmd, vec![])
            .await
            .expect("Cannot create split window");
    }

    read_stdin(env_ctx, conn).await;
}

mod split_current_buffer {
    /// This is almost copy-paste from pager/neovim.rs
    pub fn create_split_command(
        opt: &crate::cli::SplitOptions
    ) -> String {
        if opt.popup {

            let w_ratio = |s| format!("math.floor(((w / 2) * 3) / {})", s + 1);
            let h_ratio = |s| format!("math.floor(((h / 2) * 3) / {})", s + 1);

            let (w, h, o) = ("w".to_string(), "h".to_string(), "0".to_string());

            let (width, height, row, col);

            if opt.split_right != 0 {
                (width = w_ratio(opt.split_right), height = h, row = &o, col = &w)

            } else if opt.split_left != 0 {
                (width = w_ratio(opt.split_left),  height = h, row = &o, col = &o)

            } else if opt.split_below != 0 {
                (width = w, height = h_ratio(opt.split_below), row = &h, col = &o)

            } else if opt.split_above != 0 {
                (width = w, height = h_ratio(opt.split_above), row = &o, col = &o)

            } else if let Some(split_right_cols) = opt.split_right_cols.map(|x| x.to_string()) {
                (width = split_right_cols, height = h, row = &o, col = &w)

            } else if let Some(split_left_cols) = opt.split_left_cols.map(|x| x.to_string()) {
                (width = split_left_cols,  height = h, row = &o, col = &o)

            } else if let Some(split_below_rows) = opt.split_below_rows.map(|x| x.to_string()) {
                (width = w, height = split_below_rows, row = &h, col = &o)

            } else if let Some(split_above_rows) = opt.split_above_rows.map(|x| x.to_string()) {
                (width = w, height = split_above_rows, row = &o, col = &o)

            } else {
                unreachable!()
            };

            indoc::formatdoc! {"
                local w = vim.api.nvim_win_get_width(0)
                local h = vim.api.nvim_win_get_height(0)
                local buf = vim.api.nvim_create_buf(true, false)
                local win = vim.api.nvim_open_win(buf, true, {{
                    relative = 'editor',
                    width = {width},
                    height = {height},
                    row = {row},
                    col = {col}
                }})
                vim.api.nvim_set_current_win(win)
                local winblend = vim.g.page_popup_winblend or 25
                vim.api.nvim_win_set_option(win, 'winblend', winblend)
            "}
        } else {

            let w_ratio = |s| format!("' .. tostring(math.floor(((w / 2) * 3) / {})) .. '", s + 1);
            let h_ratio = |s| format!("' .. tostring(math.floor(((h / 2) * 3) / {})) .. '", s + 1);

            let (a, b) = ("aboveleft", "belowright");
            let (w, h) = ("winfixwidth", "winfixheight");
            let (v, z) = ("vsplit", "split");

            let (direction, size, split, fix);

            if opt.split_right != 0 {
                (direction = b, size = w_ratio(opt.split_right), split = v, fix = w)

            } else if opt.split_left != 0 {
                (direction = a,  size = w_ratio(opt.split_left), split = v, fix = w)

            } else if opt.split_below != 0 {
                (direction = b, size = h_ratio(opt.split_below), split = z, fix = h)

            } else if opt.split_above != 0 {
                (direction = a, size = h_ratio(opt.split_above), split = z, fix = h)

            } else if let Some(split_right_cols) = opt.split_right_cols.map(|x| x.to_string()) {
                (direction = b, size = split_right_cols, split = v, fix = w)

            } else if let Some(split_left_cols) = opt.split_left_cols.map(|x| x.to_string()) {
                (direction = a, size = split_left_cols,  split = v, fix = w)

            } else if let Some(split_below_rows) = opt.split_below_rows.map(|x| x.to_string()) {
                (direction = b, size = split_below_rows, split = z, fix = h)

            } else if let Some(split_above_rows) = opt.split_above_rows.map(|x| x.to_string()) {
                (direction = a, size = split_above_rows, split = z, fix = h)

            } else {
                unreachable!()
            };

            indoc::formatdoc! {"
                local prev_win = vim.api.nvim_get_current_win()
                local w = vim.api.nvim_win_get_width(prev_win)
                local h = vim.api.nvim_win_get_height(prev_win)
                vim.cmd('{direction} {size}{split}')
                local buf = vim.api.nvim_create_buf(true, false)
                vim.api.nvim_set_current_buf(buf)
                local win = vim.api.nvim_get_current_win()
                vim.api.nvim_win_set_option(win, '{fix}', true)
            "}
        }
    }
}


async fn read_stdin(env_ctx: context::EnvContext, conn: NeovimConnection) {
    use context::env_context::ReadStdinUsage;
    if let ReadStdinUsage::Enabled = &env_ctx.pipe_buf_usage {
        let buf = conn.nvim_actions
            .create_buf(true, true)
            .await
            .expect("Cannot create STDIN buffer");

        conn.nvim_actions
            .set_current_buf(&buf)
            .await
            .expect("Cannot set STDIN buffer");

        let mut ln = Vec::with_capacity(512);
        let mut i = 0;

        for b in std::io::Read::bytes(std::io::stdin()) {
            match b {
                Err(e) => {
                    panic!("Failed to prefetch line from stdin: {e}")
                }
                Ok(_eol @ b'\n') => {
                    ln.shrink_to_fit();

                    let ln_str = String::from_utf8(ln)
                        .expect("Cannot read UTF8 string");
                    ln = Vec::with_capacity(512);

                    buf.set_lines(i, i, false, vec![ln_str])
                        .await
                        .expect("Cannot set line");

                    i += 1;
                }
                Ok(b) => {
                    ln.push(b);
                }
            }
        }
    }

    open_files(env_ctx, conn).await
}


async fn open_files(env_ctx: context::EnvContext, mut conn: NeovimConnection) {
    use context::env_context::FilesUsage;
    match env_ctx.files_usage {
        FilesUsage::RecursiveCurrentDir {
            recurse_depth
        } => {
            let read_dir = walkdir::WalkDir::new("./")
                .contents_first(true)
                .follow_links(false)
                .max_depth(recurse_depth);

            for f in read_dir {
                let f = f.expect("Cannot recursively read dir entry");

                if let Some(f) = open_files::FileToOpen::new_existed_file(f.path()) {
                    if !f.is_text && !env_ctx.opt.open_non_text {
                        continue
                    }

                    open_files::open_file(&mut conn, &env_ctx, &f.path_string).await;
                }
            }
        },
        FilesUsage::LastModifiedFile => {
            let mut last_modified = None;

            let read_dir = std::fs::read_dir("./")
                .expect("Cannot read current directory");

            for f in read_dir {
                let f = f.expect("Cannot read dir entry");

                if let Some(f) = open_files::FileToOpen::new_existed_file(f.path()) {
                    if !f.is_text && !env_ctx.opt.open_non_text {
                        continue;
                    }

                    let Ok(f_modified_time) = f.get_modified_time() else {
                        log::error!(
                            target: "last_modified",
                            "Cannot read metadata: {}",
                             f.path_string
                        );

                        continue;
                    };

                    if let Some((l_modified_time, l_modified)) = last_modified.as_mut() {
                        if *l_modified_time < f_modified_time {
                            (*l_modified_time, *l_modified) = (f_modified_time, f);
                        }
                    } else {
                        last_modified.replace((f_modified_time, f));
                    }
                }
            }

            if let Some((_, f)) = last_modified {
                open_files::open_file(&mut conn, &env_ctx, &f.path_string).await;
            }
        },
        FilesUsage::FilesProvided => {
            for f in &env_ctx.opt.files {
                if let Some(f) = open_files::FileToOpen::new_maybe_uri(f) {
                    if !f.is_text && !env_ctx.opt.open_non_text {
                        continue
                    }

                    open_files::open_file(&mut conn, &env_ctx, &f.path_string).await;
                }
            }
        }
    }

    exit_from_neovim(env_ctx, conn).await;
}

mod open_files {
    use std::{path::{PathBuf, Path}, time::SystemTime};
    use crate::{
        cli::FileOption,
        context::EnvContext,
    };

    use once_cell::unsync::Lazy;
    const PWD: Lazy<PathBuf> = Lazy::new(|| {
        PathBuf::from(
            std::env::var("PWD")
                .expect("Cannot read $PWD value")
        )
    });

    pub struct FileToOpen {
        pub path: PathBuf,
        pub path_string: String,
        pub is_text: bool,
    }

    impl FileToOpen {
        pub fn new_existed_file<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Option<FileToOpen> {
            let path = match std::fs::canonicalize(&path) {
                Ok(canonical) => canonical,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    PWD.join(path.as_ref())
                }
                Err(e) => {
                    log::error!(
                        target: "open file",
                        "cannot open {path:?}: {e}"
                    );
                    return None;
                }
            };
            let path_string = path
                .to_string_lossy()
                .to_string();
            let is_text = is_text_file(&path_string);
            let f = FileToOpen {
                path,
                path_string,
                is_text
            };
            Some(f)
        }

        pub fn new_maybe_uri(path: &FileOption) -> Option<FileToOpen> {
            let f = match path {
                FileOption::Uri(u) => Self {
                    path: PathBuf::new(),
                    path_string: u.clone(),
                    is_text: true,
                },
                FileOption::Path(p) => Self::new_existed_file(p)?,
            };
            Some(f)
        }

        pub fn get_modified_time(&self) -> std::io::Result<SystemTime> {
            let f_meta = self.path
                .metadata()?;
            let modified = f_meta
                .modified()?;
            Ok(modified)
        }
    }

    pub fn is_text_file<F: AsRef<std::ffi::OsStr>>(f: F) -> bool {
        let file_cmd = std::process::Command::new("file")
            .arg(f.as_ref())
            .output()
            .expect("Cannot get `file` output");
        let file_cmd_output = String::from_utf8(file_cmd.stdout)
            .expect("Non UTF8 `file` output");

        if file_cmd_output.contains("ASCII text") ||
            file_cmd_output.contains("UTF-8") ||
            file_cmd_output.contains("UTF-16") ||
            file_cmd_output.contains(": empty") ||
            file_cmd_output.contains(": cannot open")
        {
            return true
        }

        if file_cmd_output.contains("symbolic link") {
            let pointee = std::fs::read_link(f.as_ref())
                .expect("Cannot read link");

            return is_text_file(pointee)
        }

        false
    }


    pub async fn open_file(
        conn: &mut super::NeovimConnection,
        env_ctx: &EnvContext,
        f: &str
    ) {
        let cmd = format!("e {}", f);
        conn.nvim_actions
            .command(&cmd)
            .await
            .expect("Cannot open file buffer");

        if let Some(ft) = &env_ctx.opt.filetype {
            let cmd = format!("set filetype={ft}");
            conn.nvim_actions
                .command(&cmd)
                .await
                .expect("Cannot set filetype")
        }

        if env_ctx.opt.follow {
            conn.nvim_actions
                .command("norm! G")
                .await
                .expect("Cannot execute follow command")

        } else if let Some(pattern) = &env_ctx.opt.pattern {
            let cmd = format!("norm! /{pattern}");
            conn.nvim_actions
                .command(&cmd)
                .await
                .expect("Cannot execute follow command")

        } else if let Some(pattern_backwards) = &env_ctx.opt.pattern_backwards {
            let cmd = format!("norm! ?{pattern_backwards}");
            conn.nvim_actions
                .command(&cmd)
                .await
                .expect("Cannot execute follow backwards command")
        }

        if env_ctx.opt.keep || env_ctx.opt.keep_until_write {
            let (channel, page_id) = (conn.channel, &env_ctx.page_id);

            let mut keep_until_write_cmd = "";
            if env_ctx.opt.keep_until_write {
                keep_until_write_cmd = indoc::indoc! {r#"
                    vim.api.nvim_create_autocmd('BufWritePost', {
                        buffer = buf,
                        callback = function()
                            pcall(function()
                                on_delete()
                                vim.api.nvim_buf_delete(buf, { force = true })
                            end)
                        end
                    })
                "#};
            }

            let cmd = indoc::formatdoc! {r#"
                local buf = vim.api.nvim_get_current_buf()
                local function on_delete()
                    pcall(function()
                        vim.rpcnotify({channel}, 'page_buffer_closed', '{page_id}')
                    end)
                end
                {keep_until_write_cmd}
                vim.api.nvim_create_autocmd({{ 'BufDelete', 'BufWinLeave' }}, {{
                    buffer = buf,
                    callback = on_delete
                }})
            "#};
            conn.nvim_actions
                .exec_lua(&cmd, vec![])
                .await
                .expect("Cannot execute keep command");
        }

        if let Some(lua) = &env_ctx.opt.lua {
            conn.nvim_actions
                .exec_lua(lua, vec![])
                .await
                .expect("Cannot execute lua command");
        }

        if let Some(command) = &env_ctx.opt.command {
            conn.nvim_actions
                .command(command)
                .await
                .expect("Cannot execute command")
        }

        if env_ctx.opt.keep || env_ctx.opt.keep_until_write {
            match conn.rx.recv().await {
                Some(connection::NotificationFromNeovim::BufferClosed) | None => {
                    return
                },
                n @ _ => {
                    log::error!("Unhandled notification: {n:?}")
                }
            }
        }
    }
}


async fn exit_from_neovim(env_ctx: context::EnvContext, mut conn: NeovimConnection) {
    if !env_ctx.opt.back && !env_ctx.opt.back_restore {
        connection::close_and_exit(&mut conn).await;
    }

    let (win, buf) = &conn.initial_win_and_buf;
    conn.nvim_actions
        .set_current_win(win)
        .await
        .expect("Cannot return to initial window");
    conn.nvim_actions
        .set_current_buf(buf)
        .await
        .expect("Cannot return to initial buffer");
    if env_ctx.opt.back_restore {
        conn.nvim_actions
            .command("norm! A")
            .await
            .expect("Cannot return to insert mode");
    }

    connection::close_and_exit(&mut conn).await;
}
