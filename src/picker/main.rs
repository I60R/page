pub(crate) mod cli;
pub(crate) mod context;

pub type NeovimConnection = connection::NeovimConnection<connection::Neovim<connection::IoWrite>>;
pub type NeovimBuffer = connection::Buffer<connection::IoWrite>;


#[tokio::main(worker_threads=2)]
async fn main() {

    connection::init_logger();

    let env_ctx = context::gather_env::enter();

    main::warn_if_incompatible_options(&env_ctx.opt);

    connect_neovim(env_ctx).await;
}

mod main {
    // Some options takes effect only when page would be
    // spawned from neovim's terminal
    pub fn warn_if_incompatible_options(opt: &crate::cli::Options) {
        if opt.address.is_some() {
            return
        }

        if opt.is_file_open_split_implied() {
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

    gather_files(env_ctx, nvim_conn).await
}


async fn gather_files(env_ctx: context::EnvContext, conn: NeovimConnection) {

    use context::gather_env::FilesUsage;

    match env_ctx.files_usage {
        FilesUsage::RecursiveCurrentDir { recurse_depth } => {
            let read_dir = walkdir::WalkDir::new("./")
                .contents_first(true)
                .follow_links(false)
                .max_depth(recurse_depth);

            for f in read_dir {
                let f = f.expect("Cannot recursively read dir entry");
                let f = gather_files::FileToOpen::new(f.path());

                if !f.is_text && !env_ctx.opt.open_non_text {
                    continue
                }

                gather_files::open_file(&conn, &env_ctx, &f.path_string).await;
            }
        },
        FilesUsage::LastModifiedFile => {
            let mut last_modified = None;

            let read_dir = std::fs::read_dir("./").expect("Cannot read current directory");
            for f in read_dir {
                let f = f.expect("Cannot read dir entry");
                let f = gather_files::FileToOpen::new(f.path());

                if !f.is_text && !env_ctx.opt.open_non_text {
                    continue;
                }

                let f_modified_time = f.get_modified_time();

                if let Some((last_modified_time, last_modified)) = last_modified.as_mut() {
                    if *last_modified_time < f_modified_time {
                        (*last_modified_time, *last_modified) = (f_modified_time, f);
                    }
                } else {
                    last_modified.replace((f_modified_time, f));
                }
            }

            if let Some((_, f)) = last_modified {
                gather_files::open_file(&conn, &env_ctx, &f.path_string).await;
            }
        },
        FilesUsage::FilesProvided => {
            for f in &env_ctx.opt.files {
                let f = gather_files::FileToOpen::new(f.as_str());

                if !f.is_text && !env_ctx.opt.open_non_text {
                    continue
                }

                gather_files::open_file(&conn, &env_ctx, &f.path_string).await;
            }
        }
    }
    if env_ctx.opt.back || env_ctx.opt.back_restore {
        let (win, buf) = &conn.initial_win_and_buf;
        conn.nvim_actions.set_current_win(win).await
            .expect("Cannot return to initial window");
        conn.nvim_actions.set_current_buf(buf).await
            .expect("Cannot return to initial buffer");

        if env_ctx.opt.back_restore {
            conn.nvim_actions.command("norm! A").await
                .expect("Cannot return to insert mode");
        }
    }
}


mod gather_files {
    use std::{path::{PathBuf, Path}, time::SystemTime};
    use once_cell::unsync::Lazy;

    use crate::context::EnvContext;

    const PWD: Lazy<PathBuf> = Lazy::new(|| {
        PathBuf::from(std::env::var("PWD").unwrap())
    });

    pub struct FileToOpen {
        pub path: PathBuf,
        pub path_string: String,
        pub is_text: bool,
    }

    impl FileToOpen {
        pub fn new<P: AsRef<Path>>(path: P) -> FileToOpen {
            let path = PWD.join(path);
            let path_string = path
                .to_string_lossy()
                .to_string();
            let is_text = is_text_file(&path_string);
            FileToOpen {
                path,
                path_string,
                is_text
            }
        }

        pub fn get_modified_time(&self) -> SystemTime {
            let f_meta = self.path
                .metadata()
                .expect("Cannot read dir entry metadata");
            f_meta
                .modified()
                .expect("Cannot read modified metadata")
        }
    }

    pub fn is_text_file(f: &str) -> bool {
        let file_cmd = std::process::Command::new("file")
            .arg(f)
            .output()
            .expect("Cannot get `file` output");
        let file_cmd_output = String::from_utf8(file_cmd.stdout)
            .expect("Non UTF8 `file` output");

        let filetype = file_cmd_output
            .split(": ")
            .last()
            .expect("Wrong `file` output format");

        filetype == "ASCII text\n"
    }


    pub async fn open_file(
        conn: &super::NeovimConnection,
        env_ctx: &EnvContext,
        f: &str
    ) {
        let cmd = format!("e {}", f);
        conn.nvim_actions.command(&cmd).await
            .expect("Cannot open file buffer");

        if env_ctx.opt.follow {
            conn.nvim_actions.command("norm! G").await
                .expect("Cannot execute follow command")

        } else if let Some(pattern) = &env_ctx.opt.pattern {
            let cmd = format!("norm! /{pattern}");
            conn.nvim_actions.command(&cmd).await
                .expect("Cannot execute follow command")

        } else if let Some(pattern_backwards) = &env_ctx.opt.pattern_backwards {
            let cmd = format!("norm! /{pattern_backwards}");
            conn.nvim_actions.command(&cmd).await
                .expect("Cannot execute follow command")
        }

        if let Some(lua) = &env_ctx.opt.lua {
            conn.nvim_actions.exec_lua(lua, vec![]).await
                .expect("Cannot execute command");
        }

        if let Some(command) = &env_ctx.opt.command {
            conn.nvim_actions.command(command).await
                .expect("Cannot execute command")
        }
    }
}