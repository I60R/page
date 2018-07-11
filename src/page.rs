#[macro_use]
extern crate structopt;

extern crate neovim_lib;
extern crate rand;
extern crate notify;


mod cli;
mod common;

use neovim_lib::{self as nvim, neovim_api as nvim_api, NeovimApi, Value};
use rand::{Rng, thread_rng, distributions::Alphanumeric};
use structopt::StructOpt;
use std::{
    env,
    fs::{self, remove_file, File, OpenOptions},
    path::PathBuf,
    iter,
    io::{self, Read, Write},
    thread,
    process::{self, Command, Stdio},
    sync::mpsc,
    time::Duration,
    net::SocketAddr,
    error::Error,
    os::unix::fs::FileTypeExt,
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


// Handles application use cases
struct App<'a> {
    nvim_manager: &'a mut NvimManager<'a>,
    pty_path: &'a mut Option<PathBuf>,
    buffer: &'a mut Option<nvim_api::Buffer>,
}

impl <'a> App<'a> {
    fn handle_close_instance_pty(&mut self, &Cx { opt, ref nvim_child_process, .. }: &Cx) -> IO {
        if let Some(name) = opt.instance_close.as_ref() {
            match nvim_child_process {
                Some(_) => eprintln!("Can't close instance on newly spawned nvim process"),
                None => self.nvim_manager.close_pty_instance(name)?,
            }
        }
        Ok(())
    }

    fn handle_open_files_provided(&mut self, &Cx { opt, ref initial_position, read_from_fifo, use_instance, .. }: &Cx) -> IO {
        if !opt.files.is_empty() {
            for file in opt.files.iter().as_ref() {
                if let Err(e) = self.nvim_manager.open_file_buffer(file) {
                    println!("Error opening \"{}\": {}", file, e);
                } else {
                    self.nvim_manager.set_page_default_options_to_current_buffer()?;
                    self.nvim_manager.set_current_buffer_reading_mode()?;
                }
            }
            if read_from_fifo || use_instance.is_some() {
                self.nvim_manager.switch_to_buffer_position(&initial_position)?;
            }
        }
        Ok(())
    }

    fn should_exit_without_pty_open(&self, &Cx { opt, read_from_fifo, .. }: &Cx) -> bool {
        let has_early_exit_command = opt.instance_close.is_some() || !opt.files.is_empty();
        (has_early_exit_command && !opt.pty_open && !read_from_fifo) // Check for absence of other commands
            && !opt.back && !opt.back_insert
            && !opt.pty_print
            && !opt.follow
            && opt.instance.is_none() && opt.instance_append.is_none()
            && opt.command.is_none() && opt.command_post.is_none()
            && opt.split_left_cols.is_none() && opt.split_right_cols.is_none() && opt.split_above_rows.is_none() && opt.split_below_rows.is_none()
            && opt.split_left == 0 && opt.split_right == 0 && opt.split_above == 0 && opt.split_below == 0
            && &opt.filetype == "pager"
    }

    fn handle_pty_open_with_settings(&mut self, &Cx { opt, ref nvim_child_process, use_instance, read_from_fifo, .. }: &Cx) -> IO {
        let open_page_buffer_hook = |app: &mut App| -> IO<_> {
            if nvim_child_process.is_none() {
                app.nvim_manager.split_current_buffer_if_required(opt)?;
            }
            let (buffer, pty_path) = app.nvim_manager.create_pty_with_buffer()?;
            app.nvim_manager.set_page_default_options_to_current_buffer()?;
            app.nvim_manager.update_buffer_filetype(&buffer, &opt.filetype)?;
            Ok((buffer, pty_path))
        };
        let (buffer, pty_path) = match use_instance {
            None => {
                let (buffer, pty_path) = open_page_buffer_hook(self)?;
                let (page_icon_key, page_icon_default) = if read_from_fifo { ("page_icon_pipe", "|$") } else { ("page_icon_redirect", ">$") };
                let page_icon = self.nvim_manager.get_var_or_default(page_icon_key, page_icon_default)?;
                self.nvim_manager.update_buffer_name(&buffer, &page_icon)?;
                (buffer, pty_path)
            },
            Some(name) => {
                match self.nvim_manager.find_instance_buffer(&name)? {
                    Some(it) => it,
                    None => {
                        let (buffer, pty_path) = open_page_buffer_hook(self)?;
                        self.nvim_manager.register_buffer_as_instance(name, &buffer, &pty_path.to_string_lossy())?;
                        let page_icon = self.nvim_manager.get_var_or_default("page_icon_instance", "$")?;
                        self.nvim_manager.update_buffer_name(&buffer, &format!("{}{}", page_icon, name))?;
                        (buffer, pty_path)
                    }
                }
            }
        };
        *self.buffer = Some(buffer);
        *self.pty_path = Some(pty_path);
        Ok(())
    }

