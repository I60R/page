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
    group = follow_arg_group(),
    group = instance_use_arg_group(),
)]
pub struct Options {
    /// Set title for output buffer (to display it in statusline)
    #[clap(display_order=10, short='n', env="PAGE_BUFFER_NAME")]
    pub name: Option<String>,

    /// TCP/IP socket address or path to named pipe listened
    /// by running host neovim process
    #[clap(display_order=100, short='a', env="NVIM")]
    pub address: Option<String>,

    /// Arguments that will be passed to child neovim process
    /// spawned when <ADDRESS> is missing
    #[clap(display_order=101, short='A', env="NVIM_PAGE_ARGS")]
    pub arguments: Option<String>,

    /// Config that will be used by child neovim process spawned
    /// when <ADDRESS> is missing [file: $XDG_CONFIG_HOME/page/init.vim]
    #[clap(display_order=102, short='c', value_hint=ValueHint::AnyPath)]
    pub config: Option<String>,

    /// Run command  on output buffer after it was created
    /// or connected as instance
    #[clap(display_order=106, short='E')]
    pub command_post: Option<String>,

    /// Run lua expr on output buffer after it was created
    /// or connected as instance {n}
    /// ~ ~ ~
    #[clap(display_order=107, long="E")]
    pub lua_post: Option<String>,

    /// Create output buffer with <INSTANCE> tag or use existed
    /// with replacing its content by text from page's stdin
    #[clap(display_order=200, short='i')]
    pub instance: Option<String>,

    /// Create output buffer with <INSTANCE_APPEND> tag or use existed
    /// with appending to its content text from page's stdin
    #[clap(display_order=201, short='I')]
    pub instance_append: Option<String>,

    /// Close  output buffer with <INSTANCE_CLOSE> tag if it exists
    /// [without other flags revokes implied by defalt -o or -p option] {n}
    /// ~ ~ ~
    #[clap(display_order=202, short='x')]
    pub instance_close: Option<String>,

    /// Create and use output buffer (to redirect text from page's stdin)
    /// [implied by default unless -x and/or <FILE> provided without
    /// other flags]
    #[clap(display_order=0, short='o')]
    pub output_open: bool,

    /// Print path of pty device associated with output buffer (to redirect
    /// text from commands respecting output buffer size and preserving colors)
    /// [implied if page isn't piped unless -x and/or <FILE> provided without other flags]
    #[clap(display_order=2, short='p')]
    pub pty_path_print: bool,

    /// Cursor follows content of output buffer as it appears
    /// instead of keeping top position (like `tail -f`)
    #[clap(display_order=5, short='f')]
    pub follow: bool,

    /// Cursor follows content of output and <FILE> buffers
    /// as it appears instead of keeping top position
    #[clap(display_order=6, short='F')]
    pub follow_all: bool,

    /// Return back to current buffer
    #[clap(display_order=8, short='b')]
    pub back: bool,

    /// Return back to current buffer and enter into INSERT/TERMINAL mode
    #[clap(display_order=9, short='B')]
    pub back_restore: bool,

    /// Enable PageConnect PageDisconnect autocommands
    #[clap(display_order=103, short='C')]
    pub command_auto: bool,

    /// Flush redirection protection that prevents from producing junk
    /// and possible overwriting of existed files by invoking commands like
    /// `ls > $(NVIM= page -E q)` where the RHS of > operator
    /// evaluates not into /path/to/pty as expected but into a bunch
    /// of whitespace-separated strings/escape sequences from neovim UI;
    /// bad things happens when some shells interpret this as many valid
    /// targets for text redirection. The protection is only printing of a path
    /// to the existed dummy directory always first before printing
    /// of a neovim UI might occur; this makes the first target for text
    /// redirection from page's output invalid and disrupts the
    /// whole redirection early before other harmful writes might occur.
    /// [env: PAGE_REDIRECTION_PROTECT; (0 to disable)] {n}
    ///  ~ ~ ~
    #[clap(display_order=800, short='W')]
    pub page_no_protect: bool,

