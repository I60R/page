use std::{path::{PathBuf, Path}, borrow::Cow};

use context::EnvContext;

pub(crate) mod cli;
pub(crate) mod context;

pub type NeovimConnection = connection::NeovimConnection<connection::Neovim<connection::IoWrite>>;
pub type NeovimBuffer = connection::Buffer<connection::IoWrite>;


#[tokio::main(worker_threads=2)]
async fn main() {

    main::init_logger();

    let env_ctx = context::gather_env::enter();

    main::warn_if_incompatible_options(&env_ctx.opt);

    connect_neovim(env_ctx).await;
}

mod main {
    pub fn init_logger() {
        let exec_time = std::time::Instant::now();

        let dispatch = fern::Dispatch::new().format(move |cb, msg, log_record| {
            let time = exec_time
                .elapsed()
                .as_micros();

            let lvl = log_record.level();
            let target = log_record.target();

            let mut module = log_record
                .module_path()
                .unwrap_or_default();
            let mut prep = " in ";
            if target == module {
                module = "";
                prep = "";
            };

            const BOLD: &str = "\x1B[1m";
            const UNDERL: &str = "\x1B[4m";
            const GRAY: &str = "\x1B[0;90m";
            const CLEAR: &str = "\x1B[0m";

            let mut msg_color = GRAY;
            if module.starts_with("page") {
                msg_color = ""
            };

            cb.finish(format_args!(
                "{BOLD}{UNDERL}[ {time:010} | {lvl:5} | \
                {target}{prep}{module} ]{CLEAR}\n{msg_color}{msg}{CLEAR}\n",
            ))
        });

        let log_lvl_filter = std::str::FromStr::from_str(
            std::env::var("RUST_LOG")
                .as_deref()
                .unwrap_or("warn")
        ).expect("Cannot parse $RUST_LOG value");

        dispatch
            .level(log_lvl_filter)
            .chain(std::io::stderr())
            .apply()
            .expect("Cannot initialize logger");
    }


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

    connect_neovim::init_panic_hook();

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


mod connect_neovim {
    // If neovim dies unexpectedly it messes the terminal
    // so terminal state must be cleaned
    pub fn init_panic_hook() {
        let default_panic_hook = std::panic::take_hook();

        std::panic::set_hook(Box::new(move |panic_info| {
            let try_spawn_reset = std::process::Command::new("reset")
                .spawn()
                .and_then(|mut child| child.wait());

            match try_spawn_reset {
                Ok(exit_code) if exit_code.success() => {}

                Ok(err_exit_code) => {
                    log::error!(
                        target: "termreset",
                        "`reset` exited with status: {err_exit_code}"
                    )
                }
                Err(e) => {
                    log::error!(target: "termreset", "`reset` failed: {e:?}");
                }
            }

            default_panic_hook(panic_info);
        }));
    }
}


async fn gather_files(
    env_ctx: EnvContext,
    conn: NeovimConnection,
) {
    use context::gather_env::RecursiveOpenUsage;
    if let RecursiveOpenUsage::Enabled { recurse_depth } = env_ctx.walkdir_usage {
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

            gather_files::open_file(&conn, &f.path_string).await;
        }
    } else {
        if env_ctx.opt.files.is_empty() {
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
                gather_files::open_file(&conn, &f.path_string).await;
            }

            return
        }

        for f in env_ctx.opt.files {
            let f = gather_files::FileToOpen::new(f.as_str());

            if !f.is_text && !env_ctx.opt.open_non_text {
                continue
            }

            gather_files::open_file(&conn, &f.path_string).await;
        }
    }
}


mod gather_files {
    use std::{path::{PathBuf, Path}, time::SystemTime};
    use once_cell::unsync::Lazy;

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


    pub async fn open_file(conn: &super::NeovimConnection, f: &str) {
        let cmd = format!("e {}", f);
        conn.nvim_actions.command(&cmd).await
            .expect("Cannot open file buffer");
    }
}