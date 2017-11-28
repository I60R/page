use std::fs::OpenOptions;
use std::fs::canonicalize;
use std::io::Write;
use std::path::PathBuf;
use std::env::args;
use std::thread;

mod util;


fn main() {
    {
        let stdout_pty_path = canonicalize(PathBuf::from("/dev/stdout")).unwrap();
        let pty_agent_pipe_id = args().nth(1).expect("single argument required");
        let pty_agent_pipe_path = util::open_pty_agent_communication_pipe(&pty_agent_pipe_id).unwrap();
        let mut nvim_agent_pipe_open_options = OpenOptions::new();
        nvim_agent_pipe_open_options.write(true);
        let mut nvim_agent_pipe = nvim_agent_pipe_open_options.open(&pty_agent_pipe_path).unwrap();
        nvim_agent_pipe.write_all(stdout_pty_path.to_string_lossy().as_bytes()).unwrap();
        nvim_agent_pipe.flush().unwrap();
    }
    thread::park();
}
