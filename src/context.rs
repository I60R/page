
use crate::{
    nvim::NeovimActions,
    common::IO,
    cli::Options,
};

use neovim_lib::neovim_api::{Buffer, Window};
use std::process;

// Contains data used globally through application
#[derive(Debug)]
pub(crate) struct Context {
    pub opt: Options,
    pub initial_window_and_buffer: (Window, Buffer),
    pub nvim_child_process: Option<process::Child>,
    pub switch_back_mode: SwitchBackMode,
    pub instance_mode: InstanceMode,
    pub creates: bool,
    pub prints: bool,
    pub splits: bool,
    pub focuses: bool,
    pub piped: bool,
}
    
pub(crate) fn create(
    opt: Options,
    nvim_child_process: Option<process::Child>,
    nvim_actions: &mut NeovimActions,
    piped: bool,
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
    let split_flag_provided = has_split_flag_provided(&opt);
    let creates = !has_early_exit_condition(&opt, piped, split_flag_provided);
    let splits = nvim_child_process.is_none() && split_flag_provided;
    let prints = opt.sink_print || !piped && nvim_child_process.is_none();
    let focuses = should_focus_existed_instance_buffer(&opt, &instance_mode);
    let initial_window_and_buffer = nvim_actions.get_current_window_and_buffer()?;
    Ok(Context {
        opt,
        instance_mode,
        initial_window_and_buffer,
        nvim_child_process,
        switch_back_mode,
        creates,
        prints,
        splits,
        focuses,
        piped,
    })
}

fn should_focus_existed_instance_buffer(opt: &Options, instance_mode: &InstanceMode) -> bool {
    opt.follow || opt.command_post.is_some() || instance_mode.is_replace()
}

fn has_split_flag_provided(opt: &Options) -> bool {
    opt.split_left_cols.is_some() || opt.split_right_cols.is_some()
    || opt.split_above_rows.is_some() || opt.split_below_rows.is_some()
    || opt.split_left != 0 || opt.split_right != 0
    || opt.split_above != 0 || opt.split_below != 0
}

fn has_early_exit_condition(opt: &Options, piped: bool, splits: bool) -> bool {
    let has_early_exit_opt = opt.instance_close.is_some() || !opt.files.is_empty();
    has_early_exit_opt && !piped && !splits
    && !opt.back && !opt.back_restore
    && !opt.follow && !opt.follow_all
    && !opt.sink_open && !opt.sink_print
    && opt.instance.is_none() && opt.instance_append.is_none()
    && opt.command.is_none() && opt.command_post.is_none()
    && &opt.filetype == "pager"
}


#[derive(Debug)] 
pub(crate) enum SwitchBackMode {
    Normal,
    Insert,
    NoSwitch,
}

impl SwitchBackMode {
    pub(crate) fn is_provided(&self) -> bool {
        !if let SwitchBackMode::NoSwitch = self { true } else { false }
    }

    pub(crate) fn is_insert(&self) -> bool {
        if let SwitchBackMode::Insert = self { true } else { false }
    }
}


#[derive(Debug, Clone)]
pub(crate) enum InstanceMode {
    Append(String),
    Replace(String),
    NoInstance,
}

impl InstanceMode {
    pub(crate) fn is_replace(&self) -> bool {
        if let InstanceMode::Replace(_) = self { true } else { false }
    }
    pub(crate) fn try_get_name(&self) -> Option<&String> {
        match self {
            InstanceMode::Append(instance_name) | InstanceMode::Replace(instance_name) => Some(instance_name),
            InstanceMode::NoInstance => None,
        }
    }
}

