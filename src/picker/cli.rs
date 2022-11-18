use clap::{
    Parser,
    ArgGroup,
    ArgAction,
    ValueHint,
};

// Contains arguments provided by command line
#[derive(Parser, Debug)]
#[clap(
    author,
    about,
    disable_help_subcommand = true,
    allow_negative_numbers = true,
    group = splits_arg_group(),
    group = back_arg_group(),
)]
pub struct Options {
    /// Open provided files as editable
    #[clap(name="FILE", value_hint=ValueHint::AnyPath)]
    pub files: Vec<FileOption>,

    /// Open non-text files including directories, binaries, images etc
    #[clap(display_order=1, short='o')]
    pub open_non_text: bool,

    /// Ignoring [FILE]... open all text files in the current directory
    /// and recursively open all text files in its subdirectories
    /// [0: disabled and default;
    /// empty: defaults to 1 and implied if no <RECURSE_DEPTH> provided;
    /// <RECURSE_DEPTH>: also opens in subdirectories at this level of depth] {n}
    /// ~ ~ ~
    #[clap(display_order=2, short='O')]
    pub recurse_depth: Option<Option<usize>>,

    /// Open at most <QUERY_FILES> at once;
    /// open next manually with :Page <QUERY> or <Leader-r|R|q> shortcut
    /// [empty: implies 1; 0: disabled and default;
    /// <QUERY> is optional and defaults to <QUERY_FILES>] {n}
    /// ~ ~ ~
    #[clap(display_order=3, short='q')]
    pub query_files: Option<i32>,

    /// Include [FILE]... modified after specified <DATE>
    /// [written in chrono_english format e.g. `week ago`, `yesterday`, etc.]
    #[clap(display_order=10, short='m')]
    pub modified: Option<String>,

    /// Exclude [FILE]... modified after specified <DATE>
    #[clap(display_order=11, short='M')]
    pub modified_exclude: Option<String>,

    /// Include [FILE]... by name glob
    #[clap(display_order=12, short='n')]
    pub name_glob: Option<String>,

    /// Exclude [FILE]... by name glob {n}
    /// ~ ~ ~
    #[clap(display_order=13, short='N')]
    pub name_glob_exclude: Option<String>,

    /// Open each [FILE]... at last line
    #[clap(display_order=20, short='f')]
    pub follow: bool,

    /// Open and search for a specified <PATTERN>;
    /// empty will open at first non-empty line
    #[clap(display_order=21, short='p')]
    pub pattern: Option<String>,

    /// Open and search backwars for a specified <PATTERN_BACKWARDS>;
    /// empty will open at last non-empty line {n}
    #[clap(display_order=22, short='P')]
    pub pattern_backwards: Option<String>,

    /// Override filetype on each [FILE]... buffer
    /// (to enable custom syntax highlighting) [text: default]
    /// ~ ~ ~
    #[clap(display_order=105, short='t')]
    pub filetype: Option<String>,

    /// Return back to current buffer
    #[clap(display_order=70, short='b')]
    pub back: bool,

    /// Return back to current buffer and enter into INSERT/TERMINAL mode
    #[clap(display_order=71, short='B')]
    pub back_restore: bool,

    /// Keep Page process until buffer is closed
    /// (for editing git commit message)
    #[clap(display_order=72, short='k')]
    pub keep: bool,

    /// Keep Page process until first write occur,
    /// then close buffer {n}
    /// ~ ~ ~
    #[clap(display_order=73, long="K")]
    pub keep_until_write: bool,

    /// TCP/IP socket address or path to named pipe listened
    /// by running host neovim process
    #[clap(display_order=100, short='a', env="NVIM")]
    pub address: Option<String>,

    /// Arguments that will be passed to child neovim process
    /// spawned when <ADDRESS> is missing
    #[clap(display_order=101, short='A', env="NVIM_PAGE_PICKER_ARGS")]
    pub arguments: Option<String>,

    /// Config that will be used by child neovim process spawned
    /// when <ADDRESS> is missing [file: $XDG_CONFIG_HOME/page/init.vim]
    #[clap(display_order=102, short='c', value_hint=ValueHint::AnyPath)]
    pub config: Option<String>,

    /// Enable PageEdit PageEditDone autocommands
    #[clap(display_order=103, short='C')]
    pub command_auto: bool,

    /// Run command  on file buffer after it was created
    #[clap(display_order=106, short='E')]
    pub command: Option<String>,

    /// Run lua expr on file buffer after it was created {n}
    /// ~ ~ ~
    #[clap(display_order=107, long="E")]
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

impl FileOption {
    pub fn as_str(&self) -> &str {
        let (FileOption::Uri(s) | FileOption::Path(s)) = self;
        s
    }
}
