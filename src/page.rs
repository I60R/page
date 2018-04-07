#![feature(termination_trait)]
#![feature(attr_literals)]
#![feature(iterator_try_fold)]

#[macro_use]
extern crate structopt;

extern crate neovim_lib;
extern crate rand;

use neovim_lib::{self as nvim, NeovimApi, Value};
use rand::{Rng, thread_rng};
use structopt::{StructOpt, clap::AppSettings::*};
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
#[structopt(raw(global_settings="&[Propagated,GlobalVersion,DisableHelpSubcommand,DeriveDisplayOrder]"))]
struct Opt {
    /// Neovim session address
    #[structopt(short="s", env="NVIM_LISTEN_ADDRESS")]
    address: Option<String>,

    /// Run command in pager buffer
    #[structopt(short="e")]
    command: Option<String>,

    /// Use named instance buffer instead of opening new
    #[structopt(short="i", conflicts_with="instance_append")]
    instance: Option<String>,

    /// The same as "-i" but with append mode
    #[structopt(short="a", conflicts_with="instance")]
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

    /// Print path of pty buffer
    #[structopt(short="p")]
    print_pty_path: bool,

    /// Split at right with ratio: $width  * 3 / ($r + 1)
    #[structopt(short="r", parse(from_occurrences),
        raw(conflicts_with_all=r#"&["split_below", "split_below_cols", "split_right_cols"]"#))]
    split_right: u8,

    /// Split at below with ratio: $height * 3 / ($d + 1)
    #[structopt(short="d", parse(from_occurrences),
        raw(conflicts_with_all=r#"&["split_right", "split_below_cols", "split_right_cols"]"#))]
    split_below: u8,

    /// Split at below and resize to $D rows
    #[structopt(short="D",
        raw(conflicts_with_all=r#"&["split_right_cols", "split_below", "split_right"]"#))]
    split_below_rows: Option<u8>,

    /// Split at right and resize to $R columns
    #[structopt(short="R",
        raw(conflicts_with_all=r#"&["split_below_cols", "split_below", "split_right"]"#))]
    split_right_cols: Option<u8>,

    /// Open these files in separate buffers
    #[structopt(name="FILES")]
    files: Vec<String>
}



/// Extends `nvim::Session` with optional `nvim_process` field.
/// That `nvim_process` might be a spawned on top `nvim` process connected through unix socket.
/// It's the same that `nvim::Session::ClientConnection::Child` but stdin|stdout don't inherited.
struct RunningSession {
    session: nvim::Session,
    nvim_process: Option<process::Child>
}

impl RunningSession {
    fn connect_to_parent_or_child(nvim_listen_address: Option<&String>) -> io::Result<RunningSession> {
        nvim_listen_address.map_or_else(RunningSession::child,
            |address| RunningSession::parent(address)
                .or_else(|e| {
                    eprintln!("can't connect to parent neovim session: {}", e);
                    RunningSession::child()
                }))
    }

    fn child() -> io::Result<RunningSession> {
        let mut nvim_listen_address = PathBuf::from("/tmp/nvim-page");
        fs::create_dir_all(&nvim_listen_address)?;
        nvim_listen_address.push(&format!("socket-{}", random_string()));
        let nvim_process = Command::new("nvim")
            .stdin(Stdio::null())
            .env("NVIM_LISTEN_ADDRESS", &nvim_listen_address)
            .spawn()?;
        thread::sleep(Duration::from_millis(150)); // Wait until nvim process not connected to socket.
        Ok(RunningSession {
            session: nvim::Session::new_unix_socket(&nvim_listen_address)?,
            nvim_process: Some(nvim_process),
        })
    }

    fn parent(nvim_listen_address: &String) -> io::Result<RunningSession> {
        let session = nvim_listen_address.parse::<SocketAddr>()
            .map(           |_| nvim::Session::new_tcp(nvim_listen_address))
            .unwrap_or_else(|_| nvim::Session::new_unix_socket(nvim_listen_address))?;
        Ok(RunningSession { session, nvim_process: None })
    }
}


/// A helper for neovim terminal buffer creation/setting
struct NvimManager<'a> {
    nvim: &'a mut nvim::Neovim,
    opt: &'a Opt,
}

impl <'a> NvimManager<'a> {
    fn new(opt: &'a Opt, nvim: &'a mut nvim::Neovim) -> NvimManager<'a> {
        NvimManager { opt, nvim }
    }

    fn create_pty_with_buffer(&mut self) -> Result<PathBuf, Box<Error>> {
        let agent_pipe_name = self.create_pty_buffer_with_running_agent()?;
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
                throw \"instance don't exists\"
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

    fn create_pty_buffer_with_running_agent(&mut self) -> Result<String, Box<Error>> {
        let agent_pipe_name = random_string();
        if self.opt.split_right > 0 {
            self.nvim.command("vsplit")?;
            let buf_width = self.nvim.call_function("winwidth", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_width * 3 / (self.opt.split_right as u64 + 1);
            self.nvim.command(&format!("vertical resize {}", resize_ratio))?;
        } else if self.opt.split_below > 0 {
            self.nvim.command("split")?;
            let buf_height = self.nvim.call_function("winheight", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_height * 3 / (self.opt.split_below as u64 + 1);
            self.nvim.command(&format!("resize {}", resize_ratio))?;
        } else if let Some(rows) = self.opt.split_right_cols.as_ref() {
            self.nvim.command("vsplit")?;
            self.nvim.command(&format!("vertical resize {}", rows))?;
        } else if let Some(cols) = self.opt.split_below_rows.as_ref() {
            self.nvim.command("split")?;
            self.nvim.command(&format!("resize {}", cols))?;
        }
        self.nvim.command(&format!("term pty-agent {}", agent_pipe_name))?;
        Ok(agent_pipe_name)
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

    fn update_current_buffer_name(&mut self, name: &str) -> Result<(), nvim::CallError> {
        iter::once((0,                 format!("exe 'file ' . {}",          name    )))
            .chain((1..99).map(|i| (i, format!("exe 'file ' . {} . '({})'", name, i))))
            .try_for_each(|(attempt, cmd)| match self.nvim.command(&cmd) {
                Err(err) => if attempt < 99 && err.description() == "Vim(file):E95: Buffer with this name already exists" { Ok(()) } else { Err(Err(err)) },
                Ok(succ) => Err(Ok(succ)),
            }).or_else(|break_status| break_status)
    }

    fn update_current_buffer_options(&mut self) -> Result<(), Box<Error>> {
        self.nvim.command("\
            setl scrollback=-1
            setl scrolloff=999
            setl signcolumn=no
            setl nonumber
            selt modifiable
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
                        if e.description() != "instance don't exists" {
                            eprintln!("can't connect to '{}': {}", &name, e);
                        }
                        nvim_manager.create_pty_with_buffer_instance(&name)
                    })?
            },
            Page::ShowFiles { paths } => {
                for file in paths {
                    if let Err(e) = nvim_manager.open_file_buffer(file) {
                        eprintln!("error opening \"{}\": {}", file, e);
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


fn main() -> io::Result<()> {
    let opt = Opt::from_args();
    let use_instance = opt.instance.as_ref().or(opt.instance_append.as_ref());

    let is_reading_from_fifo = is_reading_from_fifo();
    let is_files_present = !opt.files.is_empty();

    let RunningSession { mut session, nvim_process } = RunningSession::connect_to_parent_or_child(opt.address.as_ref())?;
    session.start_event_loop();
    let mut nvim = nvim::Neovim::new(session);
    let mut nvim_manager = NvimManager::new(&opt, &mut nvim);

    if let Some(instance_name) = opt.instance_close.as_ref() {
        nvim_manager.close_pty_instance(&instance_name)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Error when closing \"{}\": {}", instance_name, e)))?;
        let close_instance_only = !(is_reading_from_fifo || use_instance.is_some() || is_files_present);
        if close_instance_only {
            return Ok(());
        }
    }

    if is_files_present {
        let show_files_only = !(is_reading_from_fifo || use_instance.is_some());
        let stay_on_current_buffer = opt.back || !show_files_only;
        Page::ShowFiles { paths: &opt.files }
            .run(&mut nvim_manager, stay_on_current_buffer)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Error when reading files: {}", e)))?;
        if show_files_only {
            return Ok(());
        }
    }

    let pty_path = use_instance
        .map_or_else(||Page::Regular { is_reading_from_fifo }, |name| Page::Instance { name })
        .run(&mut nvim_manager, opt.back)
        .map_err(|e| io::Error::new(io::ErrorKind::NotConnected, format!("Can't connect to PTY: {}", e)))?;

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

    if let Some(mut nvim_process) = nvim_process {
        nvim_process.wait().map(|_| ())?;
    }
    Ok(())
}
