use structopt::clap::Shell;
use std::{env, fs, error::Error, path::PathBuf};

include!("src/pager/cli.rs");


fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap()).join("shell_completions");
    eprintln!("Shell completions would be generated in the following dir: {}", out_dir.display());
    fs::create_dir_all(&out_dir)?;
    let mut app = Options::clap();
    app.gen_completions("page", Shell::Zsh, &out_dir);
    app.gen_completions("page", Shell::Bash, &out_dir);
    app.gen_completions("page", Shell::Fish, &out_dir);
    Ok(())
}
