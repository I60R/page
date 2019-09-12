use structopt::{
    clap::{ArgGroup, AppSettings::*},
    StructOpt,
};


// Contains arguments provided by command line
#[derive(StructOpt, Debug)]
#[structopt(
    author,
    about,
    global_settings = &[DisableHelpSubcommand, DeriveDisplayOrder],
    group = splits_arg_group(),
    group = back_arg_group(),
    group = follow_arg_group(),
    group = instance_use_arg_group(),
)]
pub struct Options {
    /// Neovim session address
    #[structopt(short="a", env="NVIM_LISTEN_ADDRESS")]
    pub address: Option<String>,

    /// Neovim arguments for new child process
    #[structopt(short="A", env="NVIM_PAGE_ARGS")]
    pub arguments: Option<String>,

    /// Neovim config path for new child process [file:$XDG_CONFIG_HOME/page/init.vim]
    #[structopt(short="c")]
    pub config: Option<String>,

    /// Run command in output buffer after it's created
    #[structopt(short="e")]
    pub command: Option<String>,

    /// Run command in output buffer after it's created or connected as instance
    #[structopt(short="E")]
    pub command_post: Option<String>,

    /// Connect or create named output buffer. When connected, new content overwrites previous
    #[structopt(short="i")]
    pub instance: Option<String>,

    /// Connect or create named output buffer. When connected, new content appends to previous
    #[structopt(short="I")]
    pub instance_append: Option<String>,

    /// Close instance buffer with this name if exist [revokes implied options]
    #[structopt(short="x")]
    pub instance_close: Option<String>,

    /// Set output buffer name (displayed in statusline)
    #[structopt(short="n", env="PAGE_BUFFER_NAME")]
    pub name: Option<String>,

    /// Set output buffer filetype (for syntax highlighting)
    #[structopt(short="t", default_value="pager")]
    pub filetype: String,

    /// Create and use new output buffer (to display text from page stdin) [implied]
    #[structopt(short="o")]
    pub sink_open: bool,

    /// Print path to buffer pty (to redirect `command > /path/to/output`) [implied when page not piped]
    #[structopt(short="p")]
    pub sink_print: bool,

    /// Set $PWD as working dir for output buffer (to navigate paths with `gf`)
    #[structopt(short="P")]
    pub pwd: bool,

    /// Return back to current buffer
    #[structopt(short="b")]
    pub back: bool,

    /// Return back to current buffer and enter INSERT mode
    #[structopt(short="B")]
    pub back_restore: bool,

    /// Follow output instead of keeping top position (like `tail -f`)
    #[structopt(short="f")]
    pub follow: bool,

    /// Follow output instead of keeping top position also for each of <FILES>
    #[structopt(short="F")]
    pub follow_all: bool,

    /// Enable on-demand stdin reading with :Page <query_lines> command
    #[structopt(short="q", default_value="0")]
    pub query_lines: u64,

    /// Flush redirecting protection that prevents from producing junk and possible corruption of files
    /// by invoking commands like "unset NVIM_LISTEN_ADDRESS && ls > $(page -E q)" where "$(page -E q)"
    /// part not evaluates into /path/to/sink as expected but instead into neovim UI, which consists of
    /// a bunch of escape characters and strings. Many useless files could be created then and even
    /// overwriting of existed file might occur.
    /// To prevent that, a path to temporary directory is printed first, which causes "command > directory ..."
    /// to fail early as it's impossible to redirect text into directory.
    /// [env:PAGE_REDIRECTION_PROTECT: (0 to disable)]
    #[structopt(short="W")]
    pub page_no_protect: bool,

    /// Enable PageConnect PageDisconnect autocommands
    #[structopt(short="C")]
    pub command_auto: bool,

    /// Split right with ratio: window_width  * 3 / (<r-provided> + 1)
    #[structopt(short="r", parse(from_occurrences))]
    pub split_right: u8,

    /// Split left  with ratio: window_width  * 3 / (<l-provided> + 1)
    #[structopt(short="l", parse(from_occurrences))]
    pub split_left: u8,

    /// Split above with ratio: window_height * 3 / (<u-provided> + 1)
    #[structopt(short="u", parse(from_occurrences))]
    pub split_above: u8,

    /// Split below with ratio: window_height * 3 / (<d-provided> + 1)
    #[structopt(short="d", parse(from_occurrences))]
    pub split_below: u8,

    /// Split right and resize to <split_right_cols> columns
    #[structopt(short="R")]
    pub split_right_cols: Option<u8>,

    /// Split left  and resize to <split_left_cols>  columns
    #[structopt(short="L")]
    pub split_left_cols: Option<u8>,

    /// Split above and resize to <split_above_rows> rows
    #[structopt(short="U")]
    pub split_above_rows: Option<u8>,

    /// Split below and resize to <split_below_rows> rows
    #[structopt(short="D")]
    pub split_below_rows: Option<u8>,

    /// Open provided files in separate buffers [revokes implied options]
    #[structopt(name="FILES")]
    pub files: Vec<String>
}


fn instance_use_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("instances")
        .args(&["instance", "instance-append"])
        .multiple(false)
}

fn back_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("focusing")
        .args(&["back", "back-restore"])
        .multiple(false)
}

fn follow_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("following")
        .args(&["follow", "follow-all"])
        .multiple(false)
}

fn splits_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("splits")
        .args(&["split-left", "split-right", "split-above", "split-below"])
        .args(&["split-left-cols", "split-right-cols", "split-above-rows", "split-below-rows"])
        .multiple(false)
}


impl Options {
    pub fn is_focus_on_existed_instance_buffer_implied(&self) -> bool {
        self.follow                           // :term buffer should be focused in order to scroll it down.
        || self.instance.is_some()            // :term buffer should be focused in order to clear it.
        || self.command_auto                  // We expect autocommands to be run directly on instance buffer.
        || self.command_post.is_some()        // We expect user comamnd to be run directly on instance buffer.
        || (!self.back && !self.back_restore) // Should focus when -b is missing to be consistent with -i argument
    }

    pub fn is_split_implied(&self) -> bool {
        self.split_left_cols.is_some()
        || self.split_right_cols.is_some()
        || self.split_above_rows.is_some()
        || self.split_below_rows.is_some()
        || 0u8 < self.split_left
        || 0u8 < self.split_right
        || 0u8 < self.split_above
        || 0u8 < self.split_below
    }

    pub fn is_output_buffer_implied(&self) -> bool {
        self.instance_close.is_none() && self.files.is_empty() // These not implies creating output buffer
        || self.back
        || self.back_restore
        || self.follow
        || self.follow_all
        || self.sink_open
        || self.sink_print
        || self.pwd
        || 0u64 < self.query_lines
        || self.instance.is_some()
        || self.instance_append.is_some()
        || self.command.is_some()
        || self.command_post.is_some()
        || "pager" != &self.filetype
    }
}

pub fn get_options() -> Options {
    Options::from_args()
}
