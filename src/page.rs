#![feature(termination_trait)]
#![feature(attr_literals)]
#![feature(iterator_try_fold)]

#[macro_use]
extern crate structopt;

extern crate neovim_lib;
extern crate rand;

use neovim_lib::{self as nvim, NeovimApi, Value};
use rand::{Rng, thread_rng};
use structopt::{StructOpt, clap::{ ArgGroup, AppSettings::* }};
use std::fs::{self, remove_file, File, OpenOptions};
use std::path::PathBuf;
use std::io::{self, Read, Write};
use std::iter;
use std::process::{self, Command, Stdio};
use std::thread;
use std::time::Duration;
use std::net::{SocketAddr};
use std::error::Error;
use std::os::unix::fs::FileTypeExt;

mod util;


#[derive(StructOpt)]
#[structopt(raw(
    global_settings="&[DisableHelpSubcommand, DeriveDisplayOrder]",
    group=r#"ArgGroup::with_name("splits")
        .args(&["split_left", "split_right", "split_above", "split_below"])
        .args(&["split_left_cols", "split_right_cols", "split_above_rows", "split_below_rows"])
        .multiple(false)"#,
    group=r#"ArgGroup::with_name("instances")
        .args(&["instance", "instance_append"])
        .multiple(false)"#))]
struct Opt {
    /// Neovim session address
    #[structopt(short="s", env="NVIM_LISTEN_ADDRESS")]
    address: Option<String>,

    /// Run command in pager buffer
    #[structopt(short="e")]
    command: Option<String>,

    /// Use named instance buffer instead of opening new
    #[structopt(short="i")]
    instance: Option<String>,

    /// The same as "-i" but with append mode
    #[structopt(short="a")]
    instance_append: Option<String>,

    /// Close named instance buffer
    #[structopt(short="x")]
    instance_close: Option<String>,

    /// Filetype hint, allows color highlighting when reading from stdin
    #[structopt(short="t", default_value="pager")]
    filetype: String,

    /// Stay focused on current buffer
    #[structopt(short="b")]
    back: bool,

    /// Print path to /dev/pty/* associated with pager buffer
    #[structopt(short="p")]
    print_pty_path: bool,

    /// Split right with ratio: window_width  * 3 / (<r provided> + 1)
    #[structopt(short="r", parse(from_occurrences))]
    split_right: u8,

    /// Split left  with ratio: window_width  * 3 / (<l provided> + 1)
    #[structopt(short="l", parse(from_occurrences))]
    split_left: u8,

    /// Split above with ratio: window_height * 3 / (<u provided> + 1)
    #[structopt(short="u", parse(from_occurrences))]
    split_above: u8,

    /// Split below with ratio: window_height * 3 / (<d provided> + 1)
    #[structopt(short="d", parse(from_occurrences))]
    split_below: u8,

    /// Split right and resize to <split_right_cols> columns
    #[structopt(short="R")]
    split_right_cols: Option<u8>,

    /// Split left  and resize to <split_left_cols>  columns
    #[structopt(short="L")]
    split_left_cols: Option<u8>,

    /// Split above and resize to <split_above_rows> rows
    #[structopt(short="U")]
    split_above_rows: Option<u8>,

    /// Split below and resize to <split_below_rows> rows
    #[structopt(short="D")]
    split_below_rows: Option<u8>,

    /// Open these files in separate buffers
    #[structopt(name="FILES")]
    files: Vec<String>
}



/// Extends `nvim::Session` with optional `nvim_process` field.
/// That `nvim_process` might be a spawned on top `nvim` process connected through unix socket.
/// It's the same that `nvim::Session::ClientConnection::Child` but stdin|stdout don't inherited.
struct RunningSession {
    nvim_session: nvim::Session,
    nvim_process: Option<process::Child>
}

impl RunningSession {
    fn connect_to_parent_or_child(nvim_listen_address: Option<&String>) -> io::Result<RunningSession> {
        nvim_listen_address
            .map_or_else(RunningSession::spawn_child,
                         |address| RunningSession::connect_to_parent(address)
                             .or_else(|e| {
                                 eprintln!("can't connect to parent neovim session: {}", e);
                                 RunningSession::spawn_child()
                    }))
    }

