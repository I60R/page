use crate::cli;
use crate::util::{self, IO};

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
use log::trace;


/// Extends `nvim::Session` to be able to spawn new nvim process.
/// Unlike `nvim::Session::ClientConnection::Child` stdin|stdout of new process will be not inherited.
pub struct NeovimData {
    pub nvim: Neovim,
    pub nvim_child_process: Option<process::Child>,
    pub initial_position: (Window, Buffer),
}

impl NeovimData {
    pub(crate) fn connect_parent_or_child(
        opt: &cli::Options,
        page_tmp_dir: &PathBuf,
        print_protection: bool
    ) -> IO<NeovimData> {
        let (mut session, nvim_child_process) = Self::connect_session(opt, page_tmp_dir, print_protection)?;
        session.start_event_loop();
        let mut nvim = Neovim::new(session);
        let initial_position = (nvim.get_current_win()?, nvim.get_current_buf()?);
        Ok(NeovimData {
            nvim,
            initial_position,
            nvim_child_process
        })
    }

    fn connect_session(
        opt: &cli::Options,
        page_tmp_dir: &PathBuf,
        print_protection: bool,
    ) -> IO<(Session, Option<process::Child>)> {
        if let Some(nvim_parent_listen_address) = &opt.address {
            let nvim_session = Self::session_from_address(nvim_parent_listen_address)?;
            Ok((nvim_session, None))
        } else {
            if print_protection {
                let mut directory = page_tmp_dir.clone();
                directory.push("DO-NOT-REDIRECT-OUTSIDE-OF-NVIM-TERM(--help[-W])");
                fs::create_dir_all(&directory)?;
                println!("{}", directory.to_string_lossy());
            }
            let (nvim_child_listen_address, nvim_child_process) = NeovimData::spawn_child_nvim_process(opt, page_tmp_dir)?;
            let nvim_session = Self::session_from_address(&nvim_child_listen_address.to_string_lossy())?;
            Ok((nvim_session, Some(nvim_child_process)))
        }
    }

    fn spawn_child_nvim_process(opt: &cli::Options, page_tmp_dir: &PathBuf) -> IO<(PathBuf, process::Child)> {
        let nvim_child_listen_address = {
            let mut nvim_child_listen_address = page_tmp_dir.clone();
            nvim_child_listen_address.push(&format!("socket-{}", util::random_string()));
            nvim_child_listen_address
        };
        let nvim_args = {
            let mut nvim_args = String::new();
            nvim_args.push_str("--listen ");
            nvim_args.push_str(&nvim_child_listen_address.to_string_lossy());
            if let Some(config) = Self::find_config_path(&opt.config).as_ref() {
                nvim_args.push(' ');
                nvim_args.push_str("-u ");
                nvim_args.push_str(config);
            }
            if let Some(custom_args) = opt.arguments.as_ref() {
                nvim_args.push(' ');
                nvim_args.push_str(custom_args);
            }
            nvim_args
        };
        trace!(target: "new nvim process", "args: {}", nvim_args);
        let nvim_args_separate = nvim_args.split(|c: char| c.is_whitespace()).collect::<Vec<_>>();
        let nvim_child_process = Command::new("nvim")
            .args(&nvim_args_separate)
            .stdin(Stdio::null()) // Don't inherit stdin, nvim can't redirect text into terminal buffer from it
            .spawn()?;
        util::wait_until_file_created(&nvim_child_listen_address)?;
        Ok((nvim_child_listen_address, nvim_child_process))
    }

    fn find_config_path(config_path_opt: &Option<String>) -> Option<String> {
        if config_path_opt.is_some() {
            return config_path_opt.clone()
        }
        env::var("XDG_CONFIG_HOME").ok()
            .and_then(|xdg_config_home| {
                let mut config_path_buf = PathBuf::from(xdg_config_home);
                config_path_buf.push("page/init.vim");
                if config_path_buf.exists() {
                    Some(config_path_buf)
                } else {
                    None
                }
            })
            .or_else(|| env::var("HOME").ok()
                .and_then(|home_dir| {
                    let mut config_path_buf = PathBuf::from(home_dir);
                    config_path_buf.push(".config/page/init.vim");
                    if config_path_buf.exists() {
                        Some(config_path_buf)
                    } else {
                        None
                    }
                }))
            .map(|config_path_buf| config_path_buf.to_string_lossy().to_string())
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
pub(crate) struct NeovimManager<'a> {
    nvim: &'a mut Neovim,
}

impl <'a> NeovimManager<'a> {
    pub fn new(nvim: &mut Neovim) -> NeovimManager {
        NeovimManager {
            nvim
        }
    }

    pub fn create_pty_with_buffer(&mut self) -> IO<(Buffer, PathBuf)> {
        let ipc_file_name = util::random_string();
        self.nvim.command(&format!("term pty-agent {}", ipc_file_name))?;
        let buffer = self.nvim.get_current_buf()?;
        let sink = self.read_pty_device_path(&ipc_file_name)?;
        trace!(target: "new pty", "{:?} => {:?}", buffer.get_number(self.nvim), sink);
        Ok((buffer, sink))
    }

