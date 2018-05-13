#[macro_use]
extern crate structopt;

extern crate neovim_lib;
extern crate rand;


mod util;
mod cli;

use neovim_lib::{self as nvim, neovim_api as nvim_api, NeovimApi, Value};
use rand::{Rng, thread_rng};
use std::fs::{self, remove_file, File, OpenOptions};
use std::path::PathBuf;
use std::io::{self, Read, Write};
use std::iter;
use std::process::{self, Command, Stdio};
use std::thread;
use std::time::Duration;
use std::net::{SocketAddr};
use std::error::Error;
use std::os::unix::fs::FileTypeExt;
use structopt::StructOpt;


/// Extends `nvim::Session` with optional `nvim_process` field.
/// That `nvim_process` might be a spawned on top `nvim` process connected through unix socket.
/// It's the same that `nvim::Session::ClientConnection::Child` but stdin|stdout don't inherited.
struct RunningSession {
    nvim_session: nvim::Session,
    nvim_child_process: Option<process::Child>
}

impl RunningSession {
    fn connect_to_parent_or_child(nvim_listen_address: Option<&String>) -> io::Result<RunningSession> {
        nvim_listen_address
            .map_or_else(RunningSession::spawn_child,
                         |address| RunningSession::connect_to_parent(address)
                             .or_else(|e| {
                                 eprintln!("can't connect to parent nvim session: {}", e);
                                 RunningSession::spawn_child()
                    }))
    }

    fn spawn_child() -> io::Result<RunningSession> {
        let mut nvim_listen_address = PathBuf::from("/tmp/nvim-page");
        fs::create_dir_all(&nvim_listen_address)?;
        nvim_listen_address.push(&format!("socket-{}", random_string()));
        let nvim_child_process = Command::new("nvim")
            .stdin(Stdio::null())
            .env("NVIM_LISTEN_ADDRESS", &nvim_listen_address)
            .spawn()?;
        thread::sleep(Duration::from_millis(150)); // Wait until nvim process not connected to socket.
        let nvim_session = nvim::Session::new_unix_socket(&nvim_listen_address)?;
        Ok(RunningSession { nvim_session, nvim_child_process: Some(nvim_child_process) })
    }

    fn connect_to_parent(nvim_listen_address: &str) -> io::Result<RunningSession> {
        let nvim_session = nvim_listen_address.parse::<SocketAddr>().ok()
            .map_or_else(||nvim::Session::new_unix_socket(nvim_listen_address),
                         |_|nvim::Session::new_tcp(nvim_listen_address))?;
        Ok(RunningSession { nvim_session, nvim_child_process: None })
    }
}


/// A helper for nvim terminal buffer creation/setting
struct NvimManager<'a> {
    nvim: &'a mut nvim::Neovim,
}

impl <'a> NvimManager<'a> {
    fn create_pty_with_buffer(&mut self) -> Result<PathBuf, Box<Error>> {
        let agent_pipe_name = random_string();
        self.nvim.command(&format!("term pty-agent {}", agent_pipe_name))?;
        Ok(self.read_pty_device_path(&agent_pipe_name)?)
    }

