extern crate clap;
extern crate rand;
extern crate libc;
extern crate neovim_lib;

use clap::{Arg, App};
use neovim_lib::{Neovim, NeovimApi, Session};
use rand::{Rng, thread_rng};
use std::fs::{create_dir_all, remove_file, File, OpenOptions};
use std::io::{stdin, copy, Read};
use std::process::{Command, Stdio, exit};
use std::env::var;
use std::path::PathBuf;
use std::os::unix::fs::FileTypeExt;
use std::ffi::CString;


fn main() {
    let matches = App::new("page")
        .version(env!("CARGO_PKG_VERSION"))
        .author("160R <160R@protonmail.com>")
        .about("Pager that utilizes neovim term buffer")
        .arg(Arg::with_name("pager")
            .short("p")
            .help("default if not under neovim")
            .takes_value(true))
        .args(Arg::with_name("expose"))
        .get_matches();

    match var("NVIM_LISTEN_ADDRESS") {
        Ok(socket) => {
            let session = Session::new_unix_socket(socket).unwrap();
            use_neovim_as_pager(session)
        }
        Err(_) => match matches.value_of("pager") {
            Some(pager) => use_another_pager(pager),
            None => {
                let session = Session::new_child().unwrap();
                use_neovim_as_pager(session)
            }
        }
    }
}

fn use_neovim_as_pager(mut session: Session) {
    session.start_event_loop();
    let mut nvim = Neovim::new(session);
    let nvim_agent_pipe_name = thread_rng()
        .gen_ascii_chars()
        .take(16)
        .collect::<String>();
    nvim.command(format!(":term page-agent {}", nvim_agent_pipe_name).as_str()).unwrap();
    nvim.command(format!(":file {}", nvim_agent_pipe_name).as_str()).unwrap();
    let mut nvim_agent_pipe_path = PathBuf::from("/tmp");
    nvim_agent_pipe_path.push("nvimpages");
    create_dir_all(nvim_agent_pipe_path.as_path()).unwrap();
    nvim_agent_pipe_path.push(nvim_agent_pipe_name);
    let nvim_agent_pipe_path_c = CString::new(nvim_agent_pipe_path.to_str().unwrap()).unwrap();
    unsafe {
        libc::mkfifo(nvim_agent_pipe_path_c.as_ptr(), 0o600);
    }
    let mut nvim_pty = get_neovim_pty(nvim_agent_pipe_path);
    let stdin = stdin();
    copy(&mut stdin.lock(), &mut nvim_pty).unwrap();
}

fn get_neovim_pty(nvim_agent_pipe_path: PathBuf) -> File {
    if nvim_agent_pipe_path.metadata().unwrap().file_type().is_fifo() {
        let mut nvim_agent_pipe = File::open(&nvim_agent_pipe_path).unwrap();
        let mut nvim_pty_path = String::new();
        nvim_agent_pipe.read_to_string(&mut nvim_pty_path).unwrap();
        remove_file(&nvim_agent_pipe_path).unwrap();
        let mut pty_open_options = OpenOptions::new();
        pty_open_options.append(true);
        pty_open_options.open(&nvim_pty_path).unwrap()
    } else {
        eprintln!("invalid pipe");
        exit(1);
    }
}

fn use_another_pager(pager: &str) {
    let mut command_line = pager.split_whitespace();
    let command_name = command_line.nth(0).unwrap();
    Command::new(command_name)
        .args(command_line)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
}