    pub fn register_buffer_as_instance(
        &mut self,
        buffer: &Buffer,
        instance_name: &str,
        instance_sink: &str
    ) -> IO {
        trace!(target: "register instance buffer", "{:?}->{}->{}", buffer, instance_name, instance_sink);
        let value = Value::from(vec![Value::from(instance_name), Value::from(instance_sink)]);
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
                Err(e) => {
                    let description = e.to_string();
                    if description != "1 - Key 'page_instance' not found"
                    && description != "1 - Key not found: page_instance" { // for new nvim version
                        return Err(e)?
                    }
                }
                Ok(v) => {
                    if let Some(arr) = v.as_array().map(|a|a.iter().map(Value::as_str).collect::<Vec<_>>()) {
                        if let [Some(instance_name_found), Some(instance_sink)] = arr[..] {
                            trace!(target: "found instance", "{}->{}", instance_name_found, instance_sink);
                            if instance_name == instance_name_found {
                                let sink = PathBuf::from(instance_sink.to_string());
                                return Ok(Some((buffer, sink)))
                            }
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
                    return Ok(());
                }
            }
            self.nvim.set_current_buf(instance_buffer)?;
        }
        Ok(())
    }

    pub fn read_pty_device_path(&mut self, agent_pipe_name: &str) -> IO<PathBuf> {
        trace!(target: "read pty path", "{}", agent_pipe_name);
        let agent_pipe_path = util::open_agent_pipe(agent_pipe_name)?;
        let sink = {
            let mut sink_path = String::new();
            File::open(&agent_pipe_path)?.read_to_string(&mut sink_path)?;
            PathBuf::from(sink_path)
        };
        if let Err(e) = fs::remove_file(&agent_pipe_path) {
            eprintln!("can't remove agent pipe {:?}: {:?}", &agent_pipe_path, e);
        }
        Ok(sink)
    }

    pub fn split_current_buffer(&mut self, opt: &cli::Options) -> IO {
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
        let buffer_number = buffer.get_number(self.nvim)?;
        self.nvim.command(&format!("{}bufdo set filetype={}", buffer_number, filetype))?;
        Ok(())
    }

    pub fn set_page_default_options_to_current_buffer(
        &mut self,
        filetype: &str,
        command: &str,
    ) -> IO {
        trace!(target: "set default options", "");
        let options = &format!(
            " let g:page_scrolloff_backup = &scrolloff \
            | setl scrollback=-1 scrolloff=999 signcolumn=no nonumber nomodifiable filetype={} \
            | exe 'autocmd BufEnter <buffer> set scrolloff=999' \
            | exe 'autocmd BufLeave <buffer> let &scrolloff=g:page_scrolloff_backup' \
            | exe 'silent doautocmd User PageOpen' \
            | {}",
            filetype,
            command,
        );
        self.nvim.command(options)?;
        Ok(())
    }

    pub fn execute_command_on_buffer(&mut self, buffer: &Buffer, command: &str) -> IO {
        trace!(target: "execute command on buffer", "{}", command);
        let saved_window_and_buffer = self.get_current_window_and_buffer()?;
        if &saved_window_and_buffer.1 != buffer {
            self.nvim.set_current_buf(buffer)?;
        }
        self.nvim.command(command)?;
        if saved_window_and_buffer != self.get_current_window_and_buffer()? {
            self.switch_to_window_and_buffer(&saved_window_and_buffer)?;
        }
        Ok(())
    }

    pub fn get_current_window_and_buffer(&mut self) -> IO<(Window, Buffer)> {
        Ok((self.nvim.get_current_win()?, self.nvim.get_current_buf()?))
    }

    pub fn switch_to_window_and_buffer(&mut self, (win, buf): &(Window, Buffer)) -> IO {
        trace!(target: "switch buffer", "win:{:?} buf:{:?}",  win.get_number(self.nvim), buf.get_number(self.nvim));
        if let Err(e) = self.nvim.set_current_win(win) {
            eprintln!("Can't switch to window: {}", e);
        }
        if let Err(e) = self.nvim.set_current_buf(buf) {
            eprintln!("Can't switch to buffer: {}", e);
        }
        Ok(())
    }

    pub fn set_current_buffer_insert_mode(&mut self) -> IO {
        trace!(target: "set mode: INSERT", "");
        self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>A")"###)?;// fixes "can't enter normal mode from..."
        Ok(())
    }

    pub fn set_current_buffer_follow_output_mode(&mut self) -> IO {
        trace!(target: "set mode: FOLLOW", "");
        self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>G")"###)?;
        Ok(())
    }

    pub fn set_current_buffer_scroll_mode(&mut self) -> IO {
        trace!(target: "set mode: SCROLL", "");
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
            .or_else(|e| {
                let description = e.to_string();
                if description == format!("1 - Key '{}' not found", key)
                || description == format!("1 - Key not found: {}", key) { // for new nvim version
                    Ok(String::from(default))
                } else {
                    Err(e)
                }
            })?;
        Ok(var)
    }
}
