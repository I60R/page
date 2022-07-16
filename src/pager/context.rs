/// A module that contains data collected throughout page invocation


/// Contains data available after cli options parsed
#[derive(Debug)]
pub struct EnvContext {
    pub opt: crate::cli::Options,
    pub prefetch_lines_count: usize,
    pub query_lines_count: usize,
    pub input_from_pipe: bool,
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
            if opt.output_open ||
                opt.pty_path_print ||
                opt.instance_close.is_some() ||
                !input_from_pipe
            {
                opt.output.noopen_lines = None;
            }

            opt
        };

        let term_height = once_cell::unsync::Lazy::new(|| {
            term_size::dimensions()
                .map(|(_w, h)| h)
                .expect("Cannot get terminal height")
        });

        let prefetch_lines_count = match opt.output.noopen_lines.unwrap_or_default() {
            Some(positive_number @ 0..) => positive_number as usize,
            Some(negative_number) => term_height.saturating_sub(negative_number.abs() as usize),
            None => term_height.saturating_sub(3),
        };

        let query_lines_count = match opt.output.query_lines.unwrap_or_default() {
            Some(positive_number @ 0..) => positive_number as usize,
            Some(negative_number) => term_height.saturating_sub(negative_number.abs() as usize),
            None => term_height.saturating_sub(3),
        };

        EnvContext {
            opt,
            prefetch_lines_count,
            query_lines_count,
            input_from_pipe,
        }
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

        // Should focus in order to scroll buffer down
        opt.follow ||

        // Autocommands should run on focused buffer
        opt.command_auto ||

        // User command should run on focused buffer
        opt.command_post.is_some() ||

        // Same with lua user command
        opt.lua_post.is_some() ||

        // Otherwise, without -b and -B flags output buffer should be focused
        (!opt.back && !opt.back_restore)
    }


    pub fn lines_has_been_prefetched(&mut self, lines: Vec<Vec<u8>>) {
        self.prefetched_lines = check_usage::PrefetchedLines(lines);
    }
}


pub mod check_usage {
    use super::{EnvContext, UsageContext};

    pub fn enter(env_ctx: EnvContext) -> UsageContext {

        let EnvContext {
            input_from_pipe,
            opt,
            query_lines_count,
            ..
        } = env_ctx;

        let prefetched_lines = PrefetchedLines(vec![]);

        let tmp_dir = {
            let d = std::env::temp_dir()
                .join("neovim-page");
            std::fs::create_dir_all(&d)
                .expect("Cannot create temporary directory for page");
            d
        };

        let page_id = {
            // This should provide enough entropy for current use case
            let pid = std::process::id();
            let time = std::time::UNIX_EPOCH.elapsed()
                .unwrap()
                .as_nanos();
            format!("{pid}{time}")
        };

        let print_protection = {
            !input_from_pipe &&
            !opt.page_no_protect &&
            std::env::var_os("PAGE_REDIRECTION_PROTECT")
                .map_or(true, |v| v != "" && v != "0")
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

    pub struct PrefetchedLines(pub Vec<Vec<u8>>);

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
         self.outp_buf_usage.is_create_split() &&
            !self.opt.files.is_empty()
    }


    pub fn child_neovim_process_has_been_spawned(&mut self) {
        self.nvim_child_proc_spawned = true;

        if !self.outp_buf_usage.is_disabled() {
            self.outp_buf_usage = connect_neovim::OutputBufferUsage::CreateSubstituting;
        }
    }
}


pub mod connect_neovim {
    use super::{UsageContext, NeovimContext};

    pub fn enter(cli_ctx: UsageContext) -> NeovimContext {
        let should_focus_on_existed_instance_buffer = cli_ctx
            .is_focus_on_existed_instance_buffer_implied();

        let UsageContext {
            opt,
            input_from_pipe,
            page_id,
            prefetched_lines,
            query_lines_count,
            ..
        } = cli_ctx;

        let mut inst_usage = InstanceUsage::Disabled;
        if let Some(name) = opt.instance.clone() {
            inst_usage = InstanceUsage::Enabled {
                name,
                focused: true,
                replace_content: true
            }
        } else if let Some(name) = opt.instance_append.clone() {
            inst_usage = InstanceUsage::Enabled {
                name,
                focused: should_focus_on_existed_instance_buffer,
                replace_content: false
            }
        }

        let mut outp_buf_usage = OutputBufferUsage::Disabled;
        if opt.is_output_split_implied() {
            outp_buf_usage = OutputBufferUsage::CreateSplit
        } else if input_from_pipe ||
            opt.is_output_implied() ||
            (opt.instance_close.is_none() && opt.files.is_empty())
        {
            outp_buf_usage = OutputBufferUsage::CreateSubstituting
        }

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
        pub fn is_enabled_and_should_be_focused(&self) -> bool {
            matches!(self, Self::Enabled { focused: true, .. })
        }


        pub fn is_enabled_but_should_be_unfocused(&self) -> bool {
            matches!(self, Self::Enabled { focused: false, .. })
        }


        pub fn is_enabled_and_should_replace_its_content(&self) -> bool {
            matches!(self, Self::Enabled { replace_content: true, .. })
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
    pub fn instance_output_buffer_has_been_created(&mut self) {
        if let connect_neovim::InstanceUsage::Enabled {
            focused,
            ..
        } = &mut self.inst_usage {

            // Obtains focus on buffer creation
            *focused = true;
        }
    }
}

pub mod output_buffer_available {
    use crate::context::{NeovimContext, OutputContext};

    pub fn enter(
        nvim_ctx: NeovimContext,
        buf_pty_path: std::path::PathBuf
    ) -> OutputContext {

        let NeovimContext {
            opt,
            nvim_child_proc_spawned,
            input_from_pipe,
            inst_usage,
            prefetched_lines,
            query_lines_count,
            ..
        } = nvim_ctx;

        let mut restore_initial_buf_focus = RestoreInitialBufferFocus::Disabled;
        if !nvim_child_proc_spawned {
            if opt.back {
                restore_initial_buf_focus = RestoreInitialBufferFocus::ViModeNormal
            } else if opt.back_restore {
                restore_initial_buf_focus = RestoreInitialBufferFocus::ViModeInsert
            }
        }

        let print_output_buf_pty = opt.pty_path_print ||
            (!nvim_child_proc_spawned && !input_from_pipe);

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
