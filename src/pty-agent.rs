mod util;

use crate::util::IO;

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    env::args,
    thread,
};



fn main() -> IO {
    let stdout_sink = fs::canonicalize(PathBuf::from("/dev/stdout"))?;
    if let Some(pty_agent_pipe_id) = args().nth(1) {
        let pty_agent_pipe_path = util::open_agent_pipe(&pty_agent_pipe_id)?;
        let mut nvim_agent_pipe = OpenOptions::new().write(true).open(&pty_agent_pipe_path)?;
        nvim_agent_pipe.write_all(stdout_sink.to_string_lossy().as_bytes())?;
        nvim_agent_pipe.flush()?;
    }
    thread::park(); // Prevents :term buffer to close
    Ok(())
}
