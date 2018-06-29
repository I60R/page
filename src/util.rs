extern crate libc;

use std::path::PathBuf;
use std::fs;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::io;


pub(crate) fn open_agent_pipe(nvim_agent_pipe_name: &str) -> io::Result<PathBuf> {
    let mut nvim_agent_pipe_path = PathBuf::from("/tmp/nvimpages");
    fs::create_dir_all(nvim_agent_pipe_path.as_path())?;
    nvim_agent_pipe_path.push(nvim_agent_pipe_name);
    let nvim_agent_pipe_path_c = CString::new(nvim_agent_pipe_path.as_os_str().as_bytes())?;
    unsafe {
        libc::mkfifo(nvim_agent_pipe_path_c.as_ptr(), 0o600);
    }
    Ok(nvim_agent_pipe_path)
}
