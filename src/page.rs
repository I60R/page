mod common;
mod cli;
mod nvim;
mod context;

use crate::common::IO;

use atty::Stream;
use std::{
    env,
    str::FromStr,
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



fn main() -> IO {
    init_logger()?;
    let opt = cli::get_options();
    info!("options: {:#?}", opt);
    let page_tmp_dir = common::util::get_page_tmp_dir()?;
    let input_from_pipe = atty::isnt(Stream::Stdin);
    let prints_protection = !input_from_pipe && !opt.page_no_protect && env::var_os("PAGE_REDIRECTION_PROTECT").map_or(true, |v| v != "0");
    let (mut nvim_actions, nvim_child_process) = nvim::connection::get_nvim_connection(&opt, &page_tmp_dir, prints_protection)?;
    let context = context::create(opt, nvim_child_process, &mut nvim_actions, input_from_pipe)?;
    info!("context: {:#?}", context);
    let mut app_actions = app::create_app(nvim_actions, &context);
    app_actions.handle_close_page_instance_buffer()?;
    app_actions.handle_display_plain_files()?;
    if context.creates_output_buffer {
        let mut output_buffer_actions = app_actions.get_output_buffer()?;
        output_buffer_actions.handle_instance_state()?;
        output_buffer_actions.handle_commands()?;
        output_buffer_actions.handle_scroll_and_switch_back()?;
        output_buffer_actions.handle_output()?;
        output_buffer_actions.handle_disconnect()?;
    }
    app::exit(context.nvim_child_process)
}

pub(crate) fn init_logger() -> IO {
    Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}] {}",
                record.level(),
                record.target(),
                message
            ))
        })
        .level(LevelFilter::from_str(env::var("RUST_LOG").as_ref().map(String::as_ref).unwrap_or("warn"))?)
        .level_for("neovim_lib", LevelFilter::Off)
        .chain(std::io::stderr())
        .apply()?;
    Ok(())
}



pub(crate) mod app {
    use crate::{
        common::IO,
        nvim::NeovimActions,
        context::Context,
    };
    use std::{
        io::{self, Write},
        fs::{OpenOptions, File},
        path::PathBuf,
        process,
    };    
    use neovim_lib::neovim_api::Buffer;



