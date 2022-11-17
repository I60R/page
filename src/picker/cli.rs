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
    /// Open non-text files including directories, binaries, images etc.
    #[clap(display_order=1, short='o')]
    pub open_non_text: bool,

    /// If <FILE> is a directory then open all text files in it
    /// by treating them as provided by [FILE] arguments
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


    /// Only include [FILE].. modified after specified <DATE>
    /// (written in chrono_english format e.g. `week ago`, `yesterday`, etc.)
    #[clap(display_order=12, short='m')]
    pub modified: Option<String>,

    /// Exclude [FILE].. modified after specified <DATE>
    #[clap(display_order=13, short='M')]
    pub modified_exclude: Option<String>,


    /// Only include [FILE] by name glob
    #[clap(display_order=14, short='n')]
    pub name_glob: Option<String>,

    /// Exclude [FILE] by name glob
    #[clap(display_order=15, short='N')]
    pub name_glob_exclude: Option<String>,


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

    /// Run lua expr on file buffer after it was created
    #[clap(display_order=107, long="E")]
    pub lua: Option<String>,



    /// Open first file at last line
    #[clap(display_order=50, short='f')]
    pub follow: bool,

    /// Open and search for a specified <PATTERN>;
    /// empty will open at first non-empty line
    #[clap(display_order=51, short='p', value_hint=ValueHint::AnyPath)]
    pub pattern: Option<String>,

    /// Open and search backwars for a specified <PATTERN_BACKWARDS>;
    /// empty will open at last non-empty line
    #[clap(display_order=52, short='P', value_hint=ValueHint::AnyPath)]
    pub pattern_backwards: Option<String>,


    /// Return back to current buffer
    #[clap(display_order=70, short='b')]
    pub back: bool,

    /// Return back to current buffer and enter into INSERT/TERMINAL mode
    #[clap(display_order=71, short='B')]
    pub back_restore: bool,

    /// Keep page process until buffer is closed
    /// (for editing git commit message)
    #[clap(display_order=72, short='k')]
    pub keep: bool,

    /// Keep page process until first write occur,
    /// then close buffer
    #[clap(display_order=73, long="K")]
    pub keep_until_write: bool,


    /// Open provided file in a separate buffer
    /// [without other flags revokes implied by default -o or -p option]
    #[clap(name="FILE", value_hint=ValueHint::AnyPath)]
    pub files: Vec<FileOption>,


    #[clap(flatten)]
    pub buffer: BufferOptions,


    #[clap(skip)]
    file_split_implied: once_cell::unsync::OnceCell<bool>,
}

impl Options {
    pub fn is_file_open_split_implied(&self) -> bool {
        *self.file_split_implied.get_or_init(||
            self.buffer.split.split_left_cols.is_some() ||
            self.buffer.split.split_right_cols.is_some() ||
            self.buffer.split.split_above_rows.is_some() ||
            self.buffer.split.split_below_rows.is_some() ||
            self.buffer.split.split_left > 0u8 ||
            self.buffer.split.split_right > 0u8 ||
            self.buffer.split.split_above > 0u8 ||
            self.buffer.split.split_below > 0u8
        )
    }
}


// Options that are required on output buffer creation
#[derive(Parser, Debug)]
pub struct BufferOptions {
    /// Run command  on file buffer after it was created
    #[clap(display_order=104, short='e')]
    pub command: Option<String>,

    /// Run lua expr on file buffer after it was created
    #[clap(display_order=105, long="e")]
    pub lua: Option<String>,

    /// Set filetype on first file buffer (to enable syntax highlighting)
    /// [log: default; not works with text echoed by -O]
    #[clap(display_order=7, short='t', default_value="log", hide_default_value=true)]
    pub filetype: String,

    #[clap(flatten)]
    pub split: SplitOptions,
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
