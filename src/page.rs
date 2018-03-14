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
use std::collections::hash_map::DefaultHasher;
use std::path::PathBuf;
use std::io::{self, Read, Write};
use std::iter;
use std::process::{self, Command, Stdio};
use std::thread;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use std::net::{SocketAddr};
use std::error::Error;
use std::os::unix::{self, fs::FileTypeExt};

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
/// That `nvim_process` might be a newly spawned `nvim` process connected through unix socket.
/// It's the same that `nvim::Session::ClientConnection::Child` but stdin|stdout don't inherited.
struct SessionDecorator {
    session: nvim::Session,
    nvim_process: Option<process::Child>
}

impl SessionDecorator {

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
            session: nvim::Session::new_unix_socket(&nvim_listen_address)?,
            nvim_process: Some(nvim_process),
        })
    }

    fn parent(nvim_listen_address: &String) -> io::Result<SessionDecorator> {
        let session = nvim_listen_address.parse::<SocketAddr>()
            .map(|_|            nvim::Session::new_tcp(nvim_listen_address))
            .unwrap_or_else(|_| nvim::Session::new_unix_socket(nvim_listen_address))?;
        Ok(SessionDecorator {
            session,
            nvim_process: None
        })
    }
}


/// A helper for neovim terminal buffer creation/setting
struct NvimManager<'a> {
    neovim: &'a mut nvim::Neovim,
    is_reading_from_fifo: bool,
    opt: &'a Opt
}

impl <'a> NvimManager<'a> {

    fn new(opt: &'a Opt, neovim: &'a mut nvim::Neovim, is_reading_from_fifo: bool) -> NvimManager<'a> {
        NvimManager { opt, is_reading_from_fifo, neovim, }
    }

    fn create_pty(&mut self) -> Result<(PathBuf, Option<File>), Box<Error>> {
        let current_buffer_position = if self.opt.back {
            let winnr = self.neovim.call_function("winnr", vec![])?.as_u64().unwrap();
            let bufnr = self.neovim.call_function("bufnr", vec![nvim::Value::from("%")])?.as_u64().unwrap();
            Some((winnr, bufnr))
        } else {
            None
        };
        let agent_pipe_name = self.create_pty_buffer()?;
        let pty_path = self.read_pty_path(&agent_pipe_name)?;
        let buffer_name =
            if let Some(name) = self.opt.instance.as_ref() {
                format!(r"get(g:, 'page_icon_named', '') . '{}'", name)
            } else if self.is_reading_from_fifo {
                r"get(g:, 'page_icon_pipe', '\\|')".to_owned()
            } else {
                r"get(g:, 'page_icon_redirect', '>')".to_owned()
            };
        self.update_pty_buffer_name(&buffer_name)?;
        self.update_pty_buffer_options()?;
        if let Some((winnr, bufnr)) = current_buffer_position {
            self.neovim.command(&format!("{}wincmd w | {}b", winnr, bufnr))?;
        }
        if self.is_reading_from_fifo {
            let pty = OpenOptions::new().append(true).open(&pty_path)?;
            Ok((pty_path, Some(pty)))
        } else {
            Ok((pty_path, None))
        }
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

    fn update_pty_buffer_options(&mut self) -> Result<(), nvim::CallError> {
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
}


/// Contains data related to page
struct Page {
    pty: (PathBuf, Option<File>),
    pty_symlink: Option<PathBuf>,
    nvim_process: Option<process::Child>,
}

impl Page {

    fn new(opt: &Opt, is_read_from_fifo: bool) -> io::Result<Page> {
        opt.instance.as_ref().map_or_else(
            |    | Page::from_session(opt, is_read_from_fifo),
            |name| Page::from_instance(opt, name, is_read_from_fifo)
                .or_else(|e| {
                    eprintln!("can't connect to \"{}\": {}", name, e);
                    Page::from_session(opt, is_read_from_fifo)
                }))
    }

    fn from_session(opt: &Opt, is_read_from_fifo: bool) -> io::Result<Page> {
        let SessionDecorator { mut session, nvim_process } = opt.address.as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::AddrNotAvailable, "no NVIM_LISTEN_ADDRESS"))
            .and_then(SessionDecorator::parent)
            .or_else(|e| {
                eprintln!("can't connect to parent neovim session: {}", e);
                SessionDecorator::child()
            })?;
        session.start_event_loop();
        let neovim = &mut nvim::Neovim::new(session);
        let mut pty_manager = NvimManager::new(opt, neovim, is_read_from_fifo);
        let pty = pty_manager.create_pty()
            .map_err(|e| io::Error::new(io::ErrorKind::NotConnected, format!("can't open page: {}", e)))?;
        Ok(Page {
            pty,
            nvim_process,
            pty_symlink: None
        })
    }

    fn from_instance(opt: &Opt, name: &str, is_read_from_fifo: bool) -> io::Result<Page> {
        let nvim_listen_address = opt.address.as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::AddrNotAvailable, "no nvim_listen_address"))?;
        let nvim_listen_address_hash = {
            let mut hasher = DefaultHasher::default();
            nvim_listen_address.hash(&mut hasher);
            hasher.finish()
        };
        let mut pty_symlink = PathBuf::from("/tmp/nvimpages");
        fs::create_dir_all(&pty_symlink)?;
        pty_symlink.push(format!("{}-{}", nvim_listen_address_hash, name));
        if pty_symlink.exists() && pty_symlink.metadata()?.modified()? < pty_symlink.symlink_metadata()?.modified()? {
            let pty_path = fs::canonicalize(pty_symlink)?;
            let pty = OpenOptions::new().append(true).open(&pty_path)?;
            Ok(Page {
                pty: (pty_path, Some(pty)),
                pty_symlink: None,
                nvim_process: None,
            })
        } else {
            Ok(Page {
                pty_symlink: Some(pty_symlink),
                ..Page::from_session(opt, is_read_from_fifo)?
            })
        }
    }
}


fn random_string() -> String {
    thread_rng().gen_ascii_chars().take(32).collect::<String>()
}

fn is_read_from_fifo() -> bool {
    PathBuf::from("/dev/stdin").metadata() // Probably always fails on pipe.
        .map(|stdin_metadata| stdin_metadata.file_type().is_fifo()) // Just to be sure.
        .unwrap_or(true)
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();
    let is_read_from_fifo = is_read_from_fifo();
    let page = Page::new(&opt, is_read_from_fifo)?;
    let (pty_path, pty) = page.pty;

    if let Some(mut pty) = pty {
        if !opt.append {
            write!(&mut pty, "\x1B[2J\x1B[1;1H")?; // Clear screen
        }
        if is_read_from_fifo {
            let mut stdin = io::stdin();
            io::copy(&mut stdin.lock(), &mut pty).map(|_|())?;
        }
    }
    if !is_read_from_fifo {
        println!("{}", pty_path.to_str().unwrap());
    }
    if let Some(symlink_path) = page.pty_symlink {
        if fs::read_link(&symlink_path).is_ok() { // Check if link exists and is valid
            fs::remove_file(&symlink_path)?;
        }
        unix::fs::symlink(pty_path, symlink_path)?;
    }
    if let Some(mut spawned) = page.nvim_process {
        spawned.wait().map(|_|())?;
    }
    Ok(())
}
