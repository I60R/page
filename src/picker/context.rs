
#[derive(Debug)]
pub struct EnvContext {
    pub opt: crate::cli::Options,
    pub walkdir_usage: gather_env::WalkdirUsage,
    pub tmp_dir: std::path::PathBuf,
    pub page_id: String,
    pub pipe_buf_usage: gather_env::PipeBufferUsage,
}

pub mod gather_env {
    use super::EnvContext;

    pub fn enter() -> EnvContext {
        let mut opt = crate::cli::get_options();

        let recurse_depth = match opt.recurse_depth {
            Some(Some(n)) => n,
            Some(None) => 1,
            None => 0,
        };
        let mut walkdir_usage = WalkdirUsage::Disabled;
        if recurse_depth > 0 {
            walkdir_usage = WalkdirUsage::Enabled { recurse_depth }
        }

        let tmp_dir = {
            let d = std::env::temp_dir()
                .join("neovim-page");
            std::fs::create_dir_all(&d)
                .expect("Cannot create temporary directory for page");
            d
        };

        let pipe_path = {
            // This should provide enough entropy for current use case
            let pid = std::process::id();
            let time = std::time::UNIX_EPOCH.elapsed()
                .unwrap()
                .as_nanos();
            format!("{pid}{time}")
        };

        let input_from_pipe = !atty::is(atty::Stream::Stdin);
        let mut pipe_buf_usage = PipeBufferUsage::Disabled;
        if input_from_pipe {
            pipe_buf_usage = PipeBufferUsage::Enabled {
                pipe_name: format!("{pipe_path}-read")
            }
        }

        EnvContext {
            opt,
            walkdir_usage,
            tmp_dir,
            page_id: pipe_path,
            pipe_buf_usage,
        }
    }

    #[derive(Debug)]
    pub enum WalkdirUsage {
        Enabled {
            recurse_depth: isize,
        },
        Disabled
    }

    #[derive(Debug)]
    pub enum PipeBufferUsage {
        Enabled {
            pipe_name: String
        },
        Disabled
    }


}

pub mod neovim_connected {
    use super::EnvContext;

    pub fn enter() -> EnvContext {
        todo!()
    }

}