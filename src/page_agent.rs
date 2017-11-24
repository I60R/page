extern crate libc;

use std::fs::{read_link, create_dir_all};
use std::fs::OpenOptions;
use std::io::Write;
use std::ffi::CString;
use std::path::PathBuf;
use std::env::args;
use std::thread;


fn main() {
    {
        let stdout_pty_path = get_stdout_pty_path();
        let nvim_agent_pipe_path = get_nvim_agent_pipe_path();
        let mut nvim_agent_pipe_open_options = OpenOptions::new();
        nvim_agent_pipe_open_options.append(true);
        let mut nvim_agent_pipe = nvim_agent_pipe_open_options.open(&nvim_agent_pipe_path).unwrap();
        nvim_agent_pipe.write_all(stdout_pty_path.to_str().unwrap().as_bytes()).unwrap();
        nvim_agent_pipe.flush().unwrap();
    }
    thread::park();
}


fn get_nvim_agent_pipe_path() -> PathBuf {
    let nvim_agent_pipe_name = args().nth(1).expect("single argument expected");
    let mut nvim_agent_pipe_path = PathBuf::from("/tmp");
    nvim_agent_pipe_path.push("nvimpages");
    create_dir_all(nvim_agent_pipe_path.as_path()).unwrap();
    nvim_agent_pipe_path.push(nvim_agent_pipe_name);
    let nvim_agent_pipe_path_c = CString::new(nvim_agent_pipe_path.to_str().unwrap()).unwrap();
    unsafe {
        libc::mkfifo(nvim_agent_pipe_path_c.as_ptr(), 0o600);
    }
    nvim_agent_pipe_path
}

fn get_stdout_pty_path() -> PathBuf {
    let mut stdout_path = read_link("/proc/self/fd/1").unwrap();
    while stdout_path.symlink_metadata().unwrap().file_type().is_symlink() {
        stdout_path = stdout_path.read_link().unwrap();
    }
    stdout_path
}
