use nvim_rs::{neovim::Neovim, error::CallError, Buffer, Window, Value};
use connection::IoWrite;

pub struct NeovimActions {
    nvim: Neovim<IoWrite>
}

impl From<Neovim<IoWrite>> for NeovimActions {
    fn from(nvim: Neovim<IoWrite>) -> Self {
        NeovimActions { nvim }
    }
}
