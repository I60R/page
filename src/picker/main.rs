use context::EnvContext;

pub(crate) mod cli;
pub(crate) mod neovim;
pub(crate) mod context;

pub type NeovimConnection = connection::NeovimConnection<neovim::NeovimActions>;
pub type NeovimBuffer = connection::Buffer<connection::IoWrite>;


#[tokio::main(worker_threads=2)]
async fn main() {

    main::init_logger();

    let env_ctx = context::gather_env::enter();

    main::warn_if_incompatible_options(&env_ctx.opt);

    gather_files(env_ctx).await;
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

async fn gather_files(env_ctx: EnvContext) {
    use context::gather_env::WalkdirUsage;
    if let WalkdirUsage::Enabled { recurse_depth } = env_ctx.walkdir_usage {

    }
}


async fn send_input_from_pipe(env_ctx: EnvContext) {
    use context::neovim_connected::PipeBufferUsage;
}

mod neovim_api_usage {
    use super::NeovimConnection;

    /// This struct implements actions that should be done
    /// before output buffer is available
    pub struct ApiActions<'a> {
        nvim_conn: &'a mut NeovimConnection,
    }

    pub fn begin<'a>(
        nvim_conn: &'a mut NeovimConnection,
    ) -> ApiActions<'a> {
        ApiActions {
            nvim_conn,
        }
    }

    impl<'a> ApiActions<'a> {}
}

mod output_buffer_usage {
    use super::{NeovimConnection, NeovimBuffer};
}
