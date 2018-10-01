use util::{self, IO};
use cli::Options;
use neovim_lib::{
    neovim_api::{Window, Buffer},
    Value,
    Neovim,
    Session,
    NeovimApi,
};
use std::{
    fs::{self, File},
    process::{self, Command, Stdio},
    io::Read,
    net::SocketAddr,
    iter,
    env,
    path::PathBuf,
};


/// Extends `nvim::Session` to be able to spawn new nvim process.
/// Unlike `nvim::Session::ClientConnection::Child` stdin|stdout of new process will be not inherited.
pub struct ConnectedNeovim {
    pub nvim: Neovim,
    pub nvim_child_process: Option<process::Child>,
    pub initial_position: (Window, Buffer),
}

impl ConnectedNeovim {
    pub(crate) fn connect_parent_or_child(address: &Option<String>, print_protection: bool) -> IO<ConnectedNeovim> {
        let (mut session, nvim_child_process) = Self::connect_session(address, print_protection)?;
        session.start_event_loop();
        let mut nvim = Neovim::new(session);
        let initial_position = (nvim.get_current_win()?, nvim.get_current_buf()?);
        Ok(ConnectedNeovim {
            nvim,
            initial_position,
            nvim_child_process
        })
    }

    fn connect_session(address: &Option<String>, print_protection: bool) -> IO<(Session, Option<process::Child>)> {
        if let Some(nvim_parent_listen_address) = address.as_ref() {
            let nvim_session = Self::session_from_address(nvim_parent_listen_address)?;
            Ok((nvim_session, None))
        } else {
            if print_protection {
                let mut directory = env::temp_dir();
                directory.push(util::PAGE_TMP_DIR);
                directory.push("DO-NOT-REDIRECT-OUTSIDE-OF-NVIM-TERM(--help[-W])");
                fs::create_dir_all(&directory)?;
                println!("{}", directory.to_string_lossy());
            }
            let (nvim_child_listen_address, nvim_child_process) = ConnectedNeovim::spawn_child_nvim_process()?;
            let nvim_session = Self::session_from_address(&nvim_child_listen_address.to_string_lossy())?;
            Ok((nvim_session, Some(nvim_child_process)))
        }
    }

    fn spawn_child_nvim_process() -> IO<(PathBuf, process::Child)> {
        let mut nvim_child_listen_address = env::temp_dir();
        nvim_child_listen_address.push(util::PAGE_TMP_DIR);
        fs::create_dir_all(&nvim_child_listen_address)?;
        nvim_child_listen_address.push(&format!("socket-{}", util::random_string()));
        let nvim_child_process = Command::new("nvim")
            .stdin(Stdio::null()) // Don't inherit stdin, nvim can't redirect text into terminal buffer from it
            .env("NVIM_LISTEN_ADDRESS", &nvim_child_listen_address)
            .spawn()?;
        util::wait_until_file_created(&nvim_child_listen_address)?;
        Ok((nvim_child_listen_address, nvim_child_process))
    }

    fn session_from_address(nvim_listen_address: impl AsRef<str>) -> IO<Session> {
        let nvim_listen_address = nvim_listen_address.as_ref();
        let session = match nvim_listen_address.parse::<SocketAddr>() {
            Ok(_) => Session::new_tcp(nvim_listen_address)?,
            _ => Session::new_unix_socket(nvim_listen_address)?,
        };
        Ok(session)
    }
}


/// A helper for nvim terminal buffer creation/configuration
pub(crate) struct Manager<'a> {
    nvim: &'a mut Neovim,
}

impl <'a> Manager<'a> {
    pub fn new(nvim: &mut Neovim) -> Manager {
        Manager {
            nvim
        }
    }

    pub fn create_pty_with_buffer(&mut self) -> IO<(Buffer, PathBuf)> {
        let agent_pipe_name = util::random_string();
        self.nvim.command(&format!("term pty-agent {}", agent_pipe_name))?;
        let buffer = self.nvim.get_current_buf()?;
        let pty_path = self.read_pty_device_path(&agent_pipe_name)?;
        trace!(target: "new pty", "{:?} => {:?}", buffer.get_number(self.nvim), pty_path);
        Ok((buffer, pty_path))
    }

    pub fn register_buffer_as_instance(
        &mut self,
        buffer: &Buffer,
        instance_name: &str,
        instance_pty_path: &str
    ) -> IO {
        trace!(target: "register instance buffer", "{:?}->{}->{}", buffer, instance_name, instance_pty_path);
        let value = Value::from(vec![Value::from(instance_name), Value::from(instance_pty_path)]);
        buffer.set_var(self.nvim, "page_instance", value)?;
        Ok(())
    }

