/// A module that contains data collected throughout page invocation

pub use gather_env::Env;
pub use check_usage::Usage;
pub use connect_neovim::Neovim;
pub use output_buffer_available::Output;


pub mod gather_env {
    /// Contains data available after cli options parsed
    #[derive(Debug)]
    pub struct Env {
        pub opt: crate::cli::Options,
        pub prefetch_usage: PrefetchLinesUsage,
        pub query_lines_count: usize,
        pub input_from_pipe: bool,
    }

    pub fn enter() -> Env {
        let input_from_pipe = !atty::is(atty::Stream::Stdin);

        let opt = parse_and_alter_opts(input_from_pipe);

        let (term_height, prefetch_usage) = determine_prefetch_usage(
            opt.output.noopen_lines,
            opt.pagerize,
            &opt.files,
            input_from_pipe
        );

        let query_lines_count = determine_query_lines_count(
            opt.output.query_lines,
            term_height
        );

        Env {
            opt,
            prefetch_usage,
            query_lines_count,
            input_from_pipe,
        }
    }


    fn parse_and_alter_opts(input_from_pipe: bool) -> crate::cli::Options {
        let mut opt = crate::cli::get_options();

        // Remove some arguments from pagerized invocation
        if opt.pagerize_hidden.is_some() {
            opt.pagerized();
        }

        // Don't pagerize with -p enabled or when not read from pipe
        if opt.pty_path_print || !input_from_pipe {
            opt.pagerize = None;
        }

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

        // Override -O by -o, -p and -x flags and when page don't read from pipe
        if opt.output_open ||
            opt.pty_path_print ||
            opt.instance_close.is_some() ||
            (!input_from_pipe && opt.files.len() != 1)
        {
            opt.output.noopen_lines = None;
        }

        opt
    }


    type TermHeight = usize;

    fn determine_prefetch_usage(
        noopen_lines: Option<Option<isize>>,
        pagerize: Option<Option<usize>>,
        files: &Vec<crate::cli::FileOption>,
        input_from_pipe: bool
    ) -> (TermHeight, PrefetchLinesUsage) {
        use once_cell::unsync::Lazy;

        let term_dimensions = Lazy::new(|| {
            term_size::dimensions()
                .expect("Cannot get terminal dimensions")
        });

        let (term_width, term_height) = (
            Lazy::new(|| term_dimensions.0),
            Lazy::new(|| term_dimensions.1),
        );

        let mut prefetch_lines_count = match noopen_lines {
            Some(Some(positive_number @ 0..)) => positive_number as usize,
            Some(Some(negative_number)) => term_height
                .saturating_sub(negative_number.unsigned_abs()),
            Some(None) => term_height
                .saturating_sub(3),
            None => 0
        };

        if prefetch_lines_count > 0 {
            match pagerize {
                Some(Some(n)) if prefetch_lines_count > n => {
                    prefetch_lines_count = n;
                },
                Some(None) if prefetch_lines_count > 90_000 => {
                    prefetch_lines_count = 90_000;
                }
                _ => {}
            }
        }

        let mut prefetch_usage = PrefetchLinesUsage::Disabled;
        if prefetch_lines_count != 0 && files.is_empty() && input_from_pipe {
            prefetch_usage = PrefetchLinesUsage::Enabled {
                line_count: prefetch_lines_count,
                term_width: *term_width,
                source: PrefetchLinesSource::Stdin,
            };
        } else if prefetch_lines_count != 0 && files.len() == 1 && !input_from_pipe {
            let last_file = files
                .last()
                .unwrap();

            if let crate::cli::FileOption::Path(f) = last_file {
                prefetch_usage = PrefetchLinesUsage::Enabled {
                    line_count: prefetch_lines_count,
                    term_width: *term_width,
                    source: PrefetchLinesSource::File(f.clone()),
                }
            }
        }

        (*term_height, prefetch_usage)
    }


    fn determine_query_lines_count(
        query_lines: Option<Option<isize>>,
        term_height: TermHeight,
    ) -> usize {
        match query_lines {
            Some(Some(positive_number @ 0..)) => positive_number as usize,
            Some(Some(negative_number)) => term_height
                .saturating_sub(negative_number.unsigned_abs()),
            Some(None) => term_height
                .saturating_sub(3),
            None => 0,
        }
    }


