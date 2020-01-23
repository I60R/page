/// A module which contains data available on page invocation


/// Contains data required after page was spawned from shell
#[derive(Debug)]
pub struct CliContext {
    pub opt: crate::cli::Options,
    pub page_id: String,
    pub tmp_dir: std::path::PathBuf,
    pub input_from_pipe: bool,
    pub print_protection: bool,
    pub outp_buf_implied: bool,
    pub split_buf_implied: bool,
}

impl CliContext {
    pub fn is_inst_close_flag_given_without_address(&self) -> bool {
        let CliContext { opt, .. } = self;
        opt.address.is_none() && opt.instance_close.is_some()
    }

    pub fn is_split_flag_given_without_address(&self) -> bool {
        let CliContext { opt, split_buf_implied, .. } = self;
        opt.address.is_none() && *split_buf_implied
    }

    pub fn is_back_flag_given_without_address(&self) -> bool {
        let CliContext { opt, .. } = self;
        opt.address.is_none() && (opt.back || opt.back_restore)
    }

    pub fn is_query_flag_given_without_reading_from_pipe(&self) -> bool {
        let CliContext { opt, input_from_pipe, .. } = self;
        opt.query_lines > 0u64 && !input_from_pipe
    }
}

pub mod page_spawned {
    use crate::context::CliContext;

    pub fn enter() -> CliContext {
        let opt = crate::cli::get_options();
        let tmp_dir = {
            let d = std::env::temp_dir().join("neovim-page");
            std::fs::create_dir_all(&d).expect("Cannot create temporary directory for page");
            d
        };
        let page_id = {
            let pid = std::process::id();
            let time = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos();
            format!("{}{}", pid, time) // provides enough entropy for current use case
        };
        let input_from_pipe = !atty::is(atty::Stream::Stdin);
        let print_protection = !input_from_pipe
            && !opt.page_no_protect
            && std::env::var_os("PAGE_REDIRECTION_PROTECT").map_or(true, |v| v != "" && v != "0");
        let outp_buf_implied = opt.is_output_buffer_creation_implied();
        let split_buf_implied = opt.is_split_implied();
        CliContext {
            opt,
            tmp_dir,
            page_id,
            input_from_pipe,
            print_protection,
            outp_buf_implied,
            split_buf_implied,
        }
    }
}


/// Contains data required after neovim is attached
#[derive(Debug)]
pub struct NeovimContext {
    pub opt: crate::cli::Options,
    pub page_id: String,
    pub inst_usage: neovim_connected::InstanceUsage,
    pub outp_buf_usage: neovim_connected::OutputBufferUsage,
    pub nvim_child_proc_spawned: bool,
    pub input_from_pipe: bool,
}

impl NeovimContext {
    pub fn is_split_flag_given_with_files(&self) -> bool {
         self.outp_buf_usage.is_create_split() && !self.opt.files.is_empty()
    }

    pub fn with_child_neovim_process_spawned(mut self) -> NeovimContext {
        self.nvim_child_proc_spawned = true;
        self.outp_buf_usage = neovim_connected::OutputBufferUsage::CreateSubstituting;
        self
    }
}

pub mod neovim_connected {
    use crate::context::{CliContext, NeovimContext,};

    pub fn enter(cli_ctx: CliContext) -> NeovimContext {
        let CliContext { opt, page_id, input_from_pipe, split_buf_implied, .. } = cli_ctx;
        let inst_usage = if let Some(name) = opt.instance.clone() {
            InstanceUsage::Enabled {
                name,
                focus: true,
                replace_content: true
            }
        } else if let Some(name) = opt.instance_append.clone() {
            InstanceUsage::Enabled {
                name,
                focus: opt.is_focus_on_existed_instance_buffer_implied(),
                replace_content: false
            }
        } else {
            InstanceUsage::Disabled
        };
        let outp_buf_usage = if split_buf_implied {
            OutputBufferUsage::CreateSplit
        } else if input_from_pipe || opt.is_output_buffer_creation_implied() {
            OutputBufferUsage::CreateSubstituting
        } else {
            OutputBufferUsage::Disabled
        };
        NeovimContext {
            opt,
            page_id,
            inst_usage,
            outp_buf_usage,
            input_from_pipe,
            nvim_child_proc_spawned: false,
        }
    }

    #[derive(Debug)]
    pub enum InstanceUsage {
        Enabled { name: String, focus: bool, replace_content: bool, },
        Disabled,
    }

    impl InstanceUsage {
        pub fn is_enabled(&self) -> Option<&String> {
            if let InstanceUsage::Enabled { name, .. } = self { Some(name) } else { None }
        }

        pub fn is_enabled_and_focus_on_it_required(&self) -> bool {
            if let InstanceUsage::Enabled { focus, .. } = self { *focus } else { false }
        }

        pub fn is_enabled_but_focus_on_it_was_skipped(&self) -> bool {
            if let InstanceUsage::Enabled { focus, .. } = self { !focus } else { false }
        }

        pub fn is_enabled_and_should_replace_its_content(&self) -> bool {
            if let InstanceUsage::Enabled { replace_content, .. } = self { *replace_content } else { false }
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
            if let OutputBufferUsage::Disabled = self { true } else { false }
        }

        pub fn is_create_split(&self) -> bool {
            if let OutputBufferUsage::CreateSplit = self { true } else { false }
        }
    }
}


/// Contains data required after buffer for output was found
#[derive(Debug)]
pub struct OutputContext {
    pub opt: crate::cli::Options,
    pub inst_usage: neovim_connected::InstanceUsage,
    pub restore_initial_buf_focus: output_buffer_available::RestoreInitialBufferFocus,
    pub buf_pty_path: std::path::PathBuf,
    pub input_from_pipe: bool,
    pub nvim_child_proc_spawned: bool,
    pub print_output_buf_pty: bool,
}

impl OutputContext {
    pub fn is_query_disabled(&self) -> bool {
        self.opt.query_lines == 0
    }
}

pub mod output_buffer_available {
    use crate::context::{NeovimContext, OutputContext};

    pub fn enter(nvim_ctx: NeovimContext, buf_pty_path: std::path::PathBuf) -> OutputContext {
        let NeovimContext { opt, nvim_child_proc_spawned, inst_usage, input_from_pipe, .. } = nvim_ctx;
        let restore_initial_buf_focus = if !nvim_child_proc_spawned && opt.back {
            RestoreInitialBufferFocus::ViModeNormal
        } else if !nvim_child_proc_spawned && opt.back_restore {
            RestoreInitialBufferFocus::ViModeInsert
        } else {
            RestoreInitialBufferFocus::Disabled
        };
        let print_output_buf_pty = opt.sink_print || (!nvim_child_proc_spawned && !input_from_pipe);
        OutputContext {
            opt,
            inst_usage,
            buf_pty_path,
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
            if let RestoreInitialBufferFocus::Disabled = self { true } else { false }
        }

        pub fn is_vi_mode_insert(&self) -> bool {
            if let RestoreInitialBufferFocus::ViModeInsert = self { true } else { false }
        }
    }
}