    pub fn find_instance_buffer(&mut self, instance_name: &str) -> IO<Option<(Buffer, PathBuf)>> {
        for buffer in self.nvim.list_bufs()? {
            let instance_var = buffer.get_var(self.nvim, "page_instance");
            trace!(target: "find instance", "{:?} => {}: {:?}",
                buffer.get_number(self.nvim),
                instance_name,
                instance_var
            );
            match instance_var {
                Err(e) =>
                    if e.to_string() != "1 - Key 'page_instance' not found" {
                        return Err(e)?
                    }
                Ok(v) =>
                    if let Some(arr) = v.as_array().map(|a|a.iter().map(Value::as_str).collect::<Vec<_>>()) {
                        if let [Some(instance_name_found), Some(instance_pty_path)] = arr[..] {
                            trace!(target: "found instance", "{}->{}", instance_name_found, instance_pty_path);
                            if instance_name == &instance_name_found.to_string() {
                                let pty_path = PathBuf::from(instance_pty_path.to_string());
                                return Ok(Some((buffer, pty_path)))
                            }
                        }
                    }
            }
        };
        Ok(None)
    }

    pub fn close_instance_buffer(&mut self, instance_name: &str) -> IO {
        trace!(target: "close instance buffer", "{}", instance_name);
        let instance_buffer = self.find_instance_buffer(&instance_name)?;
        if let Some((buffer, _)) = instance_buffer {
            let instance_buffer_id = buffer.get_number(self.nvim)?;
            self.nvim.command(&format!("exe 'bd!' . {}", instance_buffer_id))?;
        }
        Ok(())
    }

    pub fn focus_instance_buffer(&mut self, instance_buffer: &Buffer) -> IO {
        trace!(target: "focus instance buffer", "{:?}", instance_buffer);
        if &self.nvim.get_current_buf()? != instance_buffer {
            for window in self.nvim.list_wins()? {
                if &window.get_buf(self.nvim)? == instance_buffer {
                    self.nvim.set_current_win(&window)?;
                    break;
                }
            }
            self.nvim.set_current_buf(instance_buffer)?;
        }
        Ok(())
    }

    pub fn read_pty_device_path(&mut self, agent_pipe_name: &str) -> IO<PathBuf> {
        trace!(target: "read pty path", "{}", agent_pipe_name);
        let agent_pipe_path = util::open_agent_pipe(agent_pipe_name)?;
        let pty_path = {
            let mut pty_path = String::new();
            File::open(&agent_pipe_path)?.read_to_string(&mut pty_path)?;
            PathBuf::from(pty_path)
        };
        if let Err(e) = fs::remove_file(&agent_pipe_path) {
            eprintln!("can't remove agent pipe {:?}: {:?}", &agent_pipe_path, e);
        }
        Ok(pty_path)
    }