    fn spawn_child() -> io::Result<RunningSession> {
        let mut nvim_listen_address = PathBuf::from("/tmp/nvim-page");
        fs::create_dir_all(&nvim_listen_address)?;
        nvim_listen_address.push(&format!("socket-{}", random_string()));
        let nvim_process = Command::new("nvim")
            .stdin(Stdio::null())
            .env("NVIM_LISTEN_ADDRESS", &nvim_listen_address)
            .spawn()?;
        thread::sleep(Duration::from_millis(150)); // Wait until nvim process not connected to socket.
        let nvim_session = nvim::Session::new_unix_socket(&nvim_listen_address)?;
        Ok(RunningSession { nvim_session, nvim_process: Some(nvim_process) })
    }

    fn connect_to_parent(nvim_listen_address: &String) -> io::Result<RunningSession> {
        let nvim_session = nvim_listen_address.parse::<SocketAddr>().ok()
            .map_or_else(||nvim::Session::new_unix_socket(nvim_listen_address),
                         |_|nvim::Session::new_tcp(nvim_listen_address))?;
        Ok(RunningSession { nvim_session, nvim_process: None })
    }
}


/// A helper for neovim terminal buffer creation/setting
struct NvimManager<'a> {
    nvim: &'a mut nvim::Neovim,
    opt: &'a Opt,
}

impl <'a> NvimManager<'a> {

    fn create_pty_with_buffer(&mut self) -> Result<PathBuf, Box<Error>> {
        self.split_current_buffer_if_required()?;
        let agent_pipe_name = random_string();
        self.nvim.command(&format!("term pty-agent {}", agent_pipe_name))?;
        let pty_path = self.read_pty_device_path(&agent_pipe_name)?;
        self.update_current_buffer_options()?;
        Ok(pty_path)
    }

