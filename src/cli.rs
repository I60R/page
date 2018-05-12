use structopt::{clap::{ ArgGroup, AppSettings::* }};


#[derive(StructOpt)]
#[structopt(raw(
    global_settings="&[DisableHelpSubcommand, DeriveDisplayOrder]",
    group="splits_arg_group()",
    group="instance_arg_group()"))]
pub struct Opt {
    /// Neovim session address
    #[structopt(short="s", env="NVIM_LISTEN_ADDRESS")]
    pub address: Option<String>,

    /// Run command in pager buffer
    #[structopt(short="e")]
    pub command: Option<String>,

    /// Use named instance buffer instead of opening new
    #[structopt(short="i")]
    pub instance: Option<String>,

    /// The same as "-i" but with append mode
    #[structopt(short="a")]
    pub instance_append: Option<String>,

    /// Close named instance buffer
    #[structopt(short="x")]
    pub instance_close: Option<String>,

    /// Filetype hint, allows color highlighting when reading from stdin
    #[structopt(short="t", default_value="pager")]
    pub filetype: String,

    /// Stay focused on current buffer
    #[structopt(short="b")]
    pub back: bool,

    /// Print path to /dev/pty/* associated with pager buffer
    #[structopt(short="p")]
    pub print_pty_path: bool,

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


fn instance_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("instances")
        .args(&["instance", "instance_append"])
        .multiple(false)
}

fn splits_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("splits")
        .args(&["split_left", "split_right", "split_above", "split_below"])
        .args(&["split_left_cols", "split_right_cols", "split_above_rows", "split_below_rows"])
        .multiple(false)
}