    #[derive(Debug)]
    pub enum PrefetchLinesUsage {
        Enabled {
            line_count: usize,
            term_width: usize,
            source: PrefetchLinesSource
        },
        Disabled,
    }

    #[derive(Debug)]
    pub enum PrefetchLinesSource {
        Stdin,
        File(String),
    }
}


pub mod check_usage {

    /// Contains data available after page was spawned from shell
    #[derive(Debug)]
    pub struct Usage {
        pub opt: crate::cli::Options,
        pub tmp_dir: std::path::PathBuf,
        pub page_id: u128,
        pub prefetched_lines: PrefetchedLines,
        pub query_lines_count: usize,
        pub input_from_pipe: bool,
        pub print_protection: bool,
    }

    impl Usage {
        pub fn is_focus_on_existed_instance_buffer_implied(&self) -> bool {
            let Usage { opt, .. } = self;

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
            self.prefetched_lines = PrefetchedLines(lines);
        }
    }


    pub fn enter(env_ctx: super::Env) -> Usage {

        let super::Env {
            input_from_pipe,
            opt,
            query_lines_count,
            ..
        } = env_ctx;

        let prefetched_lines = PrefetchedLines(vec![]);

        let tmp_dir = create_temp_directory();

        let page_id = if let Some([_, page_id]) = opt.pagerize_hidden.as_deref() {
            *page_id
        } else {
            create_page_id()
        };

        let print_protection = determine_if_should_print_protection(
            input_from_pipe,
            opt.page_no_protect,
        );

        Usage {
            opt,
            tmp_dir,
            page_id,
            prefetched_lines,
            query_lines_count,
            input_from_pipe,
            print_protection,
        }
    }

    fn create_temp_directory() -> std::path::PathBuf {
        let d = std::env::temp_dir()
            .join("neovim-page");
        std::fs::create_dir_all(&d)
            .expect("Cannot create temporary directory for page");
        d
    }

    fn create_page_id() -> u128 {
        // This should provide enough entropy for current use case
        std::time::UNIX_EPOCH
            .elapsed()
            .unwrap()
            .as_nanos()
    }


    fn determine_if_should_print_protection(
        input_from_pipe: bool,
        page_no_protect: bool,
    ) -> bool {
        !input_from_pipe && !page_no_protect &&
        std::env::var_os("PAGE_REDIRECTION_PROTECT")
            .map_or(true, |protect| protect != "" && protect != "0")
    }

    pub struct PrefetchedLines(pub Vec<Vec<u8>>);

    impl std::fmt::Debug for PrefetchedLines {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{} Strings", self.0.len())
        }
    }
}


pub mod connect_neovim {
    /// Contains data available after neovim is connected to page
    #[derive(Debug)]
    pub struct Neovim {
        pub opt: crate::cli::Options,
        pub page_id: u128,
        pub prefetched_lines: super::check_usage::PrefetchedLines,
        pub query_lines_count: usize,
        pub inst_usage: InstanceUsage,
        pub outp_buf_usage: OutputBufferUsage,
        pub nvim_child_proc_spawned: bool,
        pub input_from_pipe: bool,
    }

    impl Neovim {
        pub fn is_split_flag_given_with_files(&self) -> bool {
            self.outp_buf_usage.is_create_split() &&
                !self.opt.files.is_empty()
        }


        pub fn child_neovim_process_has_been_spawned(&mut self) {
            self.nvim_child_proc_spawned = true;

            if !self.outp_buf_usage.is_disabled() {
                self.outp_buf_usage = OutputBufferUsage::CreateSubstituting;
            }
        }
    }


