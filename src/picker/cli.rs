use clap::{
    Parser,
    ArgGroup,
    ArgAction,
    ValueHint,
};

/// File picker for neovim inspired by neovim-remote
#[derive(Parser, Debug)]
#[clap(
    author,
    disable_help_subcommand = true,
    allow_negative_numbers = true,
    group = splits_arg_group(),
    group = back_arg_group(),
    group = follow_arg_group(),
)]
pub struct Options {
    /// Open provided files as editable
    /// [if none provided nv opens last modified file in currend directory]
    #[clap(name="FILE", value_hint=ValueHint::FilePath)]
    pub files: Vec<FileOption>,

    /// Open non-text files including directories, binaries, images etc
    #[clap(display_order=1, short='o')]
    pub open_non_text: bool,

    /// Ignoring [FILE] open all text files in the current directory
    /// and recursively open all text files in its subdirectories
    /// [0: disabled and default;
    /// empty: defaults to 1 and implied if no <RECURSE_DEPTH> provided;
    /// <RECURSE_DEPTH>: also opens in subdirectories at this level of depth]
    #[clap(short='O')]
    pub recurse_depth: Option<Option<usize>>,

    /// Open in `page` instead (just postfix shortcut) {n}
    /// ~ ~ ~
    #[clap(short='v')]
    pub view_only: bool,

    /// Open each [FILE] at last line
    #[clap(short='f')]
    pub follow: bool,

    /// Open and search for a specified <PATTERN>
    #[clap(short='p')]
    pub pattern: Option<String>,

    /// Open and search backwars for a specified <PATTERN_BACKWARDS>
    #[clap(short='P')]
    pub pattern_backwards: Option<String>,

    /// Return back to current buffer
    #[clap(short='b')]
    pub back: bool,

    /// Return back to current buffer and enter into INSERT/TERMINAL mode
    #[clap(short='B')]
    pub back_restore: bool,

    /// Keep Page process until buffer is closed
    /// (for editing git commit message)
    #[clap(short='k')]
    pub keep: bool,

    /// Keep Page process until first write occur,
    /// then close buffer and neovim if it was spawned by nv {n}
    /// ~ ~ ~
    #[clap(short='K')]
    pub keep_until_write: bool,

    /// TCP/IP socket address or path to named pipe listened
    /// by running host neovim process
    #[clap(short='a', env="NVIM")]
    pub address: Option<String>,

    /// Arguments that will be passed to child neovim process
    /// spawned when <ADDRESS> is missing
    #[clap(short='A', env="NVIM_PAGE_PICKER_ARGS")]
    pub arguments: Option<String>,

    /// Config that will be used by child neovim process spawned
    /// when <ADDRESS> is missing [file: $XDG_CONFIG_HOME/page/init.vim]
    #[clap(short='c', value_hint=ValueHint::AnyPath)]
    pub config: Option<String>,

    /// Override filetype on each [FILE] buffer
    /// (to enable custom syntax highlighting) [text: default] {n}
    /// ~ ~ ~
    #[clap(short='t')]
    pub filetype: Option<String>,

    /// Run command  on each [FILE] buffer after it was created
    #[clap(short='e')]
    pub command: Option<String>,

    /// Run lua expr on each [FILE] buffer after it was created {n}
    /// ~ ~ ~
    #[clap(long="e")]
    pub lua: Option<String>,

    #[clap(flatten)]
    pub split: SplitOptions,
}

impl Options {
    pub fn is_split_implied(&self) -> bool {
        self.split.split_left_cols.is_some() ||
        self.split.split_right_cols.is_some() ||
        self.split.split_above_rows.is_some() ||
        self.split.split_below_rows.is_some() ||
        self.split.split_left > 0u8 ||
        self.split.split_right > 0u8 ||
        self.split.split_above > 0u8 ||
        self.split.split_below > 0u8
    }
}


// Options for split
#[derive(Parser, Debug)]
pub struct SplitOptions {
    /// Split left  with ratio: window_width  * 3 / (<l-PROVIDED> + 1)
    #[clap(display_order=900, short='l', action=ArgAction::Count)]
    pub split_left: u8,

    /// Split right with ratio: window_width  * 3 / (<r-PROVIDED> + 1)
    #[clap(display_order=901, short='r', action=ArgAction::Count)]
    pub split_right: u8,

    /// Split above with ratio: window_height * 3 / (<u-PROVIDED> + 1)
    #[clap(display_order=902, short='u', action=ArgAction::Count)]
    pub split_above: u8,

    /// Split below with ratio: window_height * 3 / (<d-PROVIDED> + 1)
    #[clap(display_order=903, short='d', action=ArgAction::Count)]
    pub split_below: u8,

    /// Split left  and resize to <SPLIT_LEFT_COLS>  columns
    #[clap(display_order=904, short='L')]
    pub split_left_cols: Option<u8>,

    /// Split right and resize to <SPLIT_RIGHT_COLS> columns
    #[clap(display_order=905, short='R')]
    pub split_right_cols: Option<u8>,

    /// Split above and resize to <SPLIT_ABOVE_ROWS> rows
    #[clap(display_order=906, short='U')]
    pub split_above_rows: Option<u8>,

    /// Split below and resize to <SPLIT_BELOW_ROWS> rows {n}
    /// ^
    #[clap(display_order=907, short='D')]
    pub split_below_rows: Option<u8>,

    /// With any of -r -l -u -d -R -L -U -D open floating window instead of split
    /// [to not overwrite data in the current terminal] {n}
    /// ~ ~ ~
    #[clap(display_order=908, short='+')]
    pub popup: bool,
}



fn back_arg_group() -> ArgGroup {
    ArgGroup::new("focusing")
        .args(&["back", "back_restore"])
        .multiple(false)
}

fn follow_arg_group() -> ArgGroup {
    ArgGroup::new("movement")
        .args(&["follow", "pattern", "pattern_backwards"])
        .multiple(false)
}

fn splits_arg_group() -> ArgGroup {
    ArgGroup::new("splits")
        .args(&[
            "split_left",
            "split_right",
            "split_above",
            "split_below"
        ])
        .args(&[
            "split_left_cols",
            "split_right_cols",
            "split_above_rows",
            "split_below_rows"
        ])
        .multiple(false)
}


pub fn get_options() -> Options {
    Options::parse()
}


#[derive(Debug, Clone)]
pub enum FileOption {
    Uri(String),
    Path(String),
}

impl From<&std::ffi::OsStr> for FileOption {
    fn from(value: &std::ffi::OsStr) -> Self {
        let s = value.to_string_lossy().to_string();
        let mut chars = s.chars();

        loop {
            match chars.next() {
                Some('+' | '-' | '.') => continue,

                Some(c) if c.is_alphanumeric() => continue,

                Some(c) if c == ':' &&
                    matches!(chars.next(), Some('/')) &&
                    matches!(chars.next(), Some('/')) =>

                    return FileOption::Uri(String::from(s)),

                _ => {}
            }

            return FileOption::Path(String::from(s))
        }
    }
}
