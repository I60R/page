use crate::{
    cli::Options,
    common::{self, IO},
    nvim::listen::{PageCommand, ResponseReceiver},
};

use neovim_lib::{
    neovim_api::{Window, Buffer},
    Value,
    Neovim,
    NeovimApi,
};
use std::{
    fs::{self, File},
    sync::mpsc::{Receiver, sync_channel},
    io::Read,
    iter,
    path::PathBuf,
};
use log::{
    trace,
    warn,
};


/// A facade for neovim, provides common actions 
pub struct NeovimActions {
    nvim: Neovim,
}

impl NeovimActions {
    pub fn get_current_window_and_buffer(&mut self) -> IO<(Window, Buffer)> {
        Ok((self.nvim.get_current_win()?, self.nvim.get_current_buf()?))
    }

    pub fn get_current_buffer(&mut self) -> IO<Buffer> {
        Ok(self.nvim.get_current_buf()?)
    }

    pub fn create_output_buffer_with_pty(&mut self) -> IO<(Buffer, PathBuf)> {
        let term_agent_pipe_unique_name = common::util::random_unique_string();
        self.nvim.command(&format!("term page-term-agent {}", term_agent_pipe_unique_name))?;
        let buffer = self.nvim.get_current_buf()?;
        let buffer_pty_path = self.read_output_buffer_pty_path(&term_agent_pipe_unique_name)?;
        trace!(target: "new output buffer", "{:?} => {:?}", buffer.get_number(&mut self.nvim), buffer_pty_path);
        Ok((buffer, buffer_pty_path))
    }

    pub fn register_buffer_as_instance(
        &mut self,
        buffer: &Buffer,
        instance_name: &str,
        instance_sink: &str
    ) -> IO {
        trace!(target: "register instance buffer", "{:?}->{}->{}", buffer, instance_name, instance_sink);
        let value = Value::from(vec![Value::from(instance_name), Value::from(instance_sink)]);
        buffer.set_var(&mut self.nvim, "page_instance", value)?;
        Ok(())
    }

