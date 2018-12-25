use structopt::clap::Shell;
use std::{env, fs, error::Error, path::PathBuf};

include!("src/cli.rs");


fn main() -> Result<(), Box<dyn Error>> {
    let completions_dir = {
        let mut completions_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
        completions_dir.push("target");
        completions_dir.push("shell_completions");
        completions_dir
    };
    println!("output dir for shell completions: {:?}", completions_dir);
    fs::create_dir_all(&completions_dir)?;
    let mut app = Options::clap();
    app.gen_completions("page", Shell::Zsh, &completions_dir);
    app.gen_completions("page", Shell::Bash, &completions_dir);
    app.gen_completions("page", Shell::Fish, &completions_dir);
    Ok(())
}