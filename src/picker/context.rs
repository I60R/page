






pub struct EnvContext {
    pub opt: crate::cli::Options,
    pub walkdir_usage: gather_env::WalkdirUsage,
}

pub mod gather_env {
    use super::EnvContext;

    pub fn enter() -> EnvContext {
        use clap::Parser;
        let mut opt = crate::cli::Options::parse();

        let recurse_depth = match opt.recurse_depth {
            Some(Some(n)) => n,
            Some(None) => 1,
            None => 0,
        };
        let mut walkdir_usage = WalkdirUsage::Disabled;
        if recurse_depth > 0 {
            walkdir_usage = WalkdirUsage::Enabled { recurse_depth }
        }

        EnvContext {
            opt,
            walkdir_usage
        }
    }

    pub enum WalkdirUsage {
        Enabled {
            recurse_depth: isize,
        },
        Disabled
    }


}

pub mod neovim_connected {
    use super::EnvContext;
    pub fn enter() -> EnvContext {
        let mut pipe_buf_usage = PipeBufferUsage::Disabled;

        let input_from_pipe = !atty::is(atty::Stream::Stdin);
        if input_from_pipe {
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

            pipe_buf_usage = PipeBufferUsage::Enabled {
                pipe_path
            }
        }

        todo!()
    }

    pub enum PipeBufferUsage {
        Enabled {
            pipe_path: String
        },
        Disabled
    }
}