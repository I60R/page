#![feature(termination_trait)]
#![feature(attr_literals)]
#![feature(iterator_try_fold)]

#[macro_use]
extern crate structopt;

extern crate neovim_lib;
extern crate rand;

use neovim_lib as nvim;
use neovim_lib::{NeovimApi, Value};
use rand::{Rng, thread_rng};
use structopt::StructOpt;
use structopt::clap::AppSettings;
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
#[structopt(raw(global_settings="&[AppSettings::AllowNegativeNumbers]"))]
struct Opt {
    #[structopt(short="s", env="NVIM_LISTEN_ADDRESS",
        help="nvim session socket address")]
    address: Option<String>,

    #[structopt(short="c",
        help="execute nvim command in pager buffer")]
    command: Option<String>,

    #[structopt(short="i", raw(required_if="\"append\", \"true\""),
        help="connect to this buffer (if active)")]
    instance: Option<String>,

    #[structopt(short="t", default_value="pager",
        help="set filetype for color highlighting")]
    filetype: String,

    #[structopt(short="a",
        help="don't clear instance buffer")]
    append: bool,

    #[structopt(short="e", display_order=1,
        help="set modifiable")]
    editable: bool,

    #[structopt(short="b", display_order=2,
        help="switch back from newly created buffer")]
    back: bool,

    #[structopt(subcommand)]
    split: Option<OptSplit>,
}

#[derive(StructOpt)]
enum OptSplit {
    #[structopt(name="h", display_order=1, about="hsplit with ratio: 3/(h + 1)")]
    Horizontal {
        #[structopt(default_value="1")]
        ratio: u8,
    },

    #[structopt(name="v", display_order=2, about="vsplit with ratio: 3/(h + 1)")]
    Vertical {
        #[structopt(default_value="1")]
        ratio: u8,
    },
}



/// Extends `nvim::Session` with optional `nvim_process` field.
/// That `nvim_process` might be a spawned on top `nvim` process connected through unix socket.
/// It's the same that `nvim::Session::ClientConnection::Child` but stdin|stdout don't inherited.
struct SessionDecorator {
    inner: nvim::Session,
    nvim_process: Option<process::Child>
}

impl SessionDecorator {

    fn new(nvim_listen_address: &Option<String>) -> io::Result<SessionDecorator> {
        nvim_listen_address.as_ref().map_or_else(
            SessionDecorator::child,
            |address| SessionDecorator::parent(address)
                .or_else(|e| {
                    eprintln!("can't connect to parent neovim session: {}", e);
                    SessionDecorator::child()
                }))
    }

    fn child() -> io::Result<SessionDecorator> {
        let mut nvim_listen_address = PathBuf::from("/tmp/nvimpages");
        fs::create_dir_all(&nvim_listen_address)?;
        nvim_listen_address.push(&format!("socket-{}", random_string()));
        let nvim_process = Command::new("nvim")
            .stdin(Stdio::null())
            .env("NVIM_LISTEN_ADDRESS", &nvim_listen_address)
            .spawn()?;
        thread::sleep(Duration::from_millis(150)); // Wait until nvim process not connected to socket.
        Ok(SessionDecorator {
            inner: nvim::Session::new_unix_socket(&nvim_listen_address)?,
            nvim_process: Some(nvim_process),
        })
    }

    fn parent(nvim_listen_address: &String) -> io::Result<SessionDecorator> {
        let session = nvim_listen_address.parse::<SocketAddr>()
            .map(|_|            nvim::Session::new_tcp(nvim_listen_address))
            .unwrap_or_else(|_| nvim::Session::new_unix_socket(nvim_listen_address))?;
        Ok(SessionDecorator {
            inner: session,
            nvim_process: None
        })
    }

    fn start_event_loop(&mut self) {
        self.inner.start_event_loop()
    }
}


/// A helper for neovim terminal buffer creation/setting
struct NvimManager<'a> {
    neovim: &'a mut nvim::Neovim,
    opt: &'a Opt,
}

impl <'a> NvimManager<'a> {

    fn new(opt: &'a Opt, neovim: &'a mut nvim::Neovim) -> NvimManager<'a> {
        NvimManager { opt, neovim }
    }

    fn create_pty(&mut self) -> Result<PathBuf, Box<Error>> {
        let agent_pipe_name = self.create_pty_buffer()?;
        let pty_path = self.read_pty_path(&agent_pipe_name)?;
        self.update_pty_buffer_options()?;
        Ok(pty_path)
    }

    fn create_pty_instance(&mut self, name: &str) -> Result<PathBuf, Box<Error>> {
        let pty_path = self.create_pty()?;
        self.neovim.command(&format!("\
            let last_page_instance = '{}'
            let g:page_instances[last_page_instance] = [ bufnr('%'), '{}' ]", name, pty_path.to_string_lossy()))?;
        self.update_pty_buffer_name(&format!(r"get(g:, 'page_icon_instance', '§') . '{}'", name))?;
        Ok(pty_path)
    }

    fn get_pty_instance(&mut self, instance_name: &str) -> Result<PathBuf, Box<Error>> {
        let pty_path = self.neovim.command_output(&format!("\
            let g:page_instances = get(g:, 'page_instances', {{}})
            let page_instance = get(g:page_instances, '{}', -99999999)
            if bufexists(page_instance[0])
               echo page_instance[1]
            else
               throw \"instance don't exists\"
            endif",
                                                           instance_name))
            .map(PathBuf::from)?;
        Ok(pty_path)
    }

    fn create_pty_buffer(&mut self) -> Result<String, Box<Error>> {
        let agent_pipe_name = random_string();
        match self.opt.split {
            Some(OptSplit::Horizontal { ratio }) => {
                self.neovim.command("vsplit")?;
                let buf_width = self.neovim.call_function("winwidth", vec![Value::from(0)])?.as_u64().unwrap();
                let resize_ratio = buf_width * 3 / (ratio as u64 + 1);
                self.neovim.command(&format!("vertical resize {}", resize_ratio))?
            }
            Some(OptSplit::Vertical { ratio }) => {
                self.neovim.command("split")?;
                let buf_height = self.neovim.call_function("winheight", vec![Value::from(0)])?.as_u64().unwrap();
                let resize_ratio = buf_height * 3 / (ratio as u64 + 1);
                self.neovim.command(&format!("resize {}", resize_ratio))?
            }
            _ => {}
        }
        self.neovim.command(&format!("term pty-agent {}", agent_pipe_name))?;
        Ok(agent_pipe_name)
    }

    fn read_pty_path(&mut self, agent_pipe_name: &str) -> Result<PathBuf, Box<Error>> {
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

    fn update_pty_buffer_name(&mut self, name: &str) -> Result<(), nvim::CallError> {
        iter::once(                (0, format!("exe 'file ' . {}",          name)))
            .chain((1..99).map(|i| (i, format!("exe 'file ' . {} . '({})'", name, i))))
            .try_for_each(|(i, cmd)| match self.neovim.command(&cmd) {
                Err(e) =>
                    if i < 99 && e.description() == "Vim(file):E95: Buffer with this name already exists" {
                        Ok(()) // CONTINUE
                    } else {
                        Err(Some(e)) // BREAK ERROR
                    },
                _ => Err(None), // BREAK SUCCESS
            }).or_else(|status| match status {
                Some(e) => Err(e),
                None => Ok(())
            })
    }

    fn update_pty_buffer_options(&mut self) -> Result<(), Box<Error>> {
        self.neovim.command(&format!("setl scrollback=-1"))?;
        self.neovim.command(&format!("setl filetype={}", self.opt.filetype))?;
        self.neovim.command("setl signcolumn=no")?;
        self.neovim.command("setl nonumber")?;
        self.neovim.command("norm M")?;
        if self.opt.editable {
            self.neovim.command("setl modifiable")?;
        }
        if let Some(command) = self.opt.command.as_ref() {
            self.neovim.command(command)?;
        }
        Ok(())
    }

    fn get_current_buffer_position(&mut self) -> Result<(u64, u64), Box<Error>> {
        let winnr = self.neovim.call_function("winnr", vec![])?.as_u64().unwrap();
        let bufnr = self.neovim.call_function("bufnr", vec![nvim::Value::from("%")])?.as_u64().unwrap();
        Ok((winnr, bufnr))
    }

    fn switch_back(&mut self, (winnr, bufnr): (u64, u64)) -> Result<(), Box<Error>> {
        self.neovim.command(&format!("{}wincmd w | {}b", winnr, bufnr))?;
        Ok(())
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

enum PageType<'a> {
    NamedInstance { name: &'a str },
    Regular { is_reading_from_fifo: bool }
}

fn run(nvim_manager: &mut NvimManager, page_type: PageType, switch_back: bool) -> Result<PathBuf, Box<Error>> {
    let switch_back_buffer_position = if switch_back {
        Some(nvim_manager.get_current_buffer_position()?)
    } else {
        None
    };
    let pty_path = match page_type {
        PageType::Regular { is_reading_from_fifo } => {
            let pty_path = nvim_manager.create_pty()?;
            nvim_manager.update_pty_buffer_name(if is_reading_from_fifo {
                r"get(g:, 'page_icon_pipe', '\\|§')"
            } else {
                r"get(g:, 'page_icon_redirect', '>§')"
            })?;
            pty_path
        },
        PageType::NamedInstance { name } => {
            nvim_manager.get_pty_instance(&name)
                .or_else(|e| {
                    if e.description() != "instance don't exists" {
                        eprintln!("can't connect to '{}': {}", &name, e);
                    }
                    nvim_manager.create_pty_instance(&name)
                })?
        },
    };
    if let Some(buffer_position) = switch_back_buffer_position {
        nvim_manager.switch_back(buffer_position)?;
    }
    Ok(pty_path)
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();
    let is_reading_from_fifo = is_reading_from_fifo();
    let mut session = SessionDecorator::new(&opt.address)?;
    session.start_event_loop();
    let mut nvim = nvim::Neovim::new(session.inner);
    let mut nvim_manager = NvimManager::new(&opt, &mut nvim);
    let page_type = opt.instance.as_ref().map_or_else(|| PageType::Regular { is_reading_from_fifo },
        |name| PageType::NamedInstance { name });

    let pty_path = run(&mut nvim_manager, page_type, opt.back)
        .map_err(|e| io::Error::new(io::ErrorKind::NotConnected, format!("can't connect to PTY: {}", e)))?;

    if is_reading_from_fifo {
        let mut pty = OpenOptions::new().append(true).open(pty_path)?;
        if !opt.append {
            write!(&mut pty, "\x1B[2J\x1B[1;1H")?; // Clear screen
        }
        let stdin = io::stdin();
        io::copy(&mut stdin.lock(), &mut pty).map(|_|())?;
    } else {
        println!("{}", pty_path.to_string_lossy());
    }

    if let Some(mut nvim_process) = session.nvim_process {
        nvim_process.wait().map(|_|())?;
    }
    Ok(())
}
