use clap::IntoApp;
use clap_complete::shells::{Zsh, Bash, Fish};

use std::{env, fs, error::Error, path::PathBuf};

include!("src/pager/cli.rs");


fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(
        env::var("OUT_DIR")
        .unwrap()
    ).join("shell_completions");

    fs::create_dir_all(&out_dir)?;
    eprintln!("Shell completions would be generated in: {}", out_dir.display());

    let mut app = Options::into_app();
    clap_complete::generate_to(Zsh , &mut app, "page", &out_dir)?;
    clap_complete::generate_to(Bash, &mut app, "page", &out_dir)?;
    clap_complete::generate_to(Fish, &mut app, "page", &out_dir)?;

    Ok(())
}
