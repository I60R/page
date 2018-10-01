#[macro_use]
extern crate structopt;

#[macro_use]
extern crate log;
extern crate pretty_env_logger as logger;

extern crate atty;
extern crate neovim_lib;


mod cli;
mod util;
mod nvim;

use util::IO;
use cli::SwitchBackMode;
use neovim_lib::neovim_api::Buffer;
use atty::Stream;
use structopt::StructOpt;
use std::{
    io::{self, Write},
    env,
    fs::OpenOptions,
    path::PathBuf,
    thread,
    time::Duration,
};
use notify::{
    Watcher,
    RecursiveMode,
    RawEvent,
    op,
};



/// A typealias to clarify signatures a bit. Used only when Input/Output is involved
type IO<T = ()> = Result<T, Box<Error>>;



/// Extends `nvim::Session` to be able to spawn new nvim process.
/// Unlike `nvim::Session::ClientConnection::Child` stdin|stdout of new process will be not inherited.
struct NvimSessionConnector {
    nvim_session: nvim::Session,
    nvim_child_process: Option<process::Child>
}

impl NvimSessionConnector {
    fn connect_to_parent_or_child(opt: &cli::Opt, read_from_fifo: bool) -> IO<NvimSessionConnector> {
        if let Some(nvim_parent_listen_address) = opt.address.as_ref() {
            let nvim_session = Self::session_from_address(nvim_parent_listen_address)?;
            Ok(NvimSessionConnector {
                nvim_session,
                nvim_child_process: None
            })
        } else {
            if !read_from_fifo && !opt.page_no_protect && std::env::var_os("PAGE_REDIRECTION_PROTECT").map_or(true, |v| &v != "0") {
                println!("/DON'T/REDIRECT(--help[-W])")

            }
            let (nvim_child_listen_address, nvim_child_process) = NvimSessionConnector::spawn_child_nvim_process()?;
            let nvim_session = Self::session_from_address(&nvim_child_listen_address.to_string_lossy())?;
            Ok(NvimSessionConnector {
                nvim_session,
                nvim_child_process: Some(nvim_child_process)
            })
        }
    }

    fn spawn_child_nvim_process() -> IO<(PathBuf, process::Child)> {
        let mut nvim_child_listen_address = env::temp_dir();
        nvim_child_listen_address.push(common::PAGE_TMP_DIR_NAME);
        fs::create_dir_all(&nvim_child_listen_address)?;
        nvim_child_listen_address.push(&format!("socket-{}", random_string()));
        let nvim_child_process = Command::new("nvim")
            .stdin(Stdio::null()) // Don't inherit stdin, nvim can't redirect content into terminal buffer
            .env("NVIM_LISTEN_ADDRESS", &nvim_child_listen_address)
            .spawn()?;
        wait_until_file_created(&nvim_child_listen_address)?;
        Ok((nvim_child_listen_address, nvim_child_process))
    }

    fn session_from_address(nvim_listen_address: impl AsRef<str>) -> io::Result<nvim::Session> {
        let nvim_listen_address = nvim_listen_address.as_ref();
        match nvim_listen_address.parse::<SocketAddr>() {
            Ok(_) => nvim::Session::new_tcp(nvim_listen_address),
            _ => nvim::Session::new_unix_socket(nvim_listen_address)
        }
    }
}


/// A helper for nvim terminal buffer creation/configuration
struct NvimManager<'a> {
    nvim: &'a mut nvim::Neovim,
}

impl <'a> NvimManager<'a> {
    fn create_pty_with_buffer(&mut self) -> IO<(nvim_api::Buffer, PathBuf)> {
        let agent_pipe_name = random_string();
        self.nvim.command(&format!("term pty-agent {}", agent_pipe_name))?;
        let buffer = self.nvim.get_current_buf()?;
        let pty_path = self.read_pty_device_path(&agent_pipe_name)?;
        Ok((buffer, pty_path))
    }

    fn register_buffer_as_instance(&mut self, instance_name: &str, buffer: &nvim_api::Buffer, instance_pty_path: &str) -> IO {
        buffer.set_var(self.nvim, "page_instance", Value::from(vec![Value::from(instance_name), Value::from(instance_pty_path)]))?;
        Ok(())
    }

    fn find_instance_buffer(&mut self, instance_name: &str) -> IO<Option<(nvim_api::Buffer, PathBuf)>> {
        for buffer in self.nvim.list_bufs()? {
            match buffer.get_var(self.nvim, "page_instance") {
                Err(e) =>
                    if e.to_string() != "1 - Key 'page_instance' not found" {
                        return Err(e)?
                    },
                Ok(v) => {
                    if let Some([Value::String(instance_name_found), Value::String(instance_pty_path)]) = v.as_array().map(Vec::as_slice) {
                        if instance_name == instance_name_found.to_string() {
                            let pty_path = PathBuf::from(instance_pty_path.to_string());
                            return Ok(Some((buffer, pty_path)))
                        }
                    }
                }
            }
        };
        Ok(None)
    }

    fn close_pty_instance(&mut self, instance_name: &str) -> IO {
        if let Some((buffer, _)) = self.find_instance_buffer(&instance_name)? {
            let id = buffer.get_number(self.nvim)?;
            self.nvim.command(&format!("exe 'bd!' . {}", id))?;
        }
        Ok(())
    }

    fn read_pty_device_path(&mut self, agent_pipe_name: &str) -> IO<PathBuf> {
        let agent_pipe_path = common::open_agent_pipe(agent_pipe_name)?;
        let pty_path = {
            let mut pty_path = String::new();
            File::open(&agent_pipe_path)?.read_to_string(&mut pty_path)?;
            PathBuf::from(pty_path)
        };
        if let Err(e) = remove_file(&agent_pipe_path) {
            eprintln!("can't remove agent pipe {:?}: {:?}", &agent_pipe_path, e);
        }
        Ok(pty_path)
    }

    fn split_current_buffer_if_required(&mut self, opt: &cli::Opt) -> IO {
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

    fn update_buffer_name(&mut self, buffer: &nvim_api::Buffer, name: &str) -> IO {
        let first_attempt = iter::once((0, name.to_string()));
        let next_attempts = (1..99).map(|i| (i, format!("{}({})", name, i)));
        for (attempt_count, name) in first_attempt.chain(next_attempts) {
            match buffer.set_name(self.nvim, &name) {
                Err(e) => if attempt_count > 99 || e.to_string() != "0 - Failed to rename buffer" { return Err(e)? },
                Ok(()) => {
                    self.nvim.command("redraw!")?;  // To update statusline
                    return Ok(())
                },
            }
        }
        Err("Can't update buffer name")?
    }

    fn update_buffer_filetype(&mut self, buffer: &nvim_api::Buffer, filetype: &str) -> IO {
        buffer.set_option(self.nvim, "filetype", Value::from(filetype))?;
        Ok(())
    }

    fn set_page_default_options_to_current_buffer(&mut self) -> IO {
        self.nvim.command("setl scrollback=-1 scrolloff=999 signcolumn=no nonumber modified nomodifiable")?;
        Ok(())
    }

    fn execute_user_command_on_buffer(&mut self, buffer: &nvim_api::Buffer, command: &str) -> IO {
        let initial_buffer = self.get_current_buffer_position()?;
        self.nvim.set_current_buf(buffer)?;
        self.nvim.command(command)?;
        self.switch_to_buffer_position(&initial_buffer)?;
        Ok(())
    }

    fn get_current_buffer_position(&mut self) -> IO<(nvim_api::Window, nvim_api::Buffer)> {
        Ok((self.nvim.get_current_win()?, self.nvim.get_current_buf()?))
    }

    fn switch_to_buffer_position(&mut self, (win, buf): &(nvim_api::Window, nvim_api::Buffer)) -> IO {
        self.nvim.set_current_win(win)?;
        self.nvim.set_current_buf(buf)?;
        Ok(())
    }

    fn exit_term_insert_mode(&mut self) -> IO {
        self.nvim.command(r###"exe "norm \<C-\>\<C-n>""###)?;
        Ok(()) // feedkeys not works here
    }

    fn set_current_buffer_insert_mode(&mut self) -> IO {
        self.exit_term_insert_mode()?;
        self.nvim.feedkeys("A", "n", false)?;
        Ok(())
    }

    fn set_current_buffer_follow_output_mode(&mut self) -> IO {
        self.exit_term_insert_mode()?;
        self.nvim.feedkeys("G", "n", false)?;
        Ok(())
    }

    fn set_current_buffer_reading_mode(&mut self) -> IO {
        self.exit_term_insert_mode()?;
        self.nvim.feedkeys("ggM", "n", false)?;
        Ok(())
    }

    fn open_file_buffer(&mut self, file: &str) -> IO {
        self.nvim.command(&format!("e {}", fs::canonicalize(file)?.to_string_lossy()))?;
        Ok(())
    }

    fn get_var_or_default(&mut self, key: &str, default: &str) -> IO<String> {
        let var = self.nvim.get_var(key).map(|v| v.to_string())
            .or_else(|e| if e.to_string() == format!("1 - Key '{}' not found", key) {
                Ok(String::from(default))
            } else {
                Err(e)
            })?;
        Ok(var)

    }

fn random_string() -> String {
    thread_rng().sample_iter(&Alphanumeric).take(32).collect()
}


fn is_reading_from_fifo() -> bool {
    PathBuf::from("/dev/stdin").metadata() // Probably always returns Err when `page` reads from pipe.
        .map(|stdin_metadata| stdin_metadata.file_type().is_fifo()) // Just to be sure.
        .unwrap_or(true)
}


// Context in which application is invoked. Contains related read-only data
struct Cx<'a> {
    opt: &'a cli::Opt,
    use_instance: Option<&'a String>,
    nvim_child_process: Option<process::Child>,
    initial_position: (nvim_api::Window, nvim_api::Buffer),
    read_from_fifo: bool,
}



type BufferPtyPath = PathBuf;
type ConnectedBuffer = (Buffer, BufferPtyPath);


// Handles application use cases
struct App<'a> {
    nvim_manager: &'a mut nvim::Manager<'a>,
}

impl<'a> App<'a> {
    fn new(nvim_manager: &'a mut nvim::Manager<'a>) -> App<'a> {
        App {
            nvim_manager
        }
    }

    fn handle_close_instance_pty(&mut self, &cli::Context {
        opt,
        ref nvim_child_process,
        ..
    }: &cli::Context) -> IO {
        if let Some(name) = opt.instance_close.as_ref() {
            if nvim_child_process.is_none() {
                self.nvim_manager.close_pty_instance(name)?;
            } else {
                eprintln!("Can't close instance on newly spawned nvim process");
            }
        }
        Ok(())
    }

    fn handle_open_provided_files(&mut self, &cli::Context {
        opt,
        ref initial_position,
        splits,
        ..
    }: &cli::Context) -> IO {
        if !opt.files.is_empty() {
            for file in &opt.files {
                let buffer_open_result = self.nvim_manager.open_file_buffer(file);
                if let Err(e) = buffer_open_result {
                    eprintln!("Error opening \"{}\": {}", file, e);
                } else {
                    self.nvim_manager.set_page_default_options_to_current_buffer()?;
                    self.nvim_manager.set_current_buffer_reading_mode()?;
                }
            }
            if splits {
                self.nvim_manager.switch_to_buffer_position(&initial_position)?;
            }
        }
        Ok(())
    }


    fn handle_open_pty_buffer(&mut self, &cli::Context {
        opt,
        ref instance_mode,
        piped,
        splits,
        ..
    }: &cli::Context) -> IO<ConnectedBuffer> {
        if let Some(instance_name) = instance_mode.is_any() {
            if let Some(connected_buffer) = self.nvim_manager.find_instance_buffer(&instance_name)? {
                Ok(connected_buffer)
            } else {
                let (buffer, pty_path) = self.open_page_buffer(&opt, splits)?;
                {
                    let pty_path = pty_path.to_string_lossy();
                    self.nvim_manager.register_buffer_as_instance(&buffer, instance_name, &pty_path)?;
                    Self::set_instance_buffer_name(self.nvim_manager, &opt.name, instance_name, &buffer)?;
                }
                Ok((buffer, pty_path))
            }
        } else {
            let (buffer, pty_path) = self.open_page_buffer(&opt, splits)?;
            let (page_icon_key, page_icon_default) = if piped {
                ("page_icon_pipe", "|")
            } else {
                ("page_icon_redirect", ">")
            };
            let mut buffer_title = self.nvim_manager.get_var_or_default(page_icon_key, page_icon_default)?;
            if let Some(ref buffer_name) = opt.name {
                buffer_title.insert_str(0, buffer_name);
            }
            self.nvim_manager.update_buffer_title(&buffer, &buffer_title)?;
            Ok((buffer, pty_path))
        }
    }

    fn set_instance_buffer_name(
        nvim_manager: &mut nvim::Manager,
        buffer_name: &Option<String>,
        instance_name: &String,
        buffer: &Buffer,
    ) -> IO {
        let (page_icon_key, page_icon_default) = ("page_icon_instance", "$");
        let mut buffer_title = nvim_manager.get_var_or_default(page_icon_key, page_icon_default)?;
        buffer_title.insert_str(0, instance_name);
        if let Some(ref buffer_name) = buffer_name {
            if buffer_name != instance_name {
                buffer_title.push_str(buffer_name);
            }
        }
        nvim_manager.update_buffer_title(&buffer, &buffer_title)?;
        Ok(())
    }

    fn open_page_buffer(&mut self,
        opt: &cli::Options,
        splits: bool,
    ) -> IO<(Buffer, PathBuf)> {
        if splits {
            self.nvim_manager.split_current_buffer(opt)?;
        }
        let (buffer, pty_path) = self.nvim_manager.create_pty_with_buffer()?;
        self.nvim_manager.set_page_default_options_to_current_buffer()?;
        self.nvim_manager.update_buffer_filetype(&buffer, &opt.filetype)?;
        Ok((buffer, pty_path))
    }


    fn handle_user_command(&mut self,
        command: &Option<String>,
        buffer: &Buffer,
    ) -> IO {
        if let Some(ref command) = command {
            self.nvim_manager.execute_user_command_on_buffer(buffer, command)?;
        }
        Ok(())
    }

    fn handle_instance_buffer(&mut self, &cli::Context {
        opt,
        ref switch_back_mode,
        ref instance_mode,
        ..
    }: &cli::Context,
        buffer: &Buffer,
        pty_device: &mut Write,
    ) -> IO {
        if let Some(instance_name) = instance_mode.is_any() {
            Self::set_instance_buffer_name(self.nvim_manager, &opt.name, instance_name, buffer)?;
            if instance_mode.is_replace() || switch_back_mode.is_no_switch() {
                self.nvim_manager.focus_instance_buffer(buffer)?;
            }
            if instance_mode.is_replace() {
                write!(pty_device, "\x1B[3J\x1B[H\x1b[2J")?; // Clear screen sequence
            }
        }
        Ok(())
    }

    fn handle_regular_buffer(&mut self, &cli::Context {
            opt,
            ref initial_position,
            ref switch_back_mode,
            ..
        }: &cli::Context,
    ) -> IO {
        if opt.follow {
            self.nvim_manager.set_current_buffer_follow_output_mode()?;
        } else {
            self.nvim_manager.set_current_buffer_reading_mode()?;
        }
        if !switch_back_mode.is_no_switch() {
            thread::sleep(Duration::from_millis(200)); // To prevent sending keys into wrong buffer
            self.nvim_manager.switch_to_buffer_position(&initial_position)?;
            if let SwitchBackMode::Insert = switch_back_mode {
                self.nvim_manager.set_current_buffer_insert_mode()?;
            }
        }
        Ok(())
    }

    fn handle_redirection(&mut self, &cli::Context {
            piped,
            prints,
            ..
        }: &cli::Context,
        pty_device: &mut Write,
        pty_path: BufferPtyPath,
    ) -> IO {
        if piped {
            let stdin = io::stdin();
            io::copy(&mut stdin.lock(), pty_device).map(drop)?;
        }
        if prints {
            println!("{}", pty_path.to_string_lossy());
        }
        Ok(())
    }


    fn handle_user_command_post(&mut self,
        command: &Option<String>,
        buffer: &Buffer,
    ) -> IO {
        if let Some(ref command) = command {
            thread::sleep(Duration::from_millis(50)); // Fixes errors on `MANPAGER="page -E 'syntax on'"`
            self.nvim_manager.execute_user_command_on_buffer(buffer, command)?;
        }
        Ok(())
    }



    fn handle_exit(self, cli::Context {
        nvim_child_process,
        ..
    }: cli::Context) -> IO {
        if let Some(mut nvim_child_process) = nvim_child_process {
            nvim_child_process.wait().map(drop)?;
        };
        Ok(())
    }
}


fn main() -> IO {
    logger::init();

    let opt = cli::Options::from_args();
    info!("options: {:#?}", opt);

    let piped = atty::isnt(Stream::Stdin);
    let prints_protection =
        *& !piped
        && !opt.page_no_protect
        && env::var_os("PAGE_REDIRECTION_PROTECT").map_or(true, |val| &val != "0");

    let nvim::Connect {
        mut nvim,
        initial_position,
        nvim_child_process
    } = nvim::Connect::connect_parent_or_child(&opt.address, prints_protection)?;

    let cx = cli::Context::new(&opt, nvim_child_process, initial_position, piped);
    info!("context: {:#?}", cx);

    let nvim_manager = &mut nvim::Manager::new(&mut nvim);
    let mut app = App::new(nvim_manager);

    app.handle_close_instance_pty(&cx)?;
    app.handle_open_provided_files(&cx)?;
    if cx.creates {
        let (buffer, pty_path) = app.handle_open_pty_buffer(&cx)?;
        let mut pty_device = OpenOptions::new().append(true).open(&pty_path)?;
        app.handle_user_command(&cx.opt.command, &buffer)?;
        app.handle_instance_buffer(&cx, &buffer, &mut pty_device)?;
        app.handle_regular_buffer(&cx)?;
        app.handle_redirection(&cx, &mut pty_device, pty_path)?;
        app.handle_user_command_post(&cx.opt.command_post, &buffer)?;
    }
    app.handle_exit(cx)?;
    Ok(())
}


