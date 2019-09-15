/// A module which contains data available on page invocation

use crate::cli::Options;
use std::path::PathBuf;
use log::warn;


// Contains data required after page was spawned from shell
#[derive(Debug)]
pub struct CliContext {
    pub opt: Options,
    pub page_id: String,
    pub tmp_dir: PathBuf,
    pub input_from_pipe: bool,
    pub print_protection: bool,
}

pub fn after_page_spawned() -> CliContext {
    let opt = crate::cli::get_options();
    let tmp_dir = {
        let d = std::env::temp_dir().join("neovim-page");
        std::fs::create_dir_all(&d).expect("Cannot create temporary directory for page");
        d
    };
    let page_id = {
        use rand::Rng;
        rand::thread_rng().sample_iter(&rand::distributions::Alphanumeric).take(8).collect()
    };
    let input_from_pipe = {
        use atty::Stream;
        !atty::is(Stream::Stdin)
    };
    if !input_from_pipe && 0u64 < opt.query_lines {
        warn!("Query works only when page reads from pipe");
    }
    let print_protection = !input_from_pipe
        && !opt.page_no_protect
        && std::env::var_os("PAGE_REDIRECTION_PROTECT").map_or(true, |v| v != "0");
    CliContext {
        opt,
        tmp_dir,
        page_id,
        input_from_pipe,
        print_protection,
    }
}


// Contains data required after neovim is attached
#[derive(Debug)]
pub struct NeovimContext {
    pub opt: Options,
    pub page_id: String,
    pub inst_mode: InstanceMode,
    pub nvim_child_proc_spawned: bool,
    pub use_outp_buf: bool,
    pub use_outp_buf_in_split: bool,
    pub input_from_pipe: bool,
}

pub fn after_neovim_connected(cli_ctx: CliContext, nvim_child_proc_spawned: bool) -> NeovimContext {
    let CliContext { opt, page_id, input_from_pipe, .. } = cli_ctx;
    let inst_mode = if let Some(instance) = opt.instance.clone() {
        InstanceMode::Replace(instance)
    } else if let Some(instance) = opt.instance_append.clone() {
        InstanceMode::Append(instance)
    } else {
        InstanceMode::NoInstance
    };
    let split_implied = opt.is_split_implied();
    let use_outp_buf_in_split = match (split_implied, nvim_child_proc_spawned) {
        (true, true) => { warn!("Split is ignored when using spawned neovim instance"); false }
        x => x.0
    };
    let use_outp_buf = input_from_pipe || split_implied || opt.is_output_buffer_implied();
    NeovimContext {
        opt,
        page_id,
        inst_mode,
        use_outp_buf,
        use_outp_buf_in_split,
        nvim_child_proc_spawned,
        input_from_pipe,
    }
}

#[derive(Debug, Clone)]
pub enum InstanceMode {
    Append(String),
    Replace(String),
    NoInstance,
}

impl InstanceMode {
    pub fn is_replace(&self) -> bool {
        if let InstanceMode::Replace(_) = self { true } else { false }
    }
    pub fn is_no_instance(&self) -> bool {
        if let InstanceMode::NoInstance = self { true } else { false }
    }
    pub fn any(&self) -> Option<&String> {
        match self {
            InstanceMode::Append(name) | InstanceMode::Replace(name) => Some(name),
            InstanceMode::NoInstance => None,
        }
    }
}


/// Contains data required after buffer for output was found
#[derive(Debug)]
pub struct OutputContext {
    pub opt: Options,
    pub inst_mode: InstanceMode,
    pub inst_focus: bool,
    pub move_cursor: bool,
    pub switch_back_mode: SwitchBackMode,
    pub buf_pty_path: PathBuf,
    pub input_from_pipe: bool,
    pub print_output_buf_pty: bool,
}

pub fn after_output_found(neovim_ctx: NeovimContext, buf_pty_path: PathBuf) -> OutputContext {
    after_output(neovim_ctx, buf_pty_path, true)
}
pub fn after_output_created(neovim_ctx: NeovimContext, buf_pty_path: PathBuf) -> OutputContext {
    after_output(neovim_ctx, buf_pty_path, false)
}

fn after_output(nvim_ctx: NeovimContext, buf_pty_path: PathBuf, inst_exist: bool) -> OutputContext {
    let NeovimContext { opt, nvim_child_proc_spawned, inst_mode, input_from_pipe, .. } = nvim_ctx;
    let switch_back_mode = if nvim_child_proc_spawned {
        SwitchBackMode::NoSwitch
    } else if opt.back {
        SwitchBackMode::Normal
    } else if opt.back_restore {
        SwitchBackMode::Insert
    } else {
        SwitchBackMode::NoSwitch
    };
    let inst_focus = inst_exist && opt.is_focus_on_existed_instance_buffer_implied();
    let move_cursor = inst_focus || !inst_exist || inst_mode.is_no_instance();
    let print_output_buf_pty = !nvim_child_proc_spawned && !input_from_pipe || opt.sink_print;
    OutputContext {
        opt,
        inst_mode,
        switch_back_mode,
        buf_pty_path,
        inst_focus,
        move_cursor,
        input_from_pipe,
        print_output_buf_pty,
    }
}

#[derive(Debug)]
pub enum SwitchBackMode {
    Normal,
    Insert,
    NoSwitch,
}

impl SwitchBackMode {
    pub fn is_any(&self) -> bool {
        if let SwitchBackMode::NoSwitch = self { false } else { true }
    }

    pub fn is_insert(&self) -> bool {
        match self {
            SwitchBackMode::Insert => true,
            _ => false,
        }
    }
}