    fn create_pty_with_buffer_instance(&mut self, name: &str) -> Result<PathBuf, Box<Error>> {
        let pty_path = self.create_pty_with_buffer()?;
        self.nvim.command(&format!("\
            let last_page_instance = '{}'
            let g:page_instances[last_page_instance] = [ bufnr('%'), '{}' ]", name, pty_path.to_string_lossy()))?;
        self.update_current_buffer_name(&format!(r"get(g:, 'page_icon_instance', '§') . '{}'", name))?;
        Ok(pty_path)
    }

    fn try_get_pty_instance(&mut self, name: &str) -> Result<PathBuf, Box<Error>> {
        let pty_path_str = self.nvim.command_output(&format!("\
            let g:page_instances = get(g:, 'page_instances', {{}})
            let page_instance = get(g:page_instances, '{}', -99999999)
            if bufexists(page_instance[0])
                 echo page_instance[1]
            else
                throw \"Instance don't exists\"
            endif", name))?;
        Ok(PathBuf::from(pty_path_str))
    }

    fn close_pty_instance(&mut self, instance_name: &str) -> Result<(), Box<Error>> {
        self.nvim.command_output(&format!("\
            let g:page_instances = get(g:, 'page_instances', {{}})
            let page_instance = get(g:page_instances, '{}', -99999999)
            if bufexists(page_instance[0])
                exe 'bd!' . page_instance[0]
            endif", instance_name))?;
        Ok(())
    }

    fn read_pty_device_path(&mut self, agent_pipe_name: &str) -> Result<PathBuf, Box<Error>> {
        let agent_pipe_path = util::open_agent_pipe(agent_pipe_name)?;
        let mut agent_pipe = File::open(&agent_pipe_path)?;
        let pty_path = {
            let mut pty_path = String::new();
            agent_pipe.read_to_string(&mut pty_path)?;
            PathBuf::from(pty_path)
        };
        if let Err(e) = remove_file(&agent_pipe_path) {
            eprintln!("can't remove agent pipe {:?}: {:?}", &agent_pipe_path, e);
        }
        Ok(pty_path)
    }

    fn split_current_buffer_if_required(&mut self) -> Result<(), Box<Error>> {
        if self.opt.split_right > 0 {
            self.nvim.command("belowright vsplit")?;
            let buf_width = self.nvim.call_function("winwidth", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_width * 3 / (self.opt.split_right as u64 + 1);
            self.nvim.command(&format!("vertical resize {}", resize_ratio))?;
        } else if self.opt.split_left > 0 {
            self.nvim.command("aboveleft vsplit")?;
            let buf_width = self.nvim.call_function("winwidth", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_width * 3 / (self.opt.split_left as u64 + 1);
            self.nvim.command(&format!("vertical resize {}", resize_ratio))?;
        } else if self.opt.split_below > 0 {
            self.nvim.command("belowright split")?;
            let buf_height = self.nvim.call_function("winheight", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_height * 3 / (self.opt.split_below as u64 + 1);
            self.nvim.command(&format!("resize {}", resize_ratio))?;
        } else if self.opt.split_above > 0 {
            self.nvim.command("aboveleft split")?;
            let buf_height = self.nvim.call_function("winheight", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_height * 3 / (self.opt.split_above as u64 + 1);
            self.nvim.command(&format!("resize {}", resize_ratio))?;
        } else if let Some(split_right_cols) = self.opt.split_right_cols {
            self.nvim.command(&format!("belowright vsplit | vertical resize {}", split_right_cols))?;
        } else if let Some(split_left_cols) = self.opt.split_left_cols {
            self.nvim.command(&format!("aboveleft vsplit | vertical resize {}", split_left_cols))?;
        } else if let Some(split_below_rows) = self.opt.split_below_rows {
            self.nvim.command(&format!("belowright split | resize {}", split_below_rows))?;
        } else if let Some(split_above_rows) = self.opt.split_above_rows {
            self.nvim.command(&format!("aboveleft split | resize {}", split_above_rows))?;
        }
        Ok(())
    }

    fn update_current_buffer_name(&mut self, name: &str) -> Result<(), nvim::CallError> {
        let buf_exists = "Vim(file):E95: Buffer with this name already exists";
        iter::once((0,                 format!("exe 'file ' . {}",          name    ))) // first name without number
            .chain((1..99).map(|i| (i, format!("exe 'file ' . {} . '({})'", name, i))))
            .try_for_each(|(attempt, cmd)| match self.nvim.command(&cmd) {
                Err(err) => if attempt < 99 && err.description() == buf_exists { Ok(()) } else { Err(Err(err)) },
                Ok(succ) => Err(Ok(succ)),
            }).or_else(|break_status| break_status)
    }

    fn update_current_buffer_options(&mut self) -> Result<(), Box<Error>> {
        self.nvim.command("\
            setl scrollback=-1
            setl scrolloff=999
            setl signcolumn=no
            setl nonumber
            setl modifiable
            norm M")?;
        if let Some(user_command) = self.opt.command.as_ref() {
            let saved_position = self.get_current_buffer_position()?;
            self.nvim.command(user_command)?;
            if saved_position != self.get_current_buffer_position()? { // user command can switch buffer
                self.switch_to_buffer_position(saved_position)?;
            }
        }
        Ok(())
    }

    fn update_current_buffer_filetype(&mut self) -> Result<(), Box<Error>> {
        self.nvim.command(&format!("setl filetype={}", self.opt.filetype))?;
        Ok(())
    }

    fn get_current_buffer_position(&mut self) -> Result<(u64, u64), Box<Error>> {
        let winnr = self.nvim.call_function("winnr", vec![])?.as_u64().unwrap();
        let bufnr = self.nvim.call_function("bufnr", vec![nvim::Value::from("%")])?.as_u64().unwrap();
        Ok((winnr, bufnr))
    }

    fn switch_to_buffer_position(&mut self, (winnr, bufnr): (u64, u64)) -> Result<(), Box<Error>> {
        self.nvim.command(&format!("{}wincmd w | {}b", winnr, bufnr))?;
        Ok(())
    }

    fn open_file_buffer(&mut self, file: &String) -> Result<(), Box<Error>> {
        let file_path = fs::canonicalize(file)?;
        self.nvim.command(&format!("e {} | ", file_path.to_string_lossy()))?;
        self.update_current_buffer_options()
    }
}


enum Page<'a> {
    Instance {
        name: &'a str
    },
    ShowFiles {
        paths: &'a Vec<String>
    },
    Regular {
        is_reading_from_fifo: bool
    },
}

impl <'a> Page<'a> {
    fn run(self, nvim_manager: &mut NvimManager, stay_on_current_buffer: bool) -> Result<PathBuf, Box<Error>> {
        let saved_buffer_position = if stay_on_current_buffer {
            Some(nvim_manager.get_current_buffer_position()?)
        } else {
            None
        };
        let pty_path = match self {
            Page::Regular { is_reading_from_fifo } => {
                let pty_path = nvim_manager.create_pty_with_buffer()?;
                let pty_buffer_name = if is_reading_from_fifo {
                    r"get(g:, 'page_icon_pipe', '\\|§')"
                } else {
                    r"get(g:, 'page_icon_redirect', '>§')"
                };
                nvim_manager.update_current_buffer_name(pty_buffer_name)?;
                nvim_manager.update_current_buffer_filetype()?;
                pty_path
            },
            Page::Instance { name } => {
                nvim_manager.try_get_pty_instance(&name)
                    .map(PathBuf::from)
                    .or_else(|e| {
                        if e.description() != "Instance don't exists" {
                            eprintln!("Can't connect to '{}': {}", &name, e);
                        }
                        nvim_manager.create_pty_with_buffer_instance(&name)
                    })?
            },
            Page::ShowFiles { paths } => {
                for file in paths {
                    if let Err(e) = nvim_manager.open_file_buffer(file) {
                        eprintln!("Error opening \"{}\": {}", file, e);
                    }
                }
                PathBuf::new() // Null object
            },
        };
        if let Some(saved_position) = saved_buffer_position {
            nvim_manager.switch_to_buffer_position(saved_position)?;
        }
        Ok(pty_path)
    }
}



fn random_string() -> String {
    thread_rng().gen_ascii_chars().take(32).collect::<String>()
}

fn is_reading_from_fifo() -> bool {
    PathBuf::from("/dev/stdin").metadata() // Probably always returns Err when `page` reads from pipe.
        .map(|stdin_metadata| stdin_metadata.file_type().is_fifo()) // Just to be sure.
        .unwrap_or(true)
}

fn close_child_nvim_if_spawned(spawned_nvim_process: Option<process::Child>) -> io::Result<()> {
    if let Some(mut nvim_process) = spawned_nvim_process {
        nvim_process.wait().map(|_| ())?;
    }
    Ok(())
}

fn map_io_err<E: AsRef<Error>>(msg: &str, e: E) -> io::Error {
    io::Error::new(io::ErrorKind::Other, format!("{}: {}", msg, e.as_ref()))
}


fn main() -> io::Result<()> {
    let opt = Opt::from_args();
    let use_instance = opt.instance.as_ref().or(opt.instance_append.as_ref());

    let is_reading_from_fifo = is_reading_from_fifo();
    let is_any_file_present = !opt.files.is_empty();

    let RunningSession { mut nvim_session, nvim_process } = RunningSession::connect_to_parent_or_child(opt.address.as_ref())?;
    nvim_session.start_event_loop();

    let mut nvim = nvim::Neovim::new(nvim_session);
    let mut nvim_manager = NvimManager { opt: &opt, nvim: &mut nvim };


    if let Some(instance_name) = opt.instance_close.as_ref() {
        if opt.address.is_some() {
            nvim_manager.close_pty_instance(&instance_name)
                .map_err(|e| map_io_err(&format!("Error when closing \"{}\"", instance_name), e))?;
        } else {
            eprintln!("Can't close instance on newly spawned nvim process")
        }
        let exit = !(is_reading_from_fifo || use_instance.is_some() || is_any_file_present);
        if exit {
            return Ok(());
        }
    }

    if is_any_file_present {
        let exit = !(is_reading_from_fifo || use_instance.is_some());
        let single_file = opt.files.len() == 1;
        if exit && single_file {
            nvim_manager.split_current_buffer_if_required()
                .map_err(|e| map_io_err("Can't apply split: {}", e))?;
        }
        let stay_on_current_buffer = opt.back || !exit;
        Page::ShowFiles { paths: &opt.files }
            .run(&mut nvim_manager, stay_on_current_buffer)
            .map_err(|e| map_io_err("Error when reading files: {}", e))?;
        if exit {
            return close_child_nvim_if_spawned(nvim_process);
        }
    }

    let pty_path = use_instance
        .map_or_else(||Page::Regular { is_reading_from_fifo }, |name| Page::Instance { name })
        .run(&mut nvim_manager, opt.back)
        .map_err(|e| map_io_err("Can't connect to PTY: {}", e))?;

    if is_reading_from_fifo {
        let mut pty_device = OpenOptions::new().append(true).open(&pty_path)?;
        if !opt.instance_append.is_some() {
            write!(&mut pty_device, "\x1B[2J\x1B[1;1H")?; // Clear screen
        }
        let stdin = io::stdin();
        io::copy(&mut stdin.lock(), &mut pty_device).map(|_| ())?;
    }

    if opt.print_pty_path || !is_reading_from_fifo {
        println!("{}", pty_path.to_string_lossy());
    }

    close_child_nvim_if_spawned(nvim_process)
}