    pub fn enter(cli_ctx: super::Usage) -> Neovim {
        let should_focus_on_existed_instance_buffer = cli_ctx
            .is_focus_on_existed_instance_buffer_implied();

        let super::Usage {
            opt,
            input_from_pipe,
            page_id,
            prefetched_lines,
            query_lines_count,
            ..
        } = cli_ctx;

        let inst_usage = determine_instance_usage(
            &opt.instance,
            &opt.instance_append,
            should_focus_on_existed_instance_buffer
        );

        let outp_buf_usage = determine_output_buffer_usage(
            opt.is_output_split_implied(),
            opt.is_output_implied(),
            &opt.instance_close,
            &opt.files,
            input_from_pipe
        );

        Neovim {
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

    fn determine_instance_usage(
        instance: &Option<String>,
        instance_append: &Option<String>,
        should_focus_on_existed_instance_buffer: bool
    ) -> InstanceUsage {
        let mut inst_usage = InstanceUsage::Disabled;

        if let Some(name) = instance.clone() {
            inst_usage = InstanceUsage::Enabled {
                name,
                focused: true,
                replace_content: true
            }
        } else if let Some(name) = instance_append.clone() {
            inst_usage = InstanceUsage::Enabled {
                name,
                focused: should_focus_on_existed_instance_buffer,
                replace_content: false
            }
        }

        inst_usage
    }

    fn determine_output_buffer_usage(
        is_output_split_implied: bool,
        is_output_implied: bool,
        instance_close: &Option<String>,
        files: &Vec<crate::cli::FileOption>,
        input_from_pipe: bool,
    ) -> OutputBufferUsage {
        let mut outp_buf_usage = OutputBufferUsage::Disabled;

        if is_output_split_implied {
            outp_buf_usage = OutputBufferUsage::CreateSplit;
        } else if input_from_pipe || is_output_implied ||
            (instance_close.is_none() && files.is_empty())
        {
            outp_buf_usage = OutputBufferUsage::CreateSubstituting;
        }

        outp_buf_usage
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


pub mod output_buffer_available {
    /// Contains data available after buffer for output was found
    #[derive(Debug)]
    pub struct Output {
        pub opt: crate::cli::Options,
        pub buf_pty_path: std::path::PathBuf,
        pub prefetched_lines: super::check_usage::PrefetchedLines,
        pub query_lines_count: usize,
        pub inst_usage: super::connect_neovim::InstanceUsage,
        pub input_from_pipe: bool,
        pub restore_initial_buf_focus: RestoreInitialBufferFocus,
        pub nvim_child_proc_spawned: bool,
        pub print_output_buf_pty: bool,
        pub page_id: u128,
        pub pagerized_page_size: Option<usize>,
    }

    impl Output {
        pub fn instance_output_buffer_has_been_created(&mut self) {
            if let super::connect_neovim::InstanceUsage::Enabled {
                focused,
                ..
            } = &mut self.inst_usage {

                // Obtains focus on buffer creation
                *focused = true;
            }
        }

        pub fn should_pagerize(&self, lines_displayed: usize) -> bool {
            self.pagerized_page_size
                .map_or(false, |pps| lines_displayed >= pps)
        }
    }


    pub fn enter(
        nvim_ctx: super::Neovim,
        buf_pty_path: std::path::PathBuf
    ) -> Output {

        let super::Neovim {
            opt,
            nvim_child_proc_spawned,
            input_from_pipe,
            inst_usage,
            prefetched_lines,
            query_lines_count,
            page_id,
            ..
        } = nvim_ctx;

        let restore_initial_buf_focus = determine_restore_initial_buffer_focus(
            nvim_child_proc_spawned,
            opt.back,
            opt.back_restore
        );

        let pagerized_page_size = determine_pagerized_page_size(&opt.pagerize);

        let print_output_buf_pty = opt.pty_path_print ||
            (!nvim_child_proc_spawned && !input_from_pipe);

        Output {
            opt,
            buf_pty_path,
            prefetched_lines,
            query_lines_count,
            inst_usage,
            input_from_pipe,
            restore_initial_buf_focus,
            nvim_child_proc_spawned,
            print_output_buf_pty,
            page_id,
            pagerized_page_size,
        }
    }

    fn determine_restore_initial_buffer_focus(
        nvim_child_proc_spawned: bool,
        back: bool,
        back_restore: bool,
    ) -> RestoreInitialBufferFocus {
        let mut restore_initial_buf_focus = RestoreInitialBufferFocus::Disabled;

        if !nvim_child_proc_spawned {
            if back {
                restore_initial_buf_focus = RestoreInitialBufferFocus::ViModeNormal;
            } else if back_restore {
                restore_initial_buf_focus = RestoreInitialBufferFocus::ViModeInsert;
            }
        }

        restore_initial_buf_focus
    }


    fn determine_pagerized_page_size(
        pagerize: &Option<Option<usize>>
    ) -> Option<usize> {
        match pagerize {
            Some(Some(number)) => Some(*number),
            Some(None) => Some(90_000),
            None => None,
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
