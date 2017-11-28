#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate structopt_derive;
extern crate failure;
extern crate structopt;
extern crate neovim_lib;
extern crate rand;

use failure::{Error};
use neovim_lib::{Neovim, NeovimApi, CallError, Session};
use rand::{Rng, thread_rng};
use structopt::StructOpt;
use std::fs::{remove_file, File, OpenOptions};
use std::path::PathBuf;
use std::io::{self, stdin, Read};
use std::process;
use std::net::SocketAddr;
use std::os::unix::fs::FileTypeExt;

mod util;



#[derive(StructOpt)]
struct Opt {

    #[structopt(short = "c", help = "run other nvim command in pager buffer(s)")]
    command: Option<String>,

    #[structopt(short = "s", help = "nvim session socket address", env = "NVIM_LISTEN_ADDRESS")]
    address: Option<String>,

    #[structopt(short = "t", help = "set filetype for color highlighting", default_value = "pager")]
    filetype: String,

    #[structopt(short = "e", help = "set modifiable")]
    editable: bool,

    #[structopt(short = "n", help = "set nowrap")]
    nowrap: bool,
}

lazy_static! {
    static ref OPT: Opt = Opt::from_args();
}


fn main() {
    let (mut session, is_child) = open_nvim_session();
    session.start_event_loop();
    let mut nvim = Neovim::new(session);
    use_nvim_as_pager(&mut nvim);
    if is_child {
        if let Err(error) = nvim.session.take_dispatch_guard().join() {
            eprintln!("nvim closed unexpectedly: {:?}", error);
        }
    }
}

fn open_nvim_session() -> (Session, bool) {
    if let Some(nvim_listen_address) = OPT.address.clone() {
        if let Ok(_) = nvim_listen_address.parse::<SocketAddr>() {
            match Session::new_tcp(nvim_listen_address.as_str()) {
                Ok(session) => return (session, false),
                Err(error) => eprintln!("can't connect to tcp socket: {}", error),
            }
        } else {
            match Session::new_unix_socket(nvim_listen_address) {
                Ok(session) => return (session, false),
                Err(error) => eprintln!("can't connect to unix socket: {}", error),
            }
        }
    }
    match Session::new_child() {
        Ok(session) => return (session, true),
        Err(error) => {
            eprintln!("can't connect to nvim: {}", error);
            process::exit(3)
        }
    }
}

fn use_nvim_as_pager(mut nvim: &mut Neovim) {
    let is_stdin_from_pipe = is_stdin_from_pipe();
    let pty_buf_name = if is_stdin_from_pipe { r"\|page" } else { ">page" };
    match try_open_new_nvim_pty(&mut nvim, pty_buf_name) {
        Ok((nvim_pty_path, mut nvim_pty)) => {
            if is_stdin_from_pipe {
                if let Err(error) = io::copy(&mut stdin(), &mut nvim_pty) {
                    eprintln!("IO error {:?}", error);
                    process::exit(4);
                }
            } else {
                println!("{}", nvim_pty_path);
            }
        }
        Err(error) => {
            eprintln!("can't open page {:?}", error);
            process::exit(5);
        },
    }
}

fn is_stdin_from_pipe() -> bool {
    let stdin_path = PathBuf::from("/dev/stdin");
    if let Ok(stdin_metadata) = stdin_path.metadata() { // fails when stdin is a pipe
        stdin_metadata.file_type().is_fifo()
    } else {
        true
    }
}

fn try_open_new_nvim_pty(nvim: &mut Neovim, buf_name: &str) -> Result<(String, File), Error> {
    let pty_agent_pipe_id = create_random_string();
    nvim.command(format!("term pty-agent {}", pty_agent_pipe_id).as_str())?;
    let mut pty_buf_name = String::from(buf_name);
    let mut pty_buf_id = 0;
    while let Err(error) = nvim.command(format!("file {}", pty_buf_name).as_str()) {
        if let &CallError::NeovimError(_, ref description) = &error {
            if pty_buf_id < 1000 && description == "Vim(file):E95: Buffer with this name already exists" {
                pty_buf_id += 1;
                pty_buf_name = format!("{}({})", buf_name, pty_buf_id);
                continue;
            }
        }
        return Err(error.into())
    }
    nvim.command(format!("set scrollback=-1").as_str())?;
    nvim.command(format!("set filetype={}", OPT.filetype).as_str())?;
    nvim.command("set signcolumn=no")?;
    nvim.command("set nonumber")?;
    nvim.command("norm M")?;
    if OPT.editable {
        nvim.command("set modifiable")?;
    }
    if OPT.nowrap {
        nvim.command("set nowrap")?;
    }
    if let Some(command) = OPT.command.clone() {
        nvim.command(command.as_str())?;
    }
    let nvim_pty_path = get_nvim_pty_path(&pty_agent_pipe_id)?;
    let mut pty_open_options = OpenOptions::new();
    pty_open_options.append(true);
    let nvim_pty = pty_open_options.open(&nvim_pty_path)?;
    Ok((nvim_pty_path, nvim_pty))
}

fn create_random_string() -> String {
    thread_rng()
        .gen_ascii_chars()
        .take(32)
        .collect::<String>()
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
