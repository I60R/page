pub(crate) mod common;
pub(crate) mod cli;
pub(crate) mod nvim;
pub(crate) mod context;

use crate::common::IO;

use atty::Stream;
use std::{env, str::FromStr};
use log::{info, LevelFilter};
use fern::Dispatch;



fn main() -> IO {
    init_logger()?;
    let opt = cli::get_options();
    info!("options: {:#?}", opt);
    let page_tmp_dir = common::util::get_page_tmp_dir()?;
    let input_from_pipe = atty::isnt(Stream::Stdin);
    if opt.lines_in_query != 0 && !input_from_pipe {
        eprintln!("Query works only when page reads from pipe");
    }
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

fn init_logger() -> IO {
    Dispatch::new()
        .format(|out, message, record| out.finish(format_args!("[{}][{}] {}", record.level(), record.target(), message)))
        .level(LevelFilter::from_str(env::var("RUST_LOG").as_ref().map(String::as_ref).unwrap_or("warn"))?)
        .level_for("neovim_lib", LevelFilter::Off)
        .chain(std::io::stderr())
        .apply()?;
    Ok(())
}



pub(crate) mod app {
    use crate::{
        common::IO,
        nvim::{NeovimActions, listen::PageCommand},
        context::Context,
    };
    use std::{
        io::{self, Write, BufReader, BufRead},
        fs::{OpenOptions, File},
        path::PathBuf,
        process,
    };    
    use neovim_lib::neovim_api::Buffer;



    /// A manager for `page` application action
    pub struct AppActions<'a> {
        nvim_actions: NeovimActions,
        context: &'a Context,
    }

    impl<'a> AppActions<'a> {
        pub fn handle_close_page_instance_buffer(&mut self) -> IO {
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

        pub fn handle_display_plain_files(&mut self) -> IO {
            let Self { nvim_actions, context, .. } = self;
            for file in &context.opt.files {
                if let Err(e) = nvim_actions.open_file_buffer(file) {
                    eprintln!("Error opening \"{}\": {}", file, e);
                } else {
                    let command_or_empty = &context.opt.command.as_ref().map(String::as_ref).unwrap_or_default();
                    nvim_actions.set_page_options_to_current_buffer("&filetype", command_or_empty, "", "")?; // The same filetype
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

        pub fn get_output_buffer(self) -> IO<OutputActions<'a>> {
            if let Some(instance_name) = self.context.instance_mode.try_get_name() {
                self.get_instance_output_buffer(instance_name)
            } else {
                self.get_oneoff_output_buffer()
            }
        }

        fn get_instance_output_buffer(mut self, instance_name: &'a str) -> IO<OutputActions<'a>> {
            if let Some((buffer, buffer_pty_path)) = self.nvim_actions.find_instance_buffer(instance_name)? {
                let Self { nvim_actions, context, .. } = self;
                Ok(OutputActions { existed_instance: true, sink: None, nvim_actions, context, buffer, buffer_pty_path })
            } else {
                let (buffer, buffer_pty_path) = self.open_new_output_buffer()?;
                self.nvim_actions.register_buffer_as_instance(&buffer, instance_name, &buffer_pty_path.to_string_lossy())?;
                let Self { nvim_actions, context, .. } = self;
                Ok(OutputActions { existed_instance: false, sink: None, nvim_actions, context, buffer, buffer_pty_path })
            }
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
            let command_or_emtpy = command.as_ref().map(String::as_ref).unwrap_or_default();
            let page_command = if context.opt.lines_in_query != 0 { 
                format!(r#"command! -nargs=? Page call rpcnotify(0, "page_fetch_lines", "{}", <args>)"#, context.page_id)
            } else {
                String::new()
            };
            let page_command_disconnect = if context.opt.lines_in_query != 0 {
                format!(r#"autocmd BufDelete <buffer> call rpcnotify(0, "page_buffer_closed", "{}")"#, context.page_id)
            } else { 
                String::new()
            };
            nvim_actions.set_page_options_to_current_buffer(filetype, command_or_emtpy, &page_command, &page_command_disconnect)?;
            Ok((buffer, buffer_pty_path))
        }
    }

    pub fn create_app(nvim_actions: NeovimActions, context: &Context) -> AppActions {
        AppActions { nvim_actions, context, }
    }


    /// A manager for output buffer actions 
    pub struct OutputActions<'a> {
        nvim_actions: NeovimActions,
        context: &'a Context,
        existed_instance: bool,
        buffer: Buffer,
        buffer_pty_path: PathBuf,
        sink: Option<File>
    }

    impl<'a> OutputActions<'a> {
        pub fn handle_instance_state(&mut self) -> IO {
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

        pub fn handle_commands(&mut self) -> IO {
            if self.context.opt.command_auto {
                self.nvim_actions.execute_connect_autocmd_on_current_buffer()?;
            }
            if let Some(ref command) = self.context.opt.command_post {
                self.nvim_actions.execute_command_post(&command)?;
            }
            Ok(())
        }

        pub fn handle_scroll_and_switch_back(&mut self) -> IO {
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

        pub fn handle_output(&mut self) -> IO {
            let Self { nvim_actions, context, sink, buffer_pty_path, .. } = self;
            if context.input_from_pipe {
                let stdin = io::stdin();
                let opened_sink = Self::get_opened_sink(sink, buffer_pty_path)?;
                let mut lines_to_read = context.opt.lines_in_query;
                if lines_to_read == 0 {
                    io::copy(&mut stdin.lock(), opened_sink).map(drop)?;
                } else {
                    let mut lines_to_read_in_last_query = lines_to_read;
                    let mut stdin_lines = BufReader::new(stdin.lock()).lines();
                    loop {
                        if lines_to_read == 0 {
                            nvim_actions.notify_query_finished(lines_to_read_in_last_query)?;
                            match context.receiver.recv() {
                                Ok(PageCommand::FetchLines(number)) => { lines_to_read = number; lines_to_read_in_last_query = number },
                                Ok(PageCommand::FetchPart) => lines_to_read = context.opt.lines_in_query,
                                _ => break,
                            }
                        }
                        match stdin_lines.next() {
                            Some(Ok(line)) => writeln!(opened_sink, "{}", line)?,
                            Some(Err(err)) => {
                                eprintln!("error reading stdin line: {}", err);
                                break;
                            },
                            None => {
                                nvim_actions.notify_query_finished(lines_to_read_in_last_query - lines_to_read)?;
                                break;
                            }
                        }
                        lines_to_read -= 1;
                    }
                }
                nvim_actions.notify_page_read()?;
            }
            if context.prints_output_buffer_pty {
                println!("{}", buffer_pty_path.to_string_lossy());
            }
            Ok(())
        }

        pub fn handle_disconnect(&mut self) -> IO {
            let Self { nvim_actions, buffer, context, .. } = self;
            if context.opt.command_auto {
                let final_buffer = nvim_actions.get_current_buffer()?;
                let temp_switch_buffer = &final_buffer != buffer;
                if temp_switch_buffer {
                    nvim_actions.switch_to_buffer(&buffer)?;
                }
                nvim_actions.execute_disconnect_autocmd_on_current_buffer()?;
                if temp_switch_buffer {
                    nvim_actions.switch_to_buffer(&final_buffer)?;
                    if context.initial_window_and_buffer.1 == final_buffer && context.switch_back_mode.is_insert() {
                        nvim_actions.set_current_buffer_insert_mode()?;
                    }
                }
            }
            Ok(())
        }
    }

    pub fn exit(nvim_child_process: Option<process::Child>) -> IO {
        if let Some(mut process) = nvim_child_process {
            process.wait().map(drop)?;
        }
        Ok(())
    }
}
