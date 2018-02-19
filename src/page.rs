#![feature(test)]
#![feature(try_trait)]
#![feature(iterator_try_fold)]
#![feature(termination_trait)]
#![feature(attr_literals)]
#![feature(generators)]
#![feature(getpid)]

#[macro_use]
extern crate lazy_static;
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
use std::io::{self, Read};
use std::process::{self, Command, Stdio};
use std::thread;
use std::time::Duration;
use std::net::SocketAddr;
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

    #[structopt(short="t", default_value="pager",
    help="set filetype for color highlighting")]
    filetype: String,

    #[structopt(short="e", display_order=1,
        help="set modifiable")]
    editable: bool,

    #[structopt(short="w", display_order=2,
        help="set nowrap")]
    nowrap: bool,

    #[structopt(subcommand)]
    split: Option<Split>,
}

#[derive(StructOpt)]
enum Split {

    #[structopt(name="h", display_order=1, about="hsplit with 6/(ratio + 1)")]
    Horizontal {
        #[structopt(default_value="4")]
        ratio: u8,
    },

    #[structopt(name="v", display_order=2, about="vsplit with 6/(ratio + 1)")]
    Vertical {
        #[structopt(default_value="4")]
        ratio: u8,
    },
}

lazy_static! {

    /// Contains command line arguments
    static ref OPT: Opt = Opt::from_args();

    /// False when STDIN is read from pipe
    static ref FROM_FIFO: bool = {
        PathBuf::from("/dev/stdin").metadata() // Probably always fails on pipe.
            .map(|stdin_metadata| stdin_metadata.file_type().is_fifo()) // Just to be sure.
            .unwrap_or(true)
    };

}



fn open_nvim_ipc_session() -> io::Result<(nvim::Session, Option<process::Child>)> {
    OPT.address.as_ref()
        .ok_or_else(|| io::Error::new(io::ErrorKind::AddrNotAvailable, "no NVIM_LISTEN_ADDRESS"))
        .and_then(|nvim_listen_address|
            nvim_listen_address.parse::<SocketAddr>()
                .map(           |_| nvim::Session::new_tcp(&nvim_listen_address))
                .unwrap_or_else(|_| nvim::Session::new_unix_socket(&nvim_listen_address))
                .map(|session| (session, None)))    // <— return parent Session and no Child nvim process.
        .or_else(|e| {
            eprintln!("can't connect to running nvim instance: {}", e);
            let mut nvim_listen_address = PathBuf::from("/tmp/nvimpages");
            fs::create_dir_all(&nvim_listen_address)?;
            nvim_listen_address.push(&format!("socket-{}", random_string()));
            let child_nvim_process = Command::new("nvim")
                .stdin(Stdio::null())
                .env("NVIM_LISTEN_ADDRESS", &nvim_listen_address)
                .spawn()?;
            thread::sleep(Duration::from_millis(150)); // Wait until nvim connects to socket.
            nvim::Session::new_unix_socket(&nvim_listen_address)
                .map(|session| (session, Some(child_nvim_process))) // <— return new Session and Child nvim process.
        })
}

fn create_new_nvim_pty(neovim: &mut nvim::Neovim) -> Result<Option<File>, Box<Error>> {
    let pty_agent_pipe_id = random_string();
    create_new_nvim_pty_buf(neovim, &pty_agent_pipe_id)?;
    set_nvim_pty_buf_name(neovim)?;
    set_nvim_pty_buf_options(neovim)?;
    let nvim_pty_path = get_nvim_pty_path(&pty_agent_pipe_id)?;
    if *FROM_FIFO {
        Ok(Some(OpenOptions::new().append(true).open(&nvim_pty_path)?)) // <— return PTY to writing from FIFO
    } else {
        println!("{}", nvim_pty_path); // <— print PTY path, user can write to it manually
        Ok(None)
    }
}

fn create_new_nvim_pty_buf(neovim: &mut nvim::Neovim, pty_agent_pipe_id: &String) -> Result<(), nvim::CallError> {
    match OPT.split {
        Some(Split::Horizontal { ratio }) => {
            neovim.command("vsplit")?;
            let buf_width = neovim.call_function("winwidth", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_width * 6 / (ratio as u64 + 1);
            neovim.command(&format!("vertical resize {}", resize_ratio))?
        },
        Some(Split::Vertical { ratio }) => {
            neovim.command("split")?;
            let buf_height = neovim.call_function("winheight", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_height * 6 / (ratio as u64 + 1);
            neovim.command(&format!("resize {}", resize_ratio))?
        }
        None => {}
    }
    neovim.command(&format!("term pty-agent {}", pty_agent_pipe_id))
}


fn set_nvim_pty_buf_name(neovim: &mut nvim::Neovim) -> Result<(), nvim::CallError> {
    let mut attempt = 0;
    let pty_buf_name = if *FROM_FIFO { "\\|PAGE" } else { ">PIPE" };
    while let Err(e) = neovim.command(
        &if attempt == 0 {
            format!("file {}", pty_buf_name)
        } else {
            format!("file {}:{}", attempt, pty_buf_name)
        }) {
        if attempt > 100 || e.description() != "Vim(file):E95: Buffer with this name already exists" {
            return Err(e);
        }
        attempt += 1;
    }
    Ok(())
}

fn set_nvim_pty_buf_options(neovim: &mut nvim::Neovim) -> Result<(), nvim::CallError> {
    neovim.command(&format!("set scrollback=-1"))?;
    neovim.command(&format!("set filetype={}", OPT.filetype))?;
    neovim.command("set signcolumn=no")?;
    neovim.command("set nonumber")?;
    neovim.command("norm M")?;
    if OPT.editable {
        neovim.command("set modifiable")?;
    }
    if OPT.nowrap {
        neovim.command("set nowrap")?;
    }
    if let &Some(ref command) = &OPT.command {
        neovim.command(&command)?;
    }
    Ok(())
}

fn get_nvim_pty_path(page_id: &str) -> io::Result<String> {
    let nvim_agent_pipe_path = util::open_pty_agent_communication_pipe(&page_id)?;
    let mut nvim_agent_pipe = File::open(&nvim_agent_pipe_path)?;
    let mut nvim_pty_path = String::new();
    nvim_agent_pipe.read_to_string(&mut nvim_pty_path)?;
    if let Err(error) = remove_file(&nvim_agent_pipe_path) {
        eprintln!("can't remove pipe {:?}: {:?}", &nvim_agent_pipe_path, error);
    }
    Ok(nvim_pty_path)
}

fn random_string() -> String {
    thread_rng().gen_ascii_chars().take(32).collect::<String>()
}

fn main() -> io::Result<()> {
    open_nvim_ipc_session()
        .and_then(|(mut nvim_session, new_nvim_process)| {
            nvim_session.start_event_loop();
            let neovim = &mut nvim::Neovim::new(nvim_session);
            create_new_nvim_pty(neovim)
                .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, format!("can't create pty: {}", e)))
                .and_then(|created_nvim_pty|
                    if let Some(mut nvim_pty) = created_nvim_pty {
                        io::copy(&mut io::stdin(), &mut nvim_pty).map(|_| ())
                    } else {
                        Ok(())
                    })
                .and_then(|_|
                    if let Some(mut nvim_process) = new_nvim_process {
                        nvim_process.wait().map(|_| ())
                    } else {
                        Ok(())
                    })
        })
}