    fn handle_user_command(&mut self, command: &Option<String>) -> IO {
        if let Some(command) = command {
            self.nvim_manager.execute_user_command_on_buffer(self.buffer.as_ref().unwrap(), &command)?;
        }
        Ok(())
    }

    fn handle_user_command_post(&mut self, command: &Option<String>) -> IO {
        if command.is_some() {
            thread::sleep(Duration::from_millis(50)); // Fixes errors when `page` is set as `$MANPAGER`
            self.handle_user_command(command)?;
        }
        Ok(())
    }


    fn handle_redirect_mode(&mut self, &Cx { opt, read_from_fifo, ref initial_position, ref nvim_child_process, use_instance, .. }: &Cx) -> IO {
        let App { ref mut nvim_manager, pty_path, buffer, .. } = self;
        let (pty_path, buffer) = (pty_path.as_ref().unwrap(), buffer.as_ref().unwrap());

        let before_writing_hook = |nvim_manager: &mut NvimManager| -> IO {
            if opt.follow {
                nvim_manager.set_current_buffer_follow_output_mode()?;
                thread::sleep(Duration::from_millis(50)); // Nvim needs time to process key
            } else {
                nvim_manager.set_current_buffer_reading_mode()?;
            }
            if nvim_child_process.is_some() {
                return Ok(()); // There's no buffer to switch back
            }
            if opt.back_insert || opt.back {
                thread::sleep(Duration::from_millis(50)); // To prevent sending keys into wrong buffer
                nvim_manager.switch_to_buffer_position(&initial_position)?;
            }
            if opt.back_insert {
                nvim_manager.set_current_buffer_insert_mode()?;
            }
            Ok(())
        };
        let uses_instance = use_instance.is_some();
        if uses_instance || read_from_fifo {
            let mut pty_device = OpenOptions::new().append(true).open(pty_path)?;
            if uses_instance {
                nvim_manager.nvim.set_current_buf(&buffer)?;
                if opt.instance.is_some() {
                    write!(&mut pty_device, "\x1B[3J\x1B[H\x1b[2J")?; // Clear screen sequence
                }
            }
            before_writing_hook(nvim_manager)?;
            if read_from_fifo {
                let stdin = io::stdin();
                io::copy(&mut stdin.lock(), &mut pty_device).map(drop)?;
            }
        } else {
            before_writing_hook(nvim_manager)?;
        }
        if opt.pty_print || (!read_from_fifo && nvim_child_process.is_none()) {
            println!("{}", pty_path.to_string_lossy());
        }
        Ok(())
    }

    fn handle_exit(self, Cx { nvim_child_process, .. }: Cx) -> IO {
        if let Some(mut nvim_child_process) = nvim_child_process {
            nvim_child_process.wait().map(drop)?;
        };
        Ok(())
    }
}



fn main() -> IO {
    let opt = cli::Opt::from_args();
    let read_from_fifo = is_reading_from_fifo();

    let NvimSessionConnector { mut nvim_session, nvim_child_process, } =
        NvimSessionConnector::connect_to_parent_or_child(&opt, read_from_fifo)?;

    nvim_session.start_event_loop();
    let mut nvim = nvim::Neovim::new(nvim_session);

    let cx = Cx {
        opt: &opt,
        use_instance: opt.instance.as_ref().or_else(|| opt.instance_append.as_ref()),
        nvim_child_process,
        initial_position: (nvim.get_current_win()?, nvim.get_current_buf()?),
        read_from_fifo,
    };
    let mut app = App {
        nvim_manager: &mut NvimManager { nvim: &mut nvim, },
        pty_path: &mut None,
        buffer: &mut None,
    };

    app.handle_close_instance_pty(&cx)?;
    app.handle_open_files_provided(&cx)?;
    if !app.should_exit_without_pty_open(&cx) {
        app.handle_pty_open_with_settings(&cx)?;
        app.handle_user_command(&cx.opt.command)?;
        app.handle_redirect_mode(&cx)?;
        app.handle_user_command_post(&cx.opt.command_post)?;
    }
    app.handle_exit(cx)?;
    Ok(())
}


