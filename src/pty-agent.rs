use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::env::args;
use std::thread;
use std::error::Error;

mod common;


fn main() -> Result<(), Box<Error>> {
    {
        let stdout_pty_path = fs::canonicalize(PathBuf::from("/dev/stdout"))?;
        if let Some(pty_agent_pipe_id) = args().nth(1) {
            let pty_agent_pipe_path = common::open_agent_pipe(&pty_agent_pipe_id)?;
            let mut nvim_agent_pipe = OpenOptions::new().write(true)
                .open(&pty_agent_pipe_path)?;
            nvim_agent_pipe.write_all(stdout_pty_path.to_string_lossy().as_bytes())?;
            nvim_agent_pipe.flush()?;
        }
    }
    thread::park(); // Prevents :term buffer to close
    Ok(())
}
