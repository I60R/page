pub use env_context::Env;

pub mod env_context {

    #[derive(Debug)]
    pub struct Env {
        pub opt: crate::cli::Options,
        pub files_usage: FilesUsage,
        pub tmp_dir: std::path::PathBuf,
        pub page_id: u128,
        pub read_stdin_usage: ReadStdinUsage,
        pub split_usage: SplitUsage
    }

    pub fn enter() -> Env {
        let mut opt = crate::cli::get_options();

        // Fallback for neovim < 8.0 which don't uses $NVIM
        if opt.address.is_none() {
            if let Ok(address) = std::env::var("NVIM_LISTEN_ADDRESS") {
                opt.address.replace(address);
            }
        }

        // Treat empty -a value as if it wasn't provided
        if opt.address.as_deref().map_or(false, str::is_empty) {
            opt.address = None;
        }

        let input_from_pipe = !atty::is(atty::Stream::Stdin);

        let mut files_usage = FilesUsage::FilesProvided;
        if opt.files.is_empty() && !input_from_pipe {
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
            std::time::UNIX_EPOCH
                .elapsed()
                .unwrap()
                .as_nanos()
        };

        let mut split_usage = SplitUsage::Disabled;
        if opt.address.is_some() && opt.is_split_implied() {
            split_usage = SplitUsage::Enabled;
        }

        let mut pipe_buf_usage = ReadStdinUsage::Disabled;
        if input_from_pipe {
            pipe_buf_usage = ReadStdinUsage::Enabled;
        }

        Env {
            opt,
            files_usage,
            tmp_dir,
            page_id: pipe_path,
            read_stdin_usage: pipe_buf_usage,
            split_usage,
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
    pub enum ReadStdinUsage {
        Enabled,
        Disabled
    }

    #[derive(Debug)]
    pub enum SplitUsage {
        Enabled,
        Disabled
    }
}
