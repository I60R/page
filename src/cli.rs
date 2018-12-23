use structopt::{
    clap::{ArgGroup, AppSettings::*},
    StructOpt,
};


// Contains arguments provided by command line
#[derive(StructOpt, Debug)]
#[structopt(raw(
    global_settings="&[DisableHelpSubcommand, DeriveDisplayOrder]",
    group="splits_arg_group()",
    group="back_arg_group()",
    group="follow_arg_group()",
    group="instance_use_arg_group()"))]
pub(crate) struct Options {
    /// Neovim session address
    #[structopt(short="a", env="NVIM_LISTEN_ADDRESS")]
    pub address: Option<String>,

    /// Neovim arguments passed to neovim child process
    #[structopt(short="A", env="NVIM_PAGE_ARGS")]
    pub arguments: Option<String>,

    /// Neovim config override for neovim child process [file:$XDG_CONFIG_HOME/page/init.vim]
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
        .args(&["instance", "instance_append"])
        .multiple(false)
}

fn back_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("focusing")
        .args(&["back", "back_restore"])
        .multiple(false)
}

fn follow_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("following")
        .args(&["follow", "follow_all"])
        .multiple(false)
}

fn splits_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("splits")
        .args(&["split_left", "split_right", "split_above", "split_below"])
        .args(&["split_left_cols", "split_right_cols", "split_above_rows", "split_below_rows"])
        .multiple(false)
}


pub(crate) fn get_options() -> Options {
    Options::from_args()
}