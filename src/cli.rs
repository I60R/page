use neovim_lib::neovim_api::{Buffer, Window};
use std::process;
use structopt::clap::{ArgGroup, AppSettings::*};


// Contains arguments provided by command line
#[derive(StructOpt, Debug)]
#[structopt(raw(
    global_settings="&[DisableHelpSubcommand, DeriveDisplayOrder]",
    group="splits_arg_group()",
    group="back_arg_group()",
    group="instance_use_arg_group()"))]
pub(crate) struct Options {
    /// Neovim session address
    #[structopt(short="a", env="NVIM_LISTEN_ADDRESS")]
    pub address: Option<String>,

    /// Neovim arguments when a new session is started
    #[structopt(short="A", env="NVIM_PAGE_ARGS")]
    pub arguments: Option<String>,

    /// Shorthand for neovim config argument when a new session is started
    #[structopt(short="c")]
    pub config: Option<String>,

    /// Run command in a pager buffer when neovim config is sourced and reading begins
    #[structopt(short="e")]
    pub command: Option<String>,

    /// Run command in a pager buffer after reading was done
    #[structopt(short="E")]
    pub command_post: Option<String>,

    /// Use named instance buffer if exist or spawn new. New overwrites previous
    #[structopt(short="i")]
    pub instance: Option<String>,

    /// Use named instance buffer if exist or spawn new. New content appends to previous
    #[structopt(short="I")]
    pub instance_append: Option<String>,

    /// Close named instance buffer if exists [revokes implied options]
    #[structopt(short="x")]
    pub instance_close: Option<String>,

    /// Set name for buffer that displays stdin content
    #[structopt(short="n", env="PAGE_BUFFER_NAME")]
    pub name: Option<String>,

    /// Set filetype for buffer that displays stdin content (for syntax highlighting)
    #[structopt(short="t", default_value="pager")]
    pub filetype: String,

    /// Open a new buffer to display stdin context [implied]
    #[structopt(short="o")]
    pub pty_open: bool,

    /// Print /dev/pty/* path [implied when page don't read from pipe] (for > redirecting)
    #[structopt(short="p")]
    pub pty_print: bool,

    /// Focus back to current buffer
    #[structopt(short="b")]
    pub back: bool,

    /// Focus back to current buffer and keep INSERT mode
    #[structopt(short="B")]
    pub back_insert: bool,

    /// Follow output instead of keeping top position (like tail -f)
    #[structopt(short="f")]
    pub follow: bool,

    /// Follow output instead of keeping top position and scroll each of <FILES> provided to the bottom
    #[structopt(short="F")]
    pub follow_all: bool,

    /// Flush redirecting protection that prevents from producing junk and possible corruption of files
    /// by invoking commands like "unset NVIM_LISTEN_ADDRESS && ls > $(page -E q)"  where
    /// "$(page -E q)" or similar capture evaluates not into "/dev/pty/*" as expected but into
    /// whole neovim UI which consists of a bunch of characters and strings.
    /// Many useless files would be created for each word and even overwriting might occur.
    /// To prevent that, a path to temporary directory is printed first and "ls > directory ..."
    /// just fails, since it's impossible to redirect text into directory.
    /// [env: PAGE_REDIRECTION_PROTECT: (0 to disable)]
    #[structopt(short="W")]
    pub page_no_protect: bool,

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
        .args(&["back", "back_insert"])
        .multiple(false)
}

fn splits_arg_group() -> ArgGroup<'static> {
    ArgGroup::with_name("splits")
        .args(&["split_left", "split_right", "split_above", "split_below"])
        .args(&["split_left_cols", "split_right_cols", "split_above_rows", "split_below_rows"])
        .multiple(false)
}



// Context in which application is invoked. Contains related read-only data
#[derive(Debug)]
pub(crate) struct Context<'a> {
    pub opt: &'a Options,
    pub initial_position: (Window, Buffer),
    pub nvim_child_process: Option<process::Child>,
    pub switch_back_mode: SwitchBackMode,
    pub instance_mode: InstanceMode,
    pub creates: bool,
    pub prints: bool,
    pub splits: bool,
    pub focuses: bool,
    pub piped: bool,
}

impl <'a> Context<'a> {
    pub(crate) fn new (
        opt: &'a Options,
        nvim_child_process: Option<process::Child>,
        initial_position: (Window, Buffer),
        piped: bool,
    ) -> Context<'a> {
        use self::SwitchBackMode::*;
        let switch_back_mode = if nvim_child_process.is_some() {
             NoSwitch
        } else if opt.back {
             Normal
        } else if opt.back_insert {
             Insert
        } else {
             NoSwitch
        };
        use self::InstanceMode::*;
        let instance_mode = if let Some(instance) = opt.instance.as_ref() {
            Replace(instance.clone())
        } else if let Some(instance) = opt.instance_append.as_ref() {
            Append(instance.clone())
        } else {
            NoInstance
        };
        let split_flag_provided = Self::has_split_flag_provided(&opt);
        let creates = !Self::has_early_exit_condition(&opt, piped, split_flag_provided);
        let splits = nvim_child_process.is_none() && split_flag_provided;
        let prints = opt.pty_print || !piped && nvim_child_process.is_none();
        let focuses = opt.follow || switch_back_mode.is_no_switch() || instance_mode.is_replace();
        Context {
            opt,
            instance_mode,
            initial_position,
            nvim_child_process,
            switch_back_mode,
            creates,
            prints,
            splits,
            focuses,
            piped,
        }
    }

    fn has_split_flag_provided(opt: &Options) -> bool {
        *& opt.split_left_cols.is_some() || opt.split_right_cols.is_some()
        || opt.split_above_rows.is_some() || opt.split_below_rows.is_some()
        || opt.split_left != 0 || opt.split_right != 0
        || opt.split_above != 0 || opt.split_below != 0
    }

    fn has_early_exit_condition(opt: &Options, piped: bool, splits: bool) -> bool {
        let has_early_exit_opt = opt.instance_close.is_some() || !opt.files.is_empty();
        *& has_early_exit_opt && !piped && !splits
        && !opt.back && !opt.back_insert
        && !opt.follow
        && !opt.pty_open && !opt.pty_print
        && opt.instance.is_none() && opt.instance_append.is_none()
        && opt.command.is_none() && opt.command_post.is_none()
        && &opt.filetype == "pager"
    }
}



#[derive(Debug)]
pub(crate) enum SwitchBackMode {
    Normal,
    Insert,
    NoSwitch,
}

impl SwitchBackMode {
    pub(crate) fn is_no_switch(&self) -> bool {
        if let SwitchBackMode::NoSwitch = self { true } else { false }
    }
}



#[derive(Debug, Clone)]
pub(crate) enum InstanceMode {
    Append(String),
    Replace(String),
    NoInstance,
}

impl InstanceMode {
    pub(crate) fn is_replace(&self) -> bool {
        if let InstanceMode::Replace(_) = self { true } else { false }
    }
    pub(crate) fn is_any(&self) -> Option<&String> {
        match self {
            InstanceMode::Append(instance_name) | InstanceMode::Replace(instance_name) => Some(instance_name),
            InstanceMode::NoInstance => None,
        }
    }
}
