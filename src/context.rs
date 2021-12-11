/// A module that contains data collected throughout page invocation


/// Contains data available after cli options parsed
#[derive(Debug)]
pub struct EnvContext {
    pub opt: crate::cli::Options,
    pub prefetch_lines_count: usize,
    pub query_lines_count: usize,
    pub input_from_pipe: bool,
}

impl EnvContext {
    pub fn is_inst_close_flag_given_without_address(&self) -> bool {
        let EnvContext { opt, .. } = self;
        opt.address.is_none() && opt.instance_close.is_some()
    }

    pub fn is_split_flag_given_without_address(&self) -> bool {
        let EnvContext { opt, .. } = self;
        opt.address.is_none() && opt.output.split.implied
    }

    pub fn is_back_flag_given_without_address(&self) -> bool {
        let EnvContext { opt, .. } = self;
        opt.address.is_none() && (opt.back || opt.back_restore)
    }
}

pub mod gather_env {
    use super::EnvContext;

    pub fn enter() -> EnvContext {
        let input_from_pipe = !atty::is(atty::Stream::Stdin);
        let opt = {
            let mut opt = crate::cli::get_options();
            // Treat empty -a value as if it wasn't provided
            if opt.address.as_deref().map_or(false, str::is_empty) {
                opt.address = None;
            }
            // Override -O by -o, -p and -x flags and when page don't read from pipe
            if opt.output_open || opt.pty_path_print || opt.instance_close.is_some() || !input_from_pipe {
                opt.output.noopen_lines = None;
            }
            opt
        };
        let mut term_height = None;
        let prefetch_lines_count = match opt.output.noopen_lines.clone() {
            Some(Some(positive_number @ 0..)) => positive_number as usize,
            Some(negative_or_none) => term_height.get_or_insert_with(get_terminal_height).saturating_sub(negative_or_none.unwrap_or(3).abs() as usize),
            None => 0
        };
        let query_lines_count = match opt.output.query_lines.clone() {
            Some(Some(positive_number @ 0..)) => positive_number as usize,
            Some(negative_or_none) => term_height.get_or_insert_with(get_terminal_height).saturating_sub(negative_or_none.unwrap_or(3).abs() as usize),
            None => 0
        };
        EnvContext {
            opt,
            prefetch_lines_count,
            query_lines_count,
            input_from_pipe,
        }
    }

    fn get_terminal_height() -> usize {
        term_size::dimensions().map(|(_width, height)| height).expect("Cannot get terminal height")
    }
}


/// Contains data available after page was spawned from shell
#[derive(Debug)]
pub struct UsageContext {
    pub opt: crate::cli::Options,
    pub page_id: String,
    pub tmp_dir: std::path::PathBuf,
    pub prefetched_lines: check_usage::PrefetchedLines,
    pub query_lines_count: usize,
    pub input_from_pipe: bool,
    pub print_protection: bool,
}

impl UsageContext {
    pub fn is_focus_on_existed_instance_buffer_implied(&self) -> bool {
        let UsageContext { opt, .. } = self;
        !! opt.follow // Should focus in order to scroll buffer down
        || opt.command_auto // Autocommands should run on focused buffer
        || opt.command_post.is_some() // User command should run on focused buffer
        || (!opt.back && !opt.back_restore) // Otherwise, without -b and -B flags output buffer should be focused
    }
}

pub mod check_usage {
    use super::{EnvContext, UsageContext};

    pub fn enter(prefetched_lines_vec: Vec<String>, env_ctx: EnvContext) -> UsageContext {
        let EnvContext { input_from_pipe, opt, query_lines_count, .. } = env_ctx;
        let prefetched_lines = PrefetchedLines(prefetched_lines_vec);
        let tmp_dir = {
            let d = std::env::temp_dir().join("neovim-page");
            std::fs::create_dir_all(&d).expect("Cannot create temporary directory for page");
            d
        };
        let page_id = {
            // This should provide enough entropy for current use case
            let pid = std::process::id();
            let time = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos();
            format!("{}{}", pid, time)
        };
        let print_protection = {
            !! !input_from_pipe
            && !opt.page_no_protect
            && std::env::var_os("PAGE_REDIRECTION_PROTECT").map_or(true, |v| v != "" && v != "0")
        };
        UsageContext {
            opt,
            tmp_dir,
            page_id,
            input_from_pipe,
            print_protection,
            prefetched_lines,
            query_lines_count,
        }
    }
    pub struct PrefetchedLines(pub Vec<String>);

    impl std::fmt::Debug for PrefetchedLines {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{} Strings", self.0.len())
        }
    }
}


/// Contains data available after neovim is connected to page
#[derive(Debug)]
pub struct NeovimContext {
    pub opt: crate::cli::Options,
    pub page_id: String,
    pub prefetched_lines: check_usage::PrefetchedLines,
    pub query_lines_count: usize,
    pub inst_usage: connect_neovim::InstanceUsage,
    pub outp_buf_usage: connect_neovim::OutputBufferUsage,
    pub nvim_child_proc_spawned: bool,
    pub input_from_pipe: bool,
}

impl NeovimContext {
    pub fn is_split_flag_given_with_files(&self) -> bool {
         self.outp_buf_usage.is_create_split() && !self.opt.files.is_empty()
    }

