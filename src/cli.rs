use structopt::{
    clap::{ArgGroup, AppSettings::*},
    StructOpt,
};


// Contains arguments provided by command line
#[derive(StructOpt, Debug)]
#[structopt(
    author,
    about,
    global_settings = &[ColoredHelp, DisableHelpSubcommand, UnifiedHelpMessage],
    group = splits_arg_group(),
    group = back_arg_group(),
    group = follow_arg_group(),
    group = instance_use_arg_group(),
)]
pub struct Options {
    /// Set title for output buffer (to display it in statusline)
    #[structopt(display_order=10, short="n", env="PAGE_BUFFER_NAME")]
    pub name: Option<String>,

    /// TCP/IP socked address or path to named pipe listened by running host neovim process
    #[structopt(display_order=100, short="a", env="NVIM_LISTEN_ADDRESS")]
    pub address: Option<String>,

    /// Arguments that will be passed to child neovim process spawned when <address> is missing
    #[structopt(display_order=101, short="A", env="NVIM_PAGE_ARGS")]
    pub arguments: Option<String>,

    /// Config that will be used by child neovim process spawned when <address> is missing [file:$XDG_CONFIG_HOME/page/init.vim]
    #[structopt(display_order=102, short="c")]
    pub config: Option<String>,

    /// Run command on output buffer after it was created or connected as instance {n} ~ ~ ~
    #[structopt(display_order=105, short="E")]
    pub command_post: Option<String>,

    /// Create output buffer with <instance> tag or use existed with replacing its content by text from page's stdin
    #[structopt(display_order=200, short="i")]
    pub instance: Option<String>,

    /// Create output buffer with <instance_append> tag or use existed with appending to its content text from page's stdin
    #[structopt(display_order=201, short="I")]
    pub instance_append: Option<String>,

    /// Close output buffer with <instance_close> tag if it exists [without other flags revokes implied by defalt -o or -p option] {n} ~ ~ ~
    #[structopt(display_order=202, short="x")]
    pub instance_close: Option<String>,

    /// Create and use output buffer (to redirect text from page's stdin) [implied by default unless -x and/or <FILE> provided without
    /// other flags]
    #[structopt(display_order=0, short="o")]
    pub output_open: bool,

    /// Print path of pty device associated with output buffer (to redirect text from commands respecting output buffer size and preserving
    /// colors) [implied if page isn't piped unless -x and/or <FILE> provided without other flags]
    #[structopt(display_order=2, short="p")]
    pub pty_path_print: bool,

    /// Cursor follows content of output buffer as it appears instead of keeping top position (like `tail -f`)
    #[structopt(display_order=5, short="f")]
    pub follow: bool,

    /// Cursor follows content of output and <FIlE> buffers as it appears instead of keeping top position
    #[structopt(display_order=6, short="F")]
    pub follow_all: bool,

    /// Return back to current buffer
    #[structopt(display_order=8, short="b")]
    pub back: bool,

    /// Return back to current buffer and enter into INSERT/TERMINAL mode
    #[structopt(display_order=9, short="B")]
    pub back_restore: bool,

    /// Enable PageConnect PageDisconnect autocommands
    #[structopt(display_order=103, short="C")]
    pub command_auto: bool,

    /// Flush redirection protection that prevents from producing junk and possible overwriting of existed files by invoking commands like
    /// `ls > $(NVIM_LISTEN_ADDRESS= page -E q)` where the RHS of > operator evaluates not into /path/to/pty as expected but into a bunch
    /// of whitespace-separated strings/escape sequences from neovim UI; bad things happens when some shells interpret this as many valid
    /// targets for text redirection. The protection is only printing of a path to the existed dummy directory always first before
    /// printing of a neovim UI might occur; this makes the first target for text redirection from page's output invalid and disrupts the
    /// whole redirection early before other harmful writes might occur.
    /// [env:PAGE_REDIRECTION_PROTECT; (0 to disable)] {n} ~ ~ ~
    #[structopt(display_order=800, short="W")]
    pub page_no_protect: bool,

    /// Open provided file in separate buffer [without other flags revokes implied by default -o or -p option]
    #[structopt(name="FILE")]
    pub files: Vec<String>,

    #[structopt(flatten)]
    pub output: OutputOptions
}

// Options that are required on output buffer creation
#[derive(StructOpt, Debug)]
pub struct OutputOptions {
    /// Run command in output buffer after it was created
    #[structopt(display_order=104, short="e")]
    pub command: Option<String>,

    /// Prefetch <open-lines> from page's stdin: if input is smaller then print it to stdout and exit without neovim usage [empty: term
    /// height; 0: disabled and default; ignored with -o, -p, -x and when page isn't piped]
    #[structopt(display_order=1, short="O")]
    pub open_lines: Option<Option<usize>>,

    /// Read no more than <query-lines> from page's stdin: next lines should be fetched by invoking :Page <query> command on neovim side
    /// [0: disabled and default; <query> is optional and defaults to <query-lines>]
    #[structopt(display_order=4, short="q", default_value="0", hide_default_value=true)]
    pub query_lines: usize,

    /// Set filetype on output buffer (to enable syntax highlighting) [pager: default; not works with text echoed by -O]
    #[structopt(display_order=7, short="t", default_value="pager", hide_default_value=true)]
    pub filetype: String,

    /// Allow to ender into INSERT/TERMINAL mode by pressing i, I, a, A keys [ignored on connected instance output buffer] {n} ~ ~ ~
    #[structopt(display_order=11, short="w")]
    pub writeable: bool,

    /// Set $PWD as working directory at output buffer (to navigate paths with `gf`)
    #[structopt(display_order=3, short="P")]
    pub pwd: bool,

    #[structopt(flatten)]
    pub split: SplitOptions,
}

// Options for split
#[derive(StructOpt, Debug)]
pub struct SplitOptions {
    /// Split left  with ratio: window_width  * 3 / (<l-provided> + 1)
    #[structopt(display_order=900, short="l", parse(from_occurrences))]
    pub split_left: u8,

    /// Split right with ratio: window_width  * 3 / (<r-provided> + 1)
    #[structopt(display_order=901, short="r", parse(from_occurrences))]
    pub split_right: u8,

    /// Split above with ratio: window_height * 3 / (<u-provided> + 1)
    #[structopt(display_order=902, short="u", parse(from_occurrences))]
    pub split_above: u8,

    /// Split below with ratio: window_height * 3 / (<d-provided> + 1)
    #[structopt(display_order=903, short="d", parse(from_occurrences))]
    pub split_below: u8,

    /// Split left  and resize to <split-left-cols>  columns
    #[structopt(display_order=904, short="L")]
    pub split_left_cols: Option<u8>,

    /// Split right and resize to <split-right-cols> columns
    #[structopt(display_order=905, short="R")]
    pub split_right_cols: Option<u8>,

    /// Split above and resize to <split-above-rows> rows
    #[structopt(display_order=906, short="U")]
    pub split_above_rows: Option<u8>,

    /// Split below and resize to <split-below-rows> rows {n} ~ ~ ~
    #[structopt(display_order=907, short="D")]
    pub split_below_rows: Option<u8>,
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


pub fn get_options() -> Options {
    Options::from_args()
}