    fn register_buffer_as_instance(&mut self, instance_name: &str, instance_pty_path: &str) -> Result<(), Box<Error>> {
        self.nvim.command(&format!("\
            let last_page_instance = '{}'
            let g:page_instances[last_page_instance] = [ bufnr('%'), '{}' ]", instance_name, instance_pty_path))?;
        Ok(())
    }

    fn try_get_pty_path_of_instance(&mut self, name: &str) -> Result<PathBuf, Box<Error>> {
        let pty_path_str = self.nvim.command_output(&format!("\
            let g:page_instances = get(g:, 'page_instances', {{}})
            let page_instance = get(g:page_instances, '{}', -99999999)
            if bufexists(page_instance[0])
                 echo page_instance[1]
            else
                throw \"Instance don't exists\"
            endif", name))?;
        Ok(PathBuf::from(pty_path_str))
    }

    fn close_pty_instance(&mut self, instance_name: &str) -> Result<(), Box<Error>> {
        self.nvim.command_output(&format!("\
            let g:page_instances = get(g:, 'page_instances', {{}})
            let page_instance = get(g:page_instances, '{}', -99999999)
            if bufexists(page_instance[0])
                exe 'bd!' . page_instance[0]
            endif", instance_name))?;
        Ok(())
    }

    fn read_pty_device_path(&mut self, agent_pipe_name: &str) -> Result<PathBuf, Box<Error>> {
        let agent_pipe_path = util::open_agent_pipe(agent_pipe_name)?;
        let mut agent_pipe = File::open(&agent_pipe_path)?;
        let pty_path = {
            let mut pty_path = String::new();
            agent_pipe.read_to_string(&mut pty_path)?;
            PathBuf::from(pty_path)
        };
        if let Err(e) = remove_file(&agent_pipe_path) {
            eprintln!("can't remove agent pipe {:?}: {:?}", &agent_pipe_path, e);
        }
        Ok(pty_path)
    }

    fn split_current_buffer_if_required(&mut self, opt: &cli::Opt) -> Result<(), Box<Error>> {
        if opt.split_right > 0 {
            self.nvim.command("belowright vsplit")?;
            let buf_width = self.nvim.call_function("winwidth", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_width * 3 / (u64::from(opt.split_right) + 1);
            self.nvim.command(&format!("vertical resize {}", resize_ratio))?;
        } else if opt.split_left > 0 {
            self.nvim.command("aboveleft vsplit")?;
            let buf_width = self.nvim.call_function("winwidth", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_width * 3 / (u64::from(opt.split_left) + 1);
            self.nvim.command(&format!("vertical resize {}", resize_ratio))?;
        } else if opt.split_below > 0 {
            self.nvim.command("belowright split")?;
            let buf_height = self.nvim.call_function("winheight", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_height * 3 / (u64::from(opt.split_below) + 1);
            self.nvim.command(&format!("resize {}", resize_ratio))?;
        } else if opt.split_above > 0 {
            self.nvim.command("aboveleft split")?;
            let buf_height = self.nvim.call_function("winheight", vec![Value::from(0)])?.as_u64().unwrap();
            let resize_ratio = buf_height * 3 / (u64::from(opt.split_above) + 1);
            self.nvim.command(&format!("resize {}", resize_ratio))?;
        } else if let Some(split_right_cols) = opt.split_right_cols {
            self.nvim.command(&format!("belowright vsplit | vertical resize {}", split_right_cols))?;
        } else if let Some(split_left_cols) = opt.split_left_cols {
            self.nvim.command(&format!("aboveleft vsplit | vertical resize {}", split_left_cols))?;
        } else if let Some(split_below_rows) = opt.split_below_rows {
            self.nvim.command(&format!("belowright split | resize {}", split_below_rows))?;
        } else if let Some(split_above_rows) = opt.split_above_rows {
            self.nvim.command(&format!("aboveleft split | resize {}", split_above_rows))?;
        }
        Ok(())
    }

    fn update_current_buffer_name(&mut self, name: &str) -> Result<(), Box<Error>> {
        let first_attempt =                 (0, format!("exe 'file ' . {}",          name    ));
        let next_attempts = (1..99).map(|i| (i, format!("exe 'file ' . {} . '({})'", name, i)));
        let buf_exists_err_msg = "0 - Vim(file):E95: Buffer with this name already exists";
        for (attempt_count, cmd) in iter::once(first_attempt).chain(next_attempts) {
            match self.nvim.command(&cmd) {
                Err(e) => if attempt_count > 99 || e.to_string() != buf_exists_err_msg { return Err(e)? },
                Ok(()) => return Ok(()),
            }
        }
        return Err("Can't update buffer name")?;
    }

    fn set_page_default_options_to_current_buffer(&mut self) -> Result<(), Box<Error>> {
        self.nvim.command("setl scrollback=-1 scrolloff=999 signcolumn=no nonumber modifiable winfixwidth | norm M")?;
        Ok(())
    }

    fn update_current_buffer_filetype(&mut self, filetype: &str) -> Result<(), Box<Error>> {
        self.nvim.command(&format!("setl filetype={}", filetype))?;
        Ok(())
    }

    fn execute_user_command_on_current_buffer(&mut self, command: &str) -> Result<(), Box<Error>> {
        self.nvim.command(command)?;
        Ok(())
    }

    fn get_current_buffer_position(&mut self) -> Result<(nvim_api::Window, nvim_api::Buffer), Box<Error>> {
        Ok((self.nvim.get_current_win()?, self.nvim.get_current_buf()?))
    }

    fn switch_to_buffer_position(&mut self, (win, buf): (nvim_api::Window, nvim_api::Buffer)) -> Result<(), Box<Error>> {
        self.nvim.set_current_win(&win)?;
        self.nvim.set_current_buf(&buf)?;
        Ok(())
    }

    fn open_file_buffer(&mut self, file: &str) -> Result<(), Box<Error>> {
        let file_path = fs::canonicalize(file)?;
        self.nvim.command(&format!("e {}", file_path.to_string_lossy()))?;
        self.set_page_default_options_to_current_buffer()
    }
}



fn random_string() -> String {
    thread_rng().gen_ascii_chars().take(32).collect::<String>()
}


fn is_reading_from_fifo() -> bool {
    PathBuf::from("/dev/stdin").metadata() // Probably always returns Err when `page` reads from pipe.
        .map(|stdin_metadata| stdin_metadata.file_type().is_fifo()) // Just to be sure.
        .unwrap_or(true)
}


// Context in which application is invoked. Contains related read-only data
struct Ctx<'a> {
    opt: &'a cli::Opt,
    instance: Option<&'a String>,
    nvim_child_process: Option<process::Child>,
    read_from_fifo: bool,
}


// Handles application use cases
struct App<'a> {
    nvim_manager: &'a mut NvimManager<'a>,
    pty_path: &'a mut Option<PathBuf>,
}

impl <'a> App<'a> {
    fn handle_close_instance_flag(&mut self, &Ctx { opt, ref nvim_child_process, .. }: &Ctx) -> Result<(), Box<Error>> {
        if let Some(instance_name) = opt.instance_close.as_ref().or_else(||opt.instance_close_only.as_ref()) {
            match nvim_child_process {
                Some(_) => eprintln!("Can't close instance on newly spawned nvim process"),
                None => self.nvim_manager.close_pty_instance(&instance_name)?,
            }
        }
        Ok(())
    }

    fn handle_files_provided(&mut self, &Ctx { opt, ref nvim_child_process, read_from_fifo, instance, .. }: &Ctx) -> Result<(), Box<Error>> {
        if !opt.files.is_empty() {
            let continuing = read_from_fifo || instance.is_some();
            let saved_buffer_position = if continuing || opt.back {
                Some(self.nvim_manager.get_current_buffer_position()?)
            } else {
                None
            };
            if nvim_child_process.is_none() && continuing && opt.files.len() == 1 {
                self.nvim_manager.split_current_buffer_if_required(opt)?;
            }
            opt.files.iter().for_each(|file| {
                if let Err(e) = self.nvim_manager.open_file_buffer(&file) {
                    eprintln!("Error opening \"{}\": {}", file, e);
                }
            });
            if let Some(saved_position) = saved_buffer_position {
                self.nvim_manager.switch_to_buffer_position(saved_position)?;
            }
        }
        Ok(())
    }

    fn handle_open_pty(&mut self, &Ctx { opt, ref nvim_child_process, instance, read_from_fifo, .. }: &Ctx) -> Result<(), Box<Error>> {
        if opt.instance_close_only.is_some() {
            return Ok(())
        }
        let saved_buffer_position = if opt.back {
            Some(self.nvim_manager.get_current_buffer_position()?)
        } else {
            None
        };
        let pty_path = match instance {
            None => {
                if nvim_child_process.is_none() {
                    self.nvim_manager.split_current_buffer_if_required(opt)?;
                }
                let redirect_pty_path = self.nvim_manager.create_pty_with_buffer()?;
                self.nvim_manager.update_current_buffer_name(if read_from_fifo {
                    r"get(g:, 'page_icon_pipe', '\\|§')"
                } else {
                    r"get(g:, 'page_icon_redirect', '>§')"
                })?;
                redirect_pty_path
            },
            Some(instance_name) => {
                self.nvim_manager.try_get_pty_path_of_instance(&instance_name)
                    .or_else(|e| {
                        if e.description() != "Instance don't exists" {
                            eprintln!("Can't connect to '{}': {}", &instance_name, e);
                        }
                        if nvim_child_process.is_none() {
                            self.nvim_manager.split_current_buffer_if_required(opt)?;
                        }
                        let instance_pty_path = self.nvim_manager.create_pty_with_buffer()?;
                        self.nvim_manager.register_buffer_as_instance(&instance_name, &instance_pty_path.to_string_lossy())?;
                        self.nvim_manager.update_current_buffer_name(&format!(r"get(g:, 'page_icon_instance', '§') . '{}'", instance_name))?;
                        Ok(instance_pty_path) as Result<PathBuf, Box<Error>>
                    })?
            }
        };
        self.nvim_manager.set_page_default_options_to_current_buffer()?;
        self.nvim_manager.update_current_buffer_filetype(&opt.filetype)?;
        if let Some(user_command) = opt.command.as_ref() {
            self.nvim_manager.execute_user_command_on_current_buffer(&user_command)?;
        }
        if let Some(saved_position) = saved_buffer_position {
            self.nvim_manager.switch_to_buffer_position(saved_position)?;
        }
        *self.pty_path = Some(pty_path);
        Ok(())
    }

    fn handle_redirect_mode(&mut self, &Ctx { opt, ref nvim_child_process, read_from_fifo, .. }: &Ctx) -> Result<(), Box<Error>> {
        if let Some(pty_path) = self.pty_path {
            if read_from_fifo {
                let mut pty_device = OpenOptions::new().append(true).open(&pty_path)?;
                if opt.instance_append.is_none() {
                    write!(&mut pty_device, "\x1B[3J\x1B[H\x1b[2J")?; // Clear screen
                }
                let stdin = io::stdin();
                io::copy(&mut stdin.lock(), &mut pty_device).map(drop)?;
                self.nvim_manager.update_current_buffer_filetype(&opt.filetype)?;
            } else if nvim_child_process.is_none() && (opt.print_pty_path || !read_from_fifo) {
                println!("{}", pty_path.to_string_lossy());
            }
        }
        if let Some(user_command_post) = opt.command_post.as_ref() {
            self.nvim_manager.execute_user_command_on_current_buffer(user_command_post)?;
        }
        Ok(())
    }

    fn handle_exit(self, Ctx { nvim_child_process, .. }: Ctx) -> Result<(), Box<Error>> {
        nvim_child_process.map_or(Ok(()), |mut process| { process.wait()?; Ok(()) })
    }
}



fn main() -> Result<(), Box<Error>> {
    let opt = cli::Opt::from_args();

    let RunningSession { mut nvim_session, nvim_child_process } = RunningSession::connect_to_parent_or_child(opt.address.as_ref())?;
    nvim_session.start_event_loop();
    let mut nvim = nvim::Neovim::new(nvim_session);

    let ctx = Ctx {
        opt: &opt,
        instance: opt.instance.as_ref().or_else(||opt.instance_append.as_ref()),
        nvim_child_process,
        read_from_fifo: is_reading_from_fifo(),
    };
    let mut app = App {
        nvim_manager: &mut NvimManager { nvim: &mut nvim, },
        pty_path: &mut None,
    };
    app.handle_close_instance_flag(&ctx)?;
    app.handle_files_provided(&ctx)?;
    app.handle_open_pty(&ctx)?;
    app.handle_redirect_mode(&ctx)?;
    app.handle_exit(ctx)
}
