use clap::IntoApp;
use clap_generate::generators::{Zsh, Bash, Fish};

use std::{env, fs, error::Error, path::PathBuf};

include!("src/pager/cli.rs");


fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap()).join("shell_completions");
    eprintln!("Shell completions would be generated in the following dir: {}", out_dir.display());
    fs::create_dir_all(&out_dir)?;
    let mut app = Options::into_app();
    clap_generate::generate_to(Zsh , &mut app, "page", &out_dir)?;
    clap_generate::generate_to(Bash, &mut app, "page", &out_dir)?;
    clap_generate::generate_to(Fish, &mut app, "page", &out_dir)?;
    Ok(())
}
