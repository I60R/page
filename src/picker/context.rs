
#[derive(Debug)]
pub struct EnvContext {
    pub opt: crate::cli::Options,
    pub files_usage: gather_env::FilesUsage,
    pub tmp_dir: std::path::PathBuf,
    pub page_id: String,
    pub pipe_buf_usage: gather_env::PipeBufferUsage,
}

pub mod gather_env {
    use super::EnvContext;

    pub fn enter() -> EnvContext {
        let mut opt = crate::cli::get_options();

        // Treat empty -a value as if it wasn't provided
        if opt.address.as_deref().map_or(false, str::is_empty) {
            opt.address = None;
        }

        let mut files_usage = FilesUsage::FilesProvided;
        if opt.files.is_empty() {
            files_usage = FilesUsage::LastModifiedFile;
        }
        let recurse_depth = match opt.recurse_depth {
            Some(Some(n)) => n,
            Some(None) => 1,
            None => 0,
        };
        if recurse_depth > 0 {
            files_usage = FilesUsage::RecursiveCurrentDir { recurse_depth }
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
            files_usage,
            tmp_dir,
            page_id: pipe_path,
            pipe_buf_usage,
        }
    }

    #[derive(Debug)]
    pub enum FilesUsage {
        RecursiveCurrentDir {
            recurse_depth: usize,
        },
        LastModifiedFile,
        FilesProvided,
    }

    #[derive(Debug)]
    pub enum PipeBufferUsage {
        Enabled {
            pipe_name: String
        },
        Disabled
    }
}