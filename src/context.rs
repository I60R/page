use crate::{
    nvim::{NeovimActions, listen::PageCommand},
    common::{self, IO},
    cli::Options,
};

use neovim_lib::neovim_api::{Buffer, Window};
use std::{process, sync::mpsc::Receiver};

// Contains data used globally through application
#[derive(Debug)]
pub struct Context {
    pub opt: Options,
    pub initial_window_and_buffer: (Window, Buffer),
    pub nvim_child_process: Option<process::Child>,
    pub switch_back_mode: SwitchBackMode,
    pub instance_mode: InstanceMode,
    pub focuses_on_existed_instance: bool,
    pub creates_output_buffer: bool,
    pub creates_in_split: bool,
    pub prints_output_buffer_pty: bool,
    pub input_from_pipe: bool,
    pub page_id: String,
    pub receiver: Receiver<PageCommand>,
}
    
pub fn create(
    opt: Options,
    nvim_child_process: Option<process::Child>,
    nvim_actions: &mut NeovimActions,
    input_from_pipe: bool,
) -> IO<Context> {
    use self::SwitchBackMode::*;
    let switch_back_mode = if nvim_child_process.is_some() {
        NoSwitch
    } else if opt.back {
        Normal
    } else if opt.back_restore {
        Insert
    } else {
        NoSwitch
    };
    use self::InstanceMode::*;
    let instance_mode = if let Some(instance) = opt.instance.clone() {
        Replace(instance)
    } else if let Some(instance) = opt.instance_append.clone() {
        Append(instance)
    } else {
        NoInstance
    };
    let page_id = common::util::random_unique_string();
    let split_flag_provided = has_split_flag_provided(&opt);
    let creates_output_buffer = !has_early_exit_condition(&opt, input_from_pipe, split_flag_provided);
    let creates_in_split = nvim_child_process.is_none() && split_flag_provided;
    let prints_output_buffer_pty = opt.sink_print || !input_from_pipe && nvim_child_process.is_none();
    let focuses_on_existed_instance = should_focus_existed_instance_buffer(&opt, &instance_mode);
    let receiver = nvim_actions.subscribe_to_page_commands(&page_id)?;
    let initial_window_and_buffer = nvim_actions.get_current_window_and_buffer()?;
    Ok(Context {
        opt,
        instance_mode,
        initial_window_and_buffer,
        nvim_child_process,
        switch_back_mode,
        creates_output_buffer,
        prints_output_buffer_pty,
        creates_in_split,
        focuses_on_existed_instance,
        input_from_pipe,
        page_id,
        receiver,
    })
}

fn should_focus_existed_instance_buffer(opt: &Options, instance_mode: &InstanceMode) -> bool {
    opt.follow || opt.command_auto || opt.command_post.is_some() || instance_mode.is_replace()
}

fn has_split_flag_provided(opt: &Options) -> bool {
    opt.split_left_cols.is_some() || opt.split_right_cols.is_some()
    || opt.split_above_rows.is_some() || opt.split_below_rows.is_some()
    || opt.split_left != 0 || opt.split_right != 0
    || opt.split_above != 0 || opt.split_below != 0
}

fn has_early_exit_condition(opt: &Options, input_from_pipe: bool, creates_in_split: bool) -> bool {
    let has_early_exit_opt = opt.instance_close.is_some() || !opt.files.is_empty();
    has_early_exit_opt && !input_from_pipe && !creates_in_split
    && !opt.back && !opt.back_restore
    && !opt.follow && !opt.follow_all
    && !opt.sink_open && !opt.sink_print
    && !opt.lines_in_query != 0
    && opt.instance.is_none() && opt.instance_append.is_none()
    && opt.command.is_none() && opt.command_post.is_none()
    && &opt.filetype == "pager"
}


#[derive(Debug)] 
pub enum SwitchBackMode {
    Normal,
    Insert,
    NoSwitch,
}

impl SwitchBackMode {
    pub fn is_provided(&self) -> bool {
        if let SwitchBackMode::NoSwitch = self { false } else { true }
    }

    pub fn is_insert(&self) -> bool {
        if let SwitchBackMode::Insert = self { true } else { false }
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
    pub fn try_get_name(&self) -> Option<&String> {
        match self {
            InstanceMode::Append(instance_name) | InstanceMode::Replace(instance_name) => Some(instance_name),
            InstanceMode::NoInstance => None,
        }
    }
}

