#[macro_use]
extern crate structopt;

extern crate neovim_lib;
extern crate rand;


mod util;
mod cli;

use neovim_lib::{self as nvim, neovim_api as nvim_api, NeovimApi, Value};
use rand::{Rng, thread_rng, distributions::Alphanumeric};
use structopt::StructOpt;
use std::{
    fs::{self, remove_file, File, OpenOptions},
    path::PathBuf,
    io::{self, Read, Write},
    iter,
    thread,
    process::{self, Command, Stdio},
    time::Duration,
    net::SocketAddr,
    error::Error,
    os::unix::fs::FileTypeExt,
};


/// Extends `nvim::Session` to be able to spawn new nvim process.
/// Unlike `nvim::Session::ClientConnection::Child` stdin|stdout of new process will be not inherited.
struct NvimSessionConnector {
    nvim_session: nvim::Session,
    nvim_child_process: Option<process::Child>
}

impl NvimSessionConnector {
    fn connect_to_parent_or_child(nvim_parent_listen_address: &Option<String>) -> io::Result<NvimSessionConnector> {
        Ok(if let Some(nvim_parent_listen_address) = nvim_parent_listen_address {
            NvimSessionConnector {
                nvim_session: NvimSessionConnector::session_from_address(nvim_parent_listen_address)?,
                nvim_child_process: None
            }
        } else {
            let (nvim_child_listen_address, nvim_child_process) = NvimSessionConnector::spawn_child_nvim_process()?;
            NvimSessionConnector {
                nvim_session: NvimSessionConnector::session_from_address(nvim_child_listen_address.to_string_lossy().as_ref())?,
                nvim_child_process: Some(nvim_child_process)
            }
        })
    }

    fn spawn_child_nvim_process() -> io::Result<(PathBuf, process::Child)> {
        let nvim_child_listen_address = {
            let mut path = PathBuf::from("/tmp/nvim-page");
            fs::create_dir_all(&path)?;
            path.push(&format!("socket-{}", random_string()));
            path
        };
        let nvim_child_process = Command::new("nvim")
            .stdin(Stdio::null()) // Don't inherit stdin, nvim can't redirect content into terminal(!) buffer
            .env("NVIM_LISTEN_ADDRESS", &nvim_child_listen_address)
            .spawn()?;
        thread::sleep(Duration::from_millis(150)); // Wait while nvim child process connects to socket.
        Ok((nvim_child_listen_address, nvim_child_process))
    }

    fn session_from_address(nvim_listen_address: impl AsRef<str>) -> io::Result<nvim::Session> {
        let nvim_listen_address = nvim_listen_address.as_ref();
        nvim_listen_address.parse::<SocketAddr>()
            .ok().map_or_else(| |nvim::Session::new_unix_socket(nvim_listen_address),
                              |_|nvim::Session::new_tcp(nvim_listen_address))
    }
}


/// A helper for nvim terminal buffer creation/configuration
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
                Ok(()) => {
                    self.nvim.command("redraw!")?;  // To update statusline
                    return Ok(())
                },
            }
        }
        return Err("Can't update buffer name")?;
    }

    fn set_page_default_options_to_current_buffer(&mut self) -> Result<(), Box<Error>> {
        Ok(self.nvim.command("setl scrollback=-1 scrolloff=999 signcolumn=no nonumber modifiable winfixwidth | norm M")?)
    }

    fn update_current_buffer_filetype(&mut self, filetype: &str) -> Result<(), Box<Error>> {
        Ok(self.nvim.command(&format!("setl filetype={}", filetype))?)
    }

    fn execute_user_command_on_current_buffer(&mut self, command: &str) -> Result<(), Box<Error>> {
        Ok(self.nvim.command(command)?)
    }

    fn get_current_buffer_position(&mut self) -> Result<(nvim_api::Window, nvim_api::Buffer), Box<Error>> {
        Ok((self.nvim.get_current_win()?, self.nvim.get_current_buf()?))
    }

    fn switch_to_buffer_position(&mut self, (win, buf): &(nvim_api::Window, nvim_api::Buffer)) -> Result<(), Box<Error>> {
        self.nvim.set_current_win(win)?;
        self.nvim.set_current_buf(buf)?;
        Ok(())
    }

    fn open_file_buffer(&mut self, file: &str) -> Result<(), Box<Error>> {
        let file_path = fs::canonicalize(file)?;
        self.nvim.command(&format!("e {}", file_path.to_string_lossy()))?;
        self.set_page_default_options_to_current_buffer()
    }
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
    instance: Option<&'a String>,
    nvim_child_process: Option<process::Child>,
    initial_position: (nvim_api::Window, nvim_api::Buffer),
    read_from_fifo: bool,
}


// Handles application use cases
struct App<'a> {
    nvim_manager: &'a mut NvimManager<'a>,
    pty_path: &'a mut Option<PathBuf>,
    new_position: &'a mut Option<(nvim_api::Window, nvim_api::Buffer)>,
}

impl <'a> App<'a> {
    fn handle_close_instance_flag(&mut self, &Cx { opt, ref nvim_child_process, .. }: &Cx) -> Result<(), Box<Error>> {
        if let Some(instance_name) = opt.instance_close.as_ref() {
            match nvim_child_process {
                Some(_) => eprintln!("Can't close instance on newly spawned nvim process"),
                None => self.nvim_manager.close_pty_instance(&instance_name)?,
            }
        }
        Ok(())
    }

