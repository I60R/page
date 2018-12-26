//! This binary runs in :term buffer as shell.
//! It serves two purposes: 
//!     * determines path to PTY device created by its buffer and returns it to `page` through pipe
//!     * blocks its thread which prevents its buffer to close early

mod common;


use crate::common::IO;

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    env::args,
    thread,
};


fn main() -> IO {
    let stdout_sink = fs::canonicalize(PathBuf::from("/dev/stdout"))?;
    if let Some(term_agent_pipe_unique_name) = args().nth(1) {
        let term_agent_pipe_path = common::util::open_term_agent_pipe(&term_agent_pipe_unique_name)?;
        let mut term_agent_feedback_pipe = OpenOptions::new().write(true).open(&term_agent_pipe_path)?;
        term_agent_feedback_pipe.write_all(stdout_sink.to_string_lossy().as_bytes())?;
        term_agent_feedback_pipe.flush()?;
    }
    thread::park(); // Prevents :term buffer to close
    Ok(())
}
