use clap_complete::shells::{Zsh, Bash, Fish};
use clap::CommandFactory;

use std::{env, fs, error::Error, path::PathBuf};


#[cfg(feature = "pager")]
#[allow(dead_code)]
mod pager {
    include!("src/pager/cli.rs");
}

#[cfg(feature = "picker")]
#[allow(dead_code)]
mod picker {
    include!("src/picker/cli.rs");
}


fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(
        env::var("OUT_DIR")
        .unwrap()
    ).join("shell_completions");

    fs::create_dir_all(&out_dir)?;
    eprintln!("Shell completions would be generated in: {}", out_dir.display());

    #[cfg(feature = "pager")]
    {
        let mut app = pager::Options::command();
        clap_complete::generate_to(Zsh , &mut app, "page", &out_dir)?;
        clap_complete::generate_to(Bash, &mut app, "page", &out_dir)?;
        clap_complete::generate_to(Fish, &mut app, "page", &out_dir)?;
    }

    #[cfg(feature = "picker")]
    {
        let mut app = picker::Options::command();
        clap_complete::generate_to(Zsh , &mut app, "nv", &out_dir)?;
        clap_complete::generate_to(Bash, &mut app, "nv", &out_dir)?;
        clap_complete::generate_to(Fish, &mut app, "nv", &out_dir)?;
    }

    Ok(())
}
