use structopt::{clap::{ ArgGroup, AppSettings::* }};


#[derive(StructOpt)]
#[structopt(raw(
    global_settings="&[DisableHelpSubcommand, DeriveDisplayOrder]",
    group="splits_arg_group()",
    group="back_arg_group()",
    group="instance_use_arg_group()"))]
pub struct Opt {
    /// Neovim session address
    #[structopt(short="a", env="NVIM_LISTEN_ADDRESS")]
    pub address: Option<String>,

    /// Run command in pager buffer when reading begins
    #[structopt(short="e")]
    pub command: Option<String>,

    /// Run command in pager buffer after reading was done
    #[structopt(short="E")]
    pub command_post: Option<String>,

    /// Use named instance buffer if exist, or spawn new. New content will overwrite
    #[structopt(short="i")]
    pub instance: Option<String>,

    /// Use named instance buffer if exist, or spawn new. New content will be appended
    #[structopt(short="I")]
    pub instance_append: Option<String>,

    /// Only closes named instance buffer if exists
    #[structopt(short="x")]
    pub instance_close: Option<String>,

    /// Filetype hint for syntax highlighting when page reads from stdin
    #[structopt(short="t", default_value="pager")]
    pub filetype: String,

    /// Open new buffer [set by default, unless only <instance_close> or <FILES> provided]
    #[structopt(short="o")]
    pub pty_open: bool,

    /// Print path to /dev/pty/* for redirecting [set by default when don't reads from pipe]
    #[structopt(short="p")]
    pub pty_print: bool,

    /// Stay focused on current buffer
    #[structopt(short="b")]
    pub back: bool,

    /// Stay focused on current buffer and keep INSERT mode
    #[structopt(short="B")]
    pub back_insert: bool,

    /// Split right with ratio: window_width  * 3 / (<r provided> + 1)
    #[structopt(short="r", parse(from_occurrences))]
    pub split_right: u8,

    /// Split left  with ratio: window_width  * 3 / (<l provided> + 1)
    #[structopt(short="l", parse(from_occurrences))]
    pub split_left: u8,

    /// Split above with ratio: window_height * 3 / (<u provided> + 1)
    #[structopt(short="u", parse(from_occurrences))]
    pub split_above: u8,

    /// Split below with ratio: window_height * 3 / (<d provided> + 1)
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

    /// Open these files in separate buffers
    #[structopt(name="FILES")]
    pub files: Vec<String>
}


fn instance_use_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("instances")
        .args(&["instance", "instance_append"])
        .multiple(false)
}

fn back_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("backs")
        .args(&["back", "back_insert"])
        .multiple(false)
}

fn splits_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("splits")
        .args(&["split_left", "split_right", "split_above", "split_below"])
        .args(&["split_left_cols", "split_right_cols", "split_above_rows", "split_below_rows"])
        .multiple(false)
}
