extern crate libc;
extern crate notify;
extern crate rand;


use std::{
    path::PathBuf,
    env,
    fs,
    ffi::CString,
    error::Error,
    time::Duration,
    os::unix::ffi::OsStrExt,
};



/// A typealias to clarify signatures a bit.
/// Used only when Input/Output is involved
pub(crate) type IO<T = ()> = Result<T, Box<Error>>;


pub(crate) fn open_agent_pipe(nvim_agent_pipe_name: &str) -> IO<PathBuf> {
    let mut nvim_agent_pipe_path = get_page_tmp_dir()?;
    nvim_agent_pipe_path.push(nvim_agent_pipe_name);
    let nvim_agent_pipe_path_c = CString::new(nvim_agent_pipe_path.as_os_str().as_bytes())?;
    unsafe {
        libc::mkfifo(nvim_agent_pipe_path_c.as_ptr(), 0o600);
    }
    Ok(nvim_agent_pipe_path)
}

pub(crate) fn wait_until_file_created(file_path: &PathBuf) -> IO {
    use self::notify::{Watcher, RecursiveMode, RawEvent, op};
    let (tx, rx) = ::std::sync::mpsc::channel();
    let mut watcher = notify::raw_watcher(tx)?;
    let file_dir = file_path.parent().expect("invalid file path");
    watcher.watch(&file_dir, RecursiveMode::NonRecursive)?;
    if !file_path.exists() {
        loop {
            match rx.recv_timeout(Duration::from_secs(2))? {
                RawEvent { path: Some(ref p), op: Ok(op::CREATE), .. } if p == file_path => break,
                _ => continue,
            }
        }
    }
    watcher.unwatch(file_dir)?;
    Ok(())
}

pub(crate) fn get_page_tmp_dir() -> IO<PathBuf> {
    let mut page_tmp_dir = env::temp_dir();
    page_tmp_dir.push("neovim-page");
    fs::create_dir_all(&page_tmp_dir)?;
    Ok(page_tmp_dir)
}

pub(crate) fn random_string() -> String {
    use self::rand::{Rng, distributions::Alphanumeric};
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .collect()
}

