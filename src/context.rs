/// A module which contains data available on page invocation

use crate::cli::Options;
use std::path::PathBuf;
use log::warn;


/// Contains data required after page was spawned from shell
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
        let pid = std::process::id();
        let time = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos();
        format!("{}{}", pid, time) // provides enough entropy for current use case
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


/// Contains data required after neovim is attached
#[derive(Debug)]
pub struct NeovimContext {
    pub opt: Options,
    pub page_id: String,
    pub inst_buf_usage: InstanceBufferUsage,
    pub nvim_child_proc_spawned: bool,
    pub use_outp_buf: bool,
    pub use_outp_buf_in_split: bool,
    pub input_from_pipe: bool,
}

pub fn after_neovim_connected(cli_ctx: CliContext, nvim_child_proc_spawned: bool) -> NeovimContext {
    let CliContext { opt, page_id, input_from_pipe, .. } = cli_ctx;
    let inst_buf_usage = if let Some(inst_name) = opt.instance.clone() {
        InstanceBufferUsage::ReplaceContent { inst_name }
    } else if let Some(inst_name) = opt.instance_append.clone() {
        InstanceBufferUsage::AppendContent { inst_name }
    } else {
        InstanceBufferUsage::Disabled
    };
    let split_implied = opt.is_split_implied();
    let use_outp_buf_in_split = if split_implied && nvim_child_proc_spawned {
        warn!("Split is ignored when using spawned neovim instance");
        false
    } else {
        split_implied
    };
    let use_outp_buf = input_from_pipe || split_implied || opt.is_output_buffer_implied();
    NeovimContext {
        opt,
        page_id,
        inst_buf_usage,
        use_outp_buf,
        use_outp_buf_in_split,
        nvim_child_proc_spawned,
        input_from_pipe,
    }
}

#[derive(Debug, Clone)]
pub enum InstanceBufferUsage {
    AppendContent {
        inst_name: String
    },
    ReplaceContent {
        inst_name: String
    },
    Disabled,
}

impl InstanceBufferUsage {
    pub fn should_replace_content(&self) -> bool {
        if let InstanceBufferUsage::ReplaceContent { .. } = self { true } else { false }
    }

    pub fn is_disabled(&self) -> bool {
        if let InstanceBufferUsage::Disabled = self { true } else { false }
    }

    pub fn try_get_instance_name(&self) -> Option<&String> {
        match self {
            InstanceBufferUsage::AppendContent{ inst_name } | InstanceBufferUsage::ReplaceContent { inst_name } => Some(inst_name),
            InstanceBufferUsage::Disabled => None,
        }
    }
}


/// Contains data required after buffer for output was found
#[derive(Debug)]
pub struct OutputContext {
    pub opt: Options,
    pub inst_buf_usage: InstanceBufferUsage,
    pub inst_focus: bool,
    pub move_cursor: bool,
    pub restore_initial_buf_focus: RestoreInitialBufferFocus,
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
    let NeovimContext { opt, nvim_child_proc_spawned, inst_buf_usage, input_from_pipe, .. } = nvim_ctx;
    let restore_initial_buf_focus = if nvim_child_proc_spawned {
        warn!("Switch back is ignored when using spawned neovim instance");
        RestoreInitialBufferFocus::Disabled
    } else if opt.back {
        RestoreInitialBufferFocus::ViModeNormal
    } else if opt.back_restore {
        RestoreInitialBufferFocus::ViModeInsert
    } else {
        RestoreInitialBufferFocus::Disabled
    };
    let inst_focus = inst_exist && opt.is_focus_on_existed_instance_buffer_implied();
    let move_cursor = inst_focus || !inst_exist || inst_buf_usage.is_disabled();
    let print_output_buf_pty = !nvim_child_proc_spawned && !input_from_pipe || opt.sink_print;
    OutputContext {
        opt,
        inst_buf_usage,
        restore_initial_buf_focus,
        buf_pty_path,
        inst_focus,
        move_cursor,
        input_from_pipe,
        print_output_buf_pty,
    }
}

#[derive(Debug)]
pub enum RestoreInitialBufferFocus {
    ViModeNormal,
    ViModeInsert,
    Disabled,
}

impl RestoreInitialBufferFocus {
    pub fn is_disabled(&self) -> bool {
        if let RestoreInitialBufferFocus::Disabled = self { true } else { false }
    }

    pub fn is_vi_mode_insert(&self) -> bool {
        if let RestoreInitialBufferFocus::ViModeInsert = self { true } else { false }
    }
}