    /// Open provided file in a separate buffer
    /// [without other flags revokes implied by default -o or -p option]
    #[clap(name="FILE", value_hint=ValueHint::AnyPath)]
    pub files: Vec<FileOption>,


    #[clap(flatten)]
    pub output: OutputOptions,


    #[clap(skip)]
    output_implied: once_cell::unsync::OnceCell<bool>,

    #[clap(skip)]
    output_split_implied: once_cell::unsync::OnceCell<bool>,
}

impl Options {
    pub fn is_output_implied(&self) -> bool {
        *self.output_implied.get_or_init(||
            self.back ||
            self.back_restore ||
            self.follow ||
            self.follow_all ||
            self.output_open ||
            self.pty_path_print ||
            self.instance.is_some() ||
            self.instance_append.is_some() ||
            self.command_post.is_some() ||
            self.lua_post.is_some() ||
            self.output.command.is_some() ||
            self.output.lua.is_some() ||
            self.output.pwd ||
            self.output.filetype != "pager"
        )
    }


    pub fn is_output_split_implied(&self) -> bool {
        *self.output_split_implied.get_or_init(||
            self.output.split.split_left_cols.is_some() ||
            self.output.split.split_right_cols.is_some() ||
            self.output.split.split_above_rows.is_some() ||
            self.output.split.split_below_rows.is_some() ||
            self.output.split.split_left > 0u8 ||
            self.output.split.split_right > 0u8 ||
            self.output.split.split_above > 0u8 ||
            self.output.split.split_below > 0u8
        )
    }
}


// Options that are required on output buffer creation
#[derive(Parser, Debug)]
pub struct OutputOptions {
    /// Run command  on output buffer after it was created
    #[clap(display_order=104, short='e')]
    pub command: Option<String>,

    /// Run lua expr on output buffer after it was created
    #[clap(display_order=105, long="e")]
    pub lua: Option<String>,

    /// Prefetch <NOOPEN_LINES> from page's stdin: if all input fits
    /// then print it to stdout and exit without neovim usage
    /// (to emulate `less --quit-if-one-screen`)
    /// [empty: term height - 3 (space for prompt);
    /// negative: term height - <NOOPEN_LINES>;
    /// 0: disabled and default;
    /// ignored with -o, -p, -x and when page isn't piped]
    #[clap(display_order=1, short='O')]
    pub noopen_lines: Option<Option<isize>>,

    /// Read no more than <QUERY_LINES> from page's stdin:
    /// next lines should be fetched by invoking
    /// :Page <QUERY> command or 'r'/'R' keypress on neovim side
    /// [empty: term height - 2 (space for tab and buffer lines);
    /// negative: term height - <QUERY_LINES>;
    /// 0: disabled and default;
    /// <QUERY> is optional and defaults to <QUERY_LINES>;
    /// doesn't take effect on <FILE> buffers]
    #[clap(display_order=4, short='q')]
    pub query_lines: Option<Option<isize>>,

    /// Set filetype on output buffer (to enable syntax highlighting)
    /// [pager: default; not works with text echoed by -O]
    #[clap(display_order=7, short='t', default_value="pager", hide_default_value=true)]
    pub filetype: String,

    /// Do not remap i, I, a, A, u, d, x, q (and r, R with -q) keys
    /// [wouldn't unmap on connected instance output buffer] {n}
    /// ~ ~ ~
    #[clap(display_order=11, short='w')]
    pub writable: bool,

    /// Set $PWD as working directory at output buffer
    /// (to navigate paths with `gf`)
    #[clap(display_order=3, short='P')]
    pub pwd: bool,


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


fn instance_use_arg_group() -> ArgGroup {
    ArgGroup::new("instances")
        .args(&["instance", "instance_append"])
        .multiple(false)
}

fn back_arg_group() -> ArgGroup {
    ArgGroup::new("focusing")
        .args(&["back", "back_restore"])
        .multiple(false)
}

fn follow_arg_group() -> ArgGroup {
    ArgGroup::new("following")
        .args(&["follow", "follow_all"])
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