    fn handle_files_provided(&mut self, &Cx { opt, ref initial_position, read_from_fifo, instance, .. }: &Cx) -> Result<(), Box<Error>> {
        if !opt.files.is_empty() {
            for file in opt.files.iter().as_ref() {
                match self.nvim_manager.open_file_buffer(file) {
                    Err(e) => eprintln!("Error opening \"{}\": {}", file, e),
                    _ => self.nvim_manager.set_page_default_options_to_current_buffer()?
                }
            }
            if read_from_fifo || instance.is_some() || opt.back {
                self.nvim_manager.switch_to_buffer_position(&initial_position)?;
            }
        }
        Ok(())
    }

    fn handle_exit_before_pty_open(&self, &Cx { opt, instance, .. }: &Cx) {
        let can_exit = opt.instance_close.is_some() || !opt.files.is_empty();
        if (!opt.pty_open && can_exit)
            && !opt.back
            && instance.is_none()
            && opt.command.is_none() && opt.command_post.is_none()
            && opt.split_left_cols.is_none() && opt.split_right_cols.is_none() && opt.split_above_rows.is_none() && opt.split_below_rows.is_none()
            && opt.split_left == 0 && opt.split_right == 0 && opt.split_above == 0 && opt.split_below == 0
            && &opt.filetype == "pager"
            {
            process::exit(0);
        }
    }

    fn handle_pty_open(&mut self, &Cx { opt, ref nvim_child_process, instance, read_from_fifo, .. }: &Cx) -> Result<(), Box<Error>> {
        let open_page_buffer = |app: &mut App| -> Result<(), Box<Error>> {
            if nvim_child_process.is_none() {
                app.nvim_manager.split_current_buffer_if_required(opt)?;
            }
            *app.pty_path = Some(app.nvim_manager.create_pty_with_buffer()?);
            *app.new_position = Some(app.nvim_manager.get_current_buffer_position()?);
            app.nvim_manager.set_page_default_options_to_current_buffer()?;
            app.nvim_manager.update_current_buffer_filetype(&opt.filetype)
        };
        match instance {
            None => {
                open_page_buffer(self)?;
                self.nvim_manager.update_current_buffer_name(if read_from_fifo {
                    r"get(g:, 'page_icon_pipe', '\\|§')"
                } else {
                    r"get(g:, 'page_icon_redirect', '>§')"
                })?;
            },
            Some(instance_name) => match self.nvim_manager.try_get_pty_path_of_instance(&instance_name) {
                Ok(pty_path) => *self.pty_path = Some(pty_path),
                Err(e) => {
                    if e.description() != "Instance don't exists" {
                        eprintln!("Can't connect to '{}': {}", &instance_name, e);
                    }
                    open_page_buffer(self)?;
                    self.nvim_manager.register_buffer_as_instance(&instance_name, &self.pty_path.as_ref().unwrap().to_string_lossy())?;
                    self.nvim_manager.update_current_buffer_name(&format!(r"get(g:, 'page_icon_instance', '§') . '{}'", instance_name))?;
                }
            }
        };
        Ok(())
    }

    fn handle_redirect_mode(&mut self, &Cx { opt, ref initial_position, read_from_fifo, instance, .. }: &Cx) -> Result<(), Box<Error>> {
        if let Some(user_command) = opt.command.as_ref() {
            self.nvim_manager.execute_user_command_on_current_buffer(user_command)?;
        }
        if opt.back {
            self.nvim_manager.switch_to_buffer_position(&initial_position)?;
        }
        if let Some(pty_path) = self.pty_path {
            if read_from_fifo {
                let mut pty_device = OpenOptions::new().append(true).open(&pty_path)?;
                if opt.instance_append.is_none() {
                    write!(&mut pty_device, "\x1B[3J\x1B[H\x1b[2J")?; // Clear screen
                }
                let stdin = io::stdin();
                io::copy(&mut stdin.lock(), &mut pty_device).map(drop)?;
                if instance.is_some() {
                    self.nvim_manager.update_current_buffer_filetype(&opt.filetype)?;
                }
            }
            if !read_from_fifo || opt.pty_print {
                println!("{}", pty_path.to_string_lossy());
            }
        }
        if let (Some(user_command_post), Some(new_position)) = (opt.command_post.as_ref(), self.new_position.as_ref()) {
            let current_position = if new_position != initial_position {
                Some(self.nvim_manager.get_current_buffer_position()?)
            } else {
                None
            };
            self.nvim_manager.execute_user_command_on_current_buffer(user_command_post)?;
            if let Some(saved_position) = current_position {
                self.nvim_manager.switch_to_buffer_position(&saved_position)?;
            }
        }
        Ok(())
    }

    fn handle_exit(self, Cx { nvim_child_process, .. }: Cx) -> Result<(), Box<Error>> {
        nvim_child_process.map_or(Ok(()), |mut process| { process.wait()?; Ok(()) })
    }
}



fn main() -> Result<(), Box<Error>> {
    let opt = cli::Opt::from_args();

    let NvimSessionConnector { mut nvim_session, nvim_child_process } = NvimSessionConnector::connect_to_parent_or_child(&opt.address)?;
    nvim_session.start_event_loop();
    let mut nvim = nvim::Neovim::new(nvim_session);

    let cx = Cx {
        opt: &opt,
        instance: opt.instance.as_ref().or_else(||opt.instance_append.as_ref()),
        nvim_child_process,
        initial_position: (nvim.get_current_win()?, nvim.get_current_buf()?),
        read_from_fifo: is_reading_from_fifo(),
    };
    let mut app = App {
        nvim_manager: &mut NvimManager { nvim: &mut nvim, },
        pty_path: &mut None,
        new_position: &mut None,
    };
    app.handle_close_instance_flag(&cx)?;
    app.handle_files_provided(&cx)?;
    app.handle_exit_before_pty_open(&cx);
    app.handle_pty_open(&cx)?;
    app.handle_redirect_mode(&cx)?;
    app.handle_exit(cx)
}
