use indoc::indoc;
use nvim_rs::{neovim::Neovim, error::CallError, Buffer, Window, Value};
use connection::IoWrite;

use crate::cli::FileOption;

pub struct NeovimActions {
    nvim: Neovim<IoWrite>
}

impl From<Neovim<IoWrite>> for NeovimActions {
    fn from(nvim: Neovim<IoWrite>) -> Self {
        NeovimActions { nvim }
    }
}

impl NeovimActions {

    pub async fn open_file_buffer(&mut self, file: &FileOption) {
        let cmd = format!("e {}", file.as_str());
        self.nvim.command(&cmd).await
            .expect("Cannot open file buffer");
    }
}