    pub fn with_child_neovim_process_spawned(mut self) -> NeovimContext {
        self.nvim_child_proc_spawned = true;
        if !self.outp_buf_usage.is_disabled() {
            self.outp_buf_usage = connect_neovim::OutputBufferUsage::CreateSubstituting;
        }
        self
    }
}

pub mod connect_neovim {
    use super::{UsageContext, NeovimContext};

    pub fn enter(cli_ctx: UsageContext) -> NeovimContext {
        let should_focus_on_existed_instance_buffer = cli_ctx.is_focus_on_existed_instance_buffer_implied();
        let UsageContext { opt, input_from_pipe, page_id, prefetched_lines, query_lines_count, .. } = cli_ctx;
        let inst_usage = if let Some(name) = opt.instance.clone() {
            InstanceUsage::Enabled {
                name,
                focused: true,
                replace_content: true
            }
        } else if let Some(name) = opt.instance_append.clone() {
            InstanceUsage::Enabled {
                name,
                focused: should_focus_on_existed_instance_buffer,
                replace_content: false
            }
        } else {
            InstanceUsage::Disabled
        };
        let outp_buf_usage = if opt.output.split.implied {
            OutputBufferUsage::CreateSplit
        } else if input_from_pipe || opt.output.implied {
            OutputBufferUsage::CreateSubstituting
        } else {
            OutputBufferUsage::Disabled
        };
        NeovimContext {
            opt,
            page_id,
            prefetched_lines,
            query_lines_count,
            inst_usage,
            outp_buf_usage,
            input_from_pipe,
            nvim_child_proc_spawned: false,
        }
    }

    #[derive(Debug)]
    pub enum InstanceUsage {
        Enabled {
            name: String,
            focused: bool,
            replace_content: bool
        },
        Disabled,
    }

    impl InstanceUsage {
        pub fn is_enabled(&self) -> Option<&String> {
            if let InstanceUsage::Enabled { name, .. } = self {
                Some(name)
            } else {
                None
            }
        }

        pub fn is_enabled_and_should_be_focused(&self) -> bool {
            matches!(self, InstanceUsage::Enabled { focused: true, .. })
        }

        pub fn is_enabled_but_should_be_unfocused(&self) -> bool {
            matches!(self, InstanceUsage::Enabled { focused: false, .. })
        }

        pub fn is_enabled_and_should_replace_its_content(&self) -> bool {
            matches!(self, InstanceUsage::Enabled { replace_content: true, .. })
        }
    }

    #[derive(Debug)]
    pub enum OutputBufferUsage {
        CreateSubstituting,
        CreateSplit,
        Disabled,
    }

    impl OutputBufferUsage {
        pub fn is_disabled(&self) -> bool {
            matches!(self, Self::Disabled)
        }

        pub fn is_create_split(&self) -> bool {
            matches!(self, Self::CreateSplit)
        }
    }
}


/// Contains data available after buffer for output was found
#[derive(Debug)]
pub struct OutputContext {
    pub opt: crate::cli::Options,
    pub buf_pty_path: std::path::PathBuf,
    pub prefetched_lines: check_usage::PrefetchedLines,
    pub query_lines_count: usize,
    pub inst_usage: connect_neovim::InstanceUsage,
    pub restore_initial_buf_focus: output_buffer_available::RestoreInitialBufferFocus,
    pub input_from_pipe: bool,
    pub nvim_child_proc_spawned: bool,
    pub print_output_buf_pty: bool,
}

impl OutputContext {
    pub fn with_new_instance_output_buffer(mut self) -> OutputContext {
        if let connect_neovim::InstanceUsage::Enabled { focused, .. } = &mut self.inst_usage {
            *focused = true; // Obtains focus on buffer creation
        }
        self
    }

    pub fn is_query_enabled(&self) -> bool {
        self.query_lines_count != 0
    }
}

pub mod output_buffer_available {
    use crate::context::{NeovimContext, OutputContext};

    pub fn enter(nvim_ctx: NeovimContext, buf_pty_path: std::path::PathBuf) -> OutputContext {
        let NeovimContext { opt, nvim_child_proc_spawned, input_from_pipe, inst_usage, prefetched_lines, query_lines_count, .. } = nvim_ctx;
        let restore_initial_buf_focus = if !nvim_child_proc_spawned && opt.back {
            RestoreInitialBufferFocus::ViModeNormal
        } else if !nvim_child_proc_spawned && opt.back_restore {
            RestoreInitialBufferFocus::ViModeInsert
        } else {
            RestoreInitialBufferFocus::Disabled
        };
        let print_output_buf_pty = opt.pty_path_print || (!nvim_child_proc_spawned && !input_from_pipe);
        OutputContext {
            opt,
            buf_pty_path,
            prefetched_lines,
            query_lines_count,
            inst_usage,
            input_from_pipe,
            nvim_child_proc_spawned,
            restore_initial_buf_focus,
            print_output_buf_pty,
        }
    }

    #[derive(Debug)]
    pub enum RestoreInitialBufferFocus {
        ViModeNormal,
        ViModeInsert,
        Disabled,
    }

    impl RestoreInitialBufferFocus {
        pub fn is_disabled(&self) -> bool {
            matches!(self, Self::Disabled)
        }

        pub fn is_vi_mode_insert(&self) -> bool {
            matches!(self, Self::ViModeInsert)
        }
    }
}