    pub fn split_current_buffer(&mut self, opt: &Options) -> IO {
        trace!(target: "split", "");
        if opt.split_right > 0 {
            self.nvim.command("belowright vsplit")?;
            let buf_width = self.nvim.call_function("winwidth", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_width * 3 / (u64::from(opt.split_right) + 1);
            self.nvim.command(&format!("vertical resize {} | set wfw", resize_ratio))?;
        } else if opt.split_left > 0 {
            self.nvim.command("aboveleft vsplit")?;
            let buf_width = self.nvim.call_function("winwidth", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_width * 3 / (u64::from(opt.split_left) + 1);
            self.nvim.command(&format!("vertical resize {} | set wfw", resize_ratio))?;
        } else if opt.split_below > 0 {
            self.nvim.command("belowright split")?;
            let buf_height = self.nvim.call_function("winheight", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_height * 3 / (u64::from(opt.split_below) + 1);
            self.nvim.command(&format!("resize {} | set wfh", resize_ratio))?;
        } else if opt.split_above > 0 {
            self.nvim.command("aboveleft split")?;
            let buf_height = self.nvim.call_function("winheight", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_height * 3 / (u64::from(opt.split_above) + 1);
            self.nvim.command(&format!("resize {} | set wfh", resize_ratio))?;
        } else if let Some(split_right_cols) = opt.split_right_cols {
            self.nvim.command(&format!("belowright vsplit | vertical resize {} | set wfw", split_right_cols))?;
        } else if let Some(split_left_cols) = opt.split_left_cols {
            self.nvim.command(&format!("aboveleft vsplit | vertical resize {} | set wfw", split_left_cols))?;
        } else if let Some(split_below_rows) = opt.split_below_rows {
            self.nvim.command(&format!("belowright split | resize {} | set wfh", split_below_rows))?;
        } else if let Some(split_above_rows) = opt.split_above_rows {
            self.nvim.command(&format!("aboveleft split | resize {} | set wfh", split_above_rows))?;
        }
        Ok(())
    }

    pub fn update_buffer_title(&mut self, buffer: &Buffer, buffer_title: &str) -> IO {
        trace!(target: "set title", "{:?} => {}", buffer.get_number(self.nvim), buffer_title);
        let first_attempt = iter::once((0, buffer_title.to_string()));
        let next_attempts = (1..99).map(|i| (i, format!("{}({})", buffer_title, i)));
        for (attempt_count, name) in first_attempt.chain(next_attempts) {
            match buffer.set_name(self.nvim, &name) {
                Err(e) => {
                    trace!(target: "set title", "{:?} => {}: {:?}", buffer.get_number(self.nvim), buffer_title, e);
                    if attempt_count > 99 || e.to_string() != "0 - Failed to rename buffer" {
                        return Err(e)?
                    }
                }
                Ok(()) => {
                    self.nvim.command("redraw!")?;  // To update statusline
                    return Ok(())
                }
            }
        }
        Err("Can't update buffer title")?
    }

    pub fn update_buffer_filetype(&mut self, buffer: &Buffer, filetype: &str) -> IO {
        trace!(target: "update filetype", "");
        buffer.set_option(self.nvim, "filetype", Value::from(filetype))?;
        Ok(())
    }

    pub fn set_page_default_options_to_current_buffer(&mut self) -> IO {
        trace!(target: "set default options", "");
        self.nvim.command("setl scrollback=-1 scrolloff=999 signcolumn=no nonumber modified nomodifiable")?;
        Ok(())
    }

    pub fn execute_user_command_on_buffer(&mut self, buffer: &Buffer, command: &str) -> IO {
        trace!(target: "exec command", "{}", command);
        let saved_buffer_position = self.get_current_buffer_position()?;
        if buffer != &saved_buffer_position.1 {
            self.nvim.set_current_buf(buffer)?;
        }
        self.nvim.command(command)?;
        let final_buffer_position = self.get_current_buffer_position()?;
        if final_buffer_position != saved_buffer_position {
            self.switch_to_buffer_position(&saved_buffer_position)?;
        }
        Ok(())
    }

    pub fn get_current_buffer_position(&mut self) -> IO<(Window, Buffer)> {
        Ok((self.nvim.get_current_win()?, self.nvim.get_current_buf()?))
    }

    pub fn switch_to_buffer_position(&mut self, (win, buf): &(Window, Buffer)) -> IO {
        trace!(target: "switch buffer", "win:{:?} buf:{:?}",  win.get_number(self.nvim), buf.get_number(self.nvim));
        self.nvim.set_current_win(win)?;
        self.nvim.set_current_buf(buf)?;
        Ok(())
    }

    pub fn set_current_buffer_insert_mode(&mut self) -> IO {
        trace!(target: "set mode: INSERT", "");
        // this fixes "can't enter normal mode from terminal mode"
        self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>A")"###)?;
        Ok(())
    }

    pub fn set_current_buffer_follow_output_mode(&mut self) -> IO {
        trace!(target: "set mode: FOLLOW", "");
        // this fixes "can't enter normal mode from terminal mode"
        self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>G")"###)?;
        Ok(())
    }

    pub fn set_current_buffer_reading_mode(&mut self) -> IO {
        trace!(target: "set mode: SCROLL", "");
        // this fixes "can't enter normal mode from terminal mode"
        self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>ggM")"###)?;
        Ok(())
    }

    pub fn open_file_buffer(&mut self, file: &str) -> IO {
        trace!(target: "open file", "{}", file);
        self.nvim.command(&format!("e {}", fs::canonicalize(file)?.to_string_lossy()))?;
        Ok(())
    }

    pub fn get_var_or_default(&mut self, key: &str, default: &str) -> IO<String> {
        let var = self.nvim.get_var(key).map(|v| v.to_string())
            .or_else(|e| if e.to_string() == format!("1 - Key '{}' not found", key) {
                Ok(String::from(default))
            } else {
                Err(e)
            })?;
        Ok(var)
    }
}