    /// A manager for `page` application action
    pub(crate) struct AppActions<'a> {
        nvim_actions: NeovimActions,
        context: &'a Context,
    }

    impl<'a> AppActions<'a> {
        pub(crate) fn handle_close_page_instance_buffer(&mut self) -> IO {
            let Self { nvim_actions, context, .. } = self;
            if let Some(ref name) = context.opt.instance_close {
                if context.nvim_child_process.is_none() {
                    nvim_actions.close_instance_buffer(name)?;
                } else {
                    eprintln!("Newly spawned neovim process cannot contain any instances")
                }
            }
            Ok(())
        }

        pub(crate) fn handle_display_plain_files(&mut self) -> IO {
            let Self { nvim_actions, context, .. } = self;
            for file in &context.opt.files {
                if let Err(e) = nvim_actions.open_file_buffer(file) {
                    eprintln!("Error opening \"{}\": {}", file, e);
                } else {
                    let command = &context.opt.command.as_ref().map(String::as_ref).unwrap_or_default();
                    nvim_actions.set_page_options_to_current_buffer("&filetype", command)?; // The same filetype
                    if context.opt.follow_all {
                        nvim_actions.set_current_buffer_follow_output_mode()?;
                    } else {
                        nvim_actions.set_current_buffer_scroll_mode()?;
                    }
                }
            }
            if !context.opt.files.is_empty() && context.creates_in_split {
                nvim_actions.switch_to_window_and_buffer(&context.initial_window_and_buffer)?;
            }
            Ok(())
        }

        pub(crate) fn get_output_buffer(self) -> IO<OutputActions<'a>> {
            if let Some(instance_name) = self.context.instance_mode.try_get_name() {
                self.get_instance_output_buffer(instance_name)
            } else {
                self.get_oneoff_output_buffer()
            }
        }

        fn get_instance_output_buffer(mut self, instance_name: &'a str) -> IO<OutputActions<'a>> {
            Ok(if let Some((buffer, buffer_pty_path)) = self.nvim_actions.find_instance_buffer(instance_name)? {
                let Self { nvim_actions, context, .. } = self;
                OutputActions { existed_instance: true, sink: None, nvim_actions, context, buffer, buffer_pty_path }
            } else {
                let (buffer, buffer_pty_path) = self.open_new_output_buffer()?;
                self.nvim_actions.register_buffer_as_instance(&buffer, instance_name, &buffer_pty_path.to_string_lossy())?;
                let Self { nvim_actions, context, .. } = self;
                OutputActions { existed_instance: false, sink: None, nvim_actions, context, buffer, buffer_pty_path }
            })
        }

        fn get_oneoff_output_buffer(mut self) -> IO<OutputActions<'a>> {
            let (buffer, buffer_pty_path) = self.open_new_output_buffer()?;
            let Self { mut nvim_actions, context, .. } = self;
            let (page_icon_key, page_icon_default) = if context.input_from_pipe { ("page_icon_pipe", " |") } else { ("page_icon_redirect", " >") };
            let mut buffer_title = nvim_actions.get_var_or_default(page_icon_key, page_icon_default)?;
            if let Some(ref buffer_name) = context.opt.name {
                buffer_title.insert_str(0, buffer_name);
            }
            nvim_actions.update_buffer_title(&buffer, &buffer_title)?;
            Ok(OutputActions { existed_instance: false, sink: None, nvim_actions, context, buffer, buffer_pty_path })
        }

        fn open_new_output_buffer(&mut self) -> IO<(Buffer, PathBuf)> {
            let Self { nvim_actions, context, .. } = self;
            if context.creates_in_split {
                nvim_actions.split_current_buffer(&context.opt)?;
            }
            let (buffer, buffer_pty_path) = nvim_actions.create_output_buffer_with_pty()?;
            let (filetype, command) = (&context.opt.filetype, &context.opt.command);
            nvim_actions.set_page_options_to_current_buffer(filetype, command.as_ref().map(String::as_ref).unwrap_or_default())?;
            Ok((buffer, buffer_pty_path))
        }
    }

    pub(crate) fn create_app(nvim_actions: NeovimActions, context: &Context) -> AppActions {
        AppActions { nvim_actions, context, }
    }


    /// A manager for output buffer actions 
    pub(crate) struct OutputActions<'a> {
        nvim_actions: NeovimActions,
        context: &'a Context,
        existed_instance: bool,
        buffer: Buffer,
        buffer_pty_path: PathBuf,
        sink: Option<File>
    }

    impl<'a> OutputActions<'a> {
        pub(crate) fn handle_instance_state(&mut self) -> IO {
            let Self { nvim_actions, context, sink, buffer_pty_path, buffer, .. } = self;
            if let Some(instance_name) = context.instance_mode.try_get_name() {
                Self::update_instance_buffer_title(nvim_actions, &context.opt.name, instance_name, &buffer)?;
                if context.focuses_on_existed_instance {
                    nvim_actions.focus_instance_buffer(&buffer)?;
                }
                if context.instance_mode.is_replace() {
                    let opened_sink = Self::get_opened_sink(sink, buffer_pty_path)?;
                    write!(opened_sink, "\x1B[3J\x1B[H\x1b[2J")?; // Clear screen sequence
                }
            }
            Ok(())
        }

        fn update_instance_buffer_title(
            nvim_actions: &mut NeovimActions,
            buffer_name: &Option<String>,
            instance_name: &str,
            buffer: &Buffer,
        ) -> IO {
            let (page_icon_key, page_icon_default) = ("page_icon_instance", "@ ");
            let mut buffer_title = nvim_actions.get_var_or_default(page_icon_key, page_icon_default)?;
            buffer_title.insert_str(0, instance_name);
            if let Some(ref buffer_name) = buffer_name {
                if buffer_name != instance_name {
                    buffer_title.push_str(buffer_name);
                }
            }
            nvim_actions.update_buffer_title(&buffer, &buffer_title)?;
            Ok(())
        }

        fn get_opened_sink<'b>(sink: &'b mut Option<File>, buffer_pty_path: &'b PathBuf) -> IO<&'b mut File> {
            Ok(if let Some(opened_sink) = sink {
                opened_sink
            } else {
                let opened_sink = OpenOptions::new().append(true).open(buffer_pty_path)?;
                *sink = Some(opened_sink);
                sink.as_mut().unwrap()
            })
        }

        pub(crate) fn handle_commands(&mut self) -> IO {
            if self.context.opt.command_auto {
                self.nvim_actions.execute_page_connect_autocmd_on_buffer(&self.buffer)?;
            }
            if let Some(ref command) = self.context.opt.command_post {
                self.nvim_actions.execute_command_post(&command)?;
            }
            Ok(())
        }

        pub(crate) fn handle_scroll_and_switch_back(&mut self) -> IO {
            let Self { nvim_actions, context, .. } = self;
            if self.existed_instance && !context.focuses_on_existed_instance {
                return Ok(());
            }
            if context.opt.follow {
                nvim_actions.set_current_buffer_follow_output_mode()?;
            } else {
                nvim_actions.set_current_buffer_scroll_mode()?;
            }
            if context.switch_back_mode.is_provided() {
                nvim_actions.switch_to_window_and_buffer(&context.initial_window_and_buffer)?;
                if context.switch_back_mode.is_insert() {
                    nvim_actions.set_current_buffer_insert_mode()?;
                }
            }
            Ok(())
        }

        pub(crate) fn handle_output(&mut self) -> IO {
            let Self { context, sink, buffer_pty_path, .. } = self;
            if context.input_from_pipe {
                let stdin = io::stdin();
                let opened_sink = Self::get_opened_sink(sink, buffer_pty_path)?;
                io::copy(&mut stdin.lock(), opened_sink).map(drop)?;
            }
            if context.prints_output_buffer_pty {
                println!("{}", buffer_pty_path.to_string_lossy());
            }
            Ok(())
        }

        pub(crate) fn handle_disconnect(&mut self) -> IO {
            let Self { nvim_actions, buffer, context, .. } = self;
            if context.opt.command_auto {
                let temp_switch_buffer = buffer != &nvim_actions.get_current_buffer()?;
                if temp_switch_buffer {
                    nvim_actions.switch_to_buffer(&buffer)?;
                }
                nvim_actions.execute_page_disconnect_autocmd_on_buffer(&buffer)?;
                if temp_switch_buffer {
                    nvim_actions.switch_to_buffer(&context.initial_window_and_buffer.1)?;
                }
            }
            Ok(())
        }
    }

    pub(crate) fn exit(nvim_child_process: Option<process::Child>) -> IO {
        if let Some(mut process) = nvim_child_process {
            process.wait().map(drop)?;
        }
        Ok(())
    }
}