    pub fn find_instance_buffer(&mut self, instance_name: &str) -> IO<Option<(Buffer, PathBuf)>> {
        for buffer in self.nvim.list_bufs()? {
            let instance_var = buffer.get_var(&mut self.nvim, "page_instance");
            trace!(target: "find instance", "{:?} => {}: {:?}",
                buffer.get_number(&mut self.nvim),
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
            let instance_buffer_id = buffer.get_number(&mut self.nvim)?;
            self.nvim.command(&format!("exe 'bd!' . {}", instance_buffer_id))?;
        }
        Ok(())
    }

    pub fn focus_instance_buffer(&mut self, instance_buffer: &Buffer) -> IO {
        trace!(target: "focus instance buffer", "{:?}", instance_buffer);
        if &self.nvim.get_current_buf()? != instance_buffer {
            for window in self.nvim.list_wins()? {
                trace!(target: "focus instance buffer", "check window {:?}", window.get_number(&mut self.nvim));
                if &window.get_buf(&mut self.nvim)? == instance_buffer {
                    trace!(target: "focus instance buffer", "set last window");
                    self.nvim.set_current_win(&window)?;
                    return Ok(())
                }
            }
        } else {
            trace!(target: "focus instance buffer", "not from window");
        }
        self.nvim.set_current_buf(instance_buffer)?;
        Ok(())
    }

    pub fn read_output_buffer_pty_path(&mut self, term_agent_pipe_unique_name: &str) -> IO<PathBuf> {
        trace!(target: "read pty path", "{}", term_agent_pipe_unique_name);
        let term_agent_pipe_path = common::util::open_term_agent_pipe(term_agent_pipe_unique_name)?;
        let buffer_pty_path = {
            let mut buffer_pty_path = String::new();
            File::open(&term_agent_pipe_path)?.read_to_string(&mut buffer_pty_path)?;
            PathBuf::from(buffer_pty_path)
        };
        if let Err(e) = fs::remove_file(&term_agent_pipe_path) {
            warn!(target: "remove agent pipe", "failed {:?}: {:?}", term_agent_pipe_path, e);
        }
        Ok(buffer_pty_path)
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
        trace!(target: "set title", "{:?} => {}", buffer.get_number(&mut self.nvim), buffer_title);
        let first_attempt = iter::once((0, buffer_title.to_string()));
        let next_attempts = (1..99).map(|i| (i, format!("{}({})", buffer_title, i)));
        for (attempt_count, name) in first_attempt.chain(next_attempts) {
            match buffer.set_name(&mut self.nvim, &name) {
                Err(e) => {
                    trace!(target: "set title", "{:?} => {}: {:?}", buffer.get_number(&mut self.nvim), buffer_title, e);
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

    pub fn set_page_options_to_current_buffer(
        &mut self, 
        filetype: &str, 
        command: &str, 
        define_page_command: &str,
        define_page_command_disconnect: &str,
    ) -> IO {
        let options = &format!(
            " let g:page_scrolloff_backup = &scrolloff \
            | setl scrollback=-1 scrolloff=999 signcolumn=no nonumber nomodifiable filetype={filetype} \
            | exe 'autocmd BufEnter <buffer> set scrolloff=999 | {page_command}' \
            | exe 'autocmd BufLeave <buffer> let &scrolloff=g:page_scrolloff_backup' \
            | exe '{page_command}' \
            | exe '{page_command_disconnect}' \
            | exe 'silent doautocmd User PageOpen' \
            | exe '{user_command}'",
            filetype = filetype,
            page_command = define_page_command,
            page_command_disconnect = define_page_command_disconnect,
            user_command = command.replace("'", "''"), // Ecranizes viml literal string,
        );
        trace!(target: "set default options", "{}", &options);
        self.nvim.command(options)?;
        Ok(())
    }

    pub fn execute_connect_autocmd_on_current_buffer(&mut self) -> IO {
        trace!(target: "autocmd PageConnect", "");
        self.nvim.command("silent doautocmd User PageConnect")?;
        Ok(())
    }

    pub fn execute_disconnect_autocmd_on_current_buffer(&mut self) -> IO {
        trace!(target: "autocmd PageDisconnect", "");
        self.nvim.command("silent doautocmd User PageDisconnect")?;
        Ok(())
    }

    pub fn execute_command_post(&mut self, command: &str) -> IO {
        trace!(target: "exec command_post", "{}", command);
        self.nvim.command(command)?;
        Ok(())
    }

    pub fn switch_to_window_and_buffer(&mut self, (win, buf): &(Window, Buffer)) -> IO {
        trace!(target: "switch window and buffer", "win:{:?} buf:{:?}",  win.get_number(&mut self.nvim), buf.get_number(&mut self.nvim));
        if let Err(e) = self.nvim.set_current_win(win) {
            warn!("Can't switch to window: {}", e);
        }
        if let Err(e) = self.nvim.set_current_buf(buf) {
            warn!("Can't switch to buffer: {}", e);
        }
        Ok(())
    }

    pub fn switch_to_buffer(&mut self, buf: &Buffer) -> IO {
        trace!(target: "switch buffer", "buf:{:?}", buf.get_number(&mut self.nvim));
        self.nvim.set_current_buf(buf)?;
        Ok(())
    }

    pub fn set_current_buffer_insert_mode(&mut self) -> IO {
        trace!(target: "set mode: INSERT", "");
        self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>A", 'n')"###)?;// Fixes "can't enter normal mode from..."
        Ok(())
    }

    pub fn set_current_buffer_follow_output_mode(&mut self) -> IO {
        trace!(target: "set mode: FOLLOW", "");
        self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>G, 'n'")"###)?;
        Ok(())
    }

    pub fn set_current_buffer_scroll_mode(&mut self) -> IO {
        trace!(target: "set mode: SCROLL", "");
        self.nvim.command(r###"call feedkeys("\<C-\>\<C-n>ggM, 'n'")"###)?;
        Ok(())
    }

    pub fn open_file_buffer(&mut self, file: &str) -> IO {
        trace!(target: "open file", "{}", file);
        self.nvim.command(&format!("e {}", fs::canonicalize(file)?.to_string_lossy()))?;
        Ok(())
    }

    pub fn notify_query_finished(&mut self, lines_read: u64) -> IO {
        self.nvim.command(&format!("echom '{} lines read'", lines_read))?;
        Ok(())
    }
    
    pub fn notify_page_read(&mut self) -> IO {
        self.nvim.command("echom 'End of input'")?;
        Ok(())
    }

    pub fn subscribe_to_page_commands(&mut self, page_id: &str) -> IO<Receiver<PageCommand>> {
        trace!(target: "wait for next page", "");
        let (sender, receiver) = sync_channel(16);
        self.nvim.session.start_event_loop_handler(ResponseReceiver { sender, page_id: page_id.to_string() });
        self.nvim.subscribe("page_fetch_lines")?;
        self.nvim.subscribe("page_buffer_closed")?;
        Ok(receiver)
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


pub mod listen {
    use std::sync::mpsc::SyncSender;
    use neovim_lib::{Value, Handler};
    use log::{trace, warn};

    pub enum PageCommand {
        FetchPart,
        FetchLines(u64),
        BufferClosed,
    }

    pub(super) struct ResponseReceiver { 
        pub sender: SyncSender<PageCommand>,
        pub page_id: String
    }

    impl Handler for ResponseReceiver {
        fn handle_notify(&mut self, name: &str, args: Vec<Value>) { 
            trace!("Got response: {} => {:?} ", name, args);
            let id_matches = || args.get(0).and_then(|v|v.as_str()).map_or(false, |v| v == self.page_id);
            match name {
                "page_fetch_lines" if id_matches() => {
                    if let Some(lines_count) = args.get(1).and_then(|v|v.as_u64()) {
                        self.sender.send(PageCommand::FetchLines(lines_count)).unwrap();
                    } else {
                        self.sender.send(PageCommand::FetchPart).unwrap();
                    }
                }
                "page_buffer_closed" if id_matches() => { 
                    self.sender.send(PageCommand::BufferClosed).unwrap();
                }
                _ => {
                    warn!(target: "Unknown response", "");
                }
            }
        }
        fn handle_request(&mut self, name: &str, args: Vec<Value>) -> Result<Value, Value> { 
            trace!("got request: {} => {:?} ", name, args);
            warn!(target: "Unknown request", "");
            Ok(Value::from(0))
        }
    }
}


pub mod connection {
    use crate::{
        cli::Options,
        common::{self, IO},
        nvim::NeovimActions,
    };

    use neovim_lib::{Neovim, Session};
    use std::{
        fs,
        process::{self, Command, Stdio},
        net::SocketAddr,
        env,
        path::PathBuf,
    };
    use log::trace;


    /// Connects to parrent neovim session if possible or spawns new child neovim process and connects to it through socket.
    /// Replacement for `neovim_lib::Session::new_child()` which uses --embed and inherits stdin.
    pub fn get_nvim_connection(
        opt: &Options,
        page_tmp_dir: &PathBuf,
        print_protection: bool
    ) -> IO<(NeovimActions, Option<process::Child>)> {
        let (session, nvim_child_process) = create_session(opt, page_tmp_dir, print_protection)?;
        Ok((NeovimActions { nvim: Neovim::new(session) }, nvim_child_process))
    }

    fn create_session(
        opt: &Options,
        page_tmp_dir: &PathBuf,
        print_protection: bool,
    ) -> IO<(Session, Option<process::Child>)> {
        if let Some(nvim_parent_listen_address) = &opt.address {
            let nvim_session = session_from_address(nvim_parent_listen_address)?;
            Ok((nvim_session, None))
        } else {
            if print_protection {
                let mut directory = page_tmp_dir.clone();
                directory.push("DO-NOT-REDIRECT-OUTSIDE-OF-NVIM-TERM(--help[-W])");
                fs::create_dir_all(&directory)?;
                println!("{}", directory.to_string_lossy());
            }
            let (nvim_child_listen_address, nvim_child_process) = spawn_child_nvim_process(opt, page_tmp_dir)?;
            let nvim_session = session_from_address(&nvim_child_listen_address.to_string_lossy())?;
            Ok((nvim_session, Some(nvim_child_process)))
        }
    }

    fn spawn_child_nvim_process(
        opt: &Options, 
        page_tmp_dir: &PathBuf
    ) -> IO<(PathBuf, process::Child)> {
        let nvim_child_listen_address = {
            let mut nvim_child_listen_address = page_tmp_dir.clone();
            nvim_child_listen_address.push(&format!("socket-{}", common::util::random_unique_string()));
            nvim_child_listen_address
        };
        let nvim_args = {
            let mut nvim_args = String::new();
            nvim_args.push_str("--cmd 'set shortmess+=I' ");
            nvim_args.push_str("--listen ");
            nvim_args.push_str(&nvim_child_listen_address.to_string_lossy());
            if let Some(config) = opt.config.clone().or_else(default_config_path) {
                nvim_args.push(' ');
                nvim_args.push_str("-u ");
                nvim_args.push_str(&config);
            }
            if let Some(custom_args) = opt.arguments.as_ref() {
                nvim_args.push(' ');
                nvim_args.push_str(custom_args);
            }
            nvim_args
        };
        trace!(target: "new nvim process", "args: {}", nvim_args);
        let nvim_args_split = shell_words::split(&nvim_args)?;
        let nvim_child_process = Command::new("nvim")
            .args(&nvim_args_split)
            .stdin(Stdio::null()) // Don't inherit stdin, nvim can't redirect text into terminal buffer from it
            .spawn()?;
        common::util::wait_until_file_created(&nvim_child_listen_address)?;
        Ok((nvim_child_listen_address, nvim_child_process))
    }

    fn default_config_path() -> Option<String> {
        env::var("XDG_CONFIG_HOME").ok()
            .and_then(|xdg_config_home| {
                let mut config_path_buf = PathBuf::from(xdg_config_home);
                config_path_buf.push("page/init.vim");
                if config_path_buf.exists() { Some(config_path_buf) } else { None }
            })
            .or_else(|| env::var("HOME").ok()
                .and_then(|home_dir| {
                    let mut config_path_buf = PathBuf::from(home_dir);
                    config_path_buf.push(".config/page/init.vim");
                    if config_path_buf.exists() { Some(config_path_buf) } else { None }
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
