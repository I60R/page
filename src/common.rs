use std::error::Error;

/// Used to simplify signatures of functins where input/output is involved
pub(crate) type IO<T = ()> = Result<T, Box<dyn Error>>;


pub(crate) mod util {
    use std::{
        env, 
        ffi::CString, 
        fs, 
        os::unix::ffi::OsStrExt, 
        path::PathBuf, 
        time::Duration,
    };
    use super::IO;

    pub(crate) fn open_term_agent_pipe(term_agent_pipe_name: &str) -> IO<PathBuf> {
        let mut term_agent_pipe_path = get_page_tmp_dir()?;
        term_agent_pipe_path.push(term_agent_pipe_name);
        let nvim_agent_pipe_path_c = CString::new(term_agent_pipe_path.as_os_str().as_bytes())?;
        unsafe {
            libc::mkfifo(nvim_agent_pipe_path_c.as_ptr(), 0o600);
        }
        Ok(term_agent_pipe_path)
    }
    
    pub(crate) fn wait_until_file_created(file_path: &PathBuf) -> IO {
        let file_dir = file_path.parent().expect("invalid file path");
        use notify::Watcher;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut file_dir_watcher = notify::raw_watcher(tx)?;
        file_dir_watcher.watch(&file_dir, notify::RecursiveMode::NonRecursive)?;
        if !file_path.exists() {
            loop {
                match rx.recv_timeout(Duration::from_secs(2))? {
                    notify::RawEvent { path: Some(ref p), op: Ok(notify::op::CREATE), .. } if p == file_path => break,
                    _ => continue,
                }
            }
        }
        file_dir_watcher.unwatch(file_dir)?;
        Ok(())
    }
    
    pub(crate) fn get_page_tmp_dir() -> IO<PathBuf> {
        let mut page_tmp_dir = env::temp_dir();
        page_tmp_dir.push("neovim-page");
        fs::create_dir_all(&page_tmp_dir)?;
        Ok(page_tmp_dir)
    }
    
    pub(crate) fn random_unique_string() -> String {
        use rand::{Rng, distributions::Alphanumeric};
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .collect()
    }
}
