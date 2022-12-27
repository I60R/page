use clap_complete::shells::{Zsh, Bash, Fish};
use clap::CommandFactory;

use std::{fs, path::{PathBuf, Path}};


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


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(
        std::env::var("OUT_DIR")
            .unwrap()
    ).join("assets");

    fs::create_dir_all(&out_dir)?;
    eprintln!("Assets would be generated in: {}", out_dir.display());

    #[cfg(feature = "pager")]
    {
        let mut app = pager::Options::command();
        clap_complete::generate_to(Zsh , &mut app, "page", &out_dir)?;
        clap_complete::generate_to(Bash, &mut app, "page", &out_dir)?;
        clap_complete::generate_to(Fish, &mut app, "page", &out_dir)?;

        let page_1 = Path::new(&out_dir)
            .join("page.1");
        let mut page_1 = fs::File::create(page_1)?;
        let man = clap_mangen::Man::new(app);
        man.render_title(&mut page_1)?;
        man.render_description_section(&mut page_1)?;
        let mut options_section = vec![];
        man.render_options_section(&mut options_section)?;
        let options_section = String::from_utf8(options_section)?
            .replace("{n} ~ ~ ~", "")
            .replace("{n} ^ ~ ~ ~", "")
            .replace("[", "【\\fB")
            .replace("]", "\\fR】")
            .replace("【\\fBFILE\\fR】", "[FILE]..")
            .replace("【\\fB\\fIFILE\\fR\\fR】", "[FILE]..");
        use std::io::Write;
        write!(page_1, "{options_section}")?;
        man.render_authors_section(&mut page_1)?;
    }

    #[cfg(feature = "picker")]
    {
        let mut app = picker::Options::command();
        clap_complete::generate_to(Zsh , &mut app, "nv", &out_dir)?;
        clap_complete::generate_to(Bash, &mut app, "nv", &out_dir)?;
        clap_complete::generate_to(Fish, &mut app, "nv", &out_dir)?;

        let nv_1 = Path::new(&out_dir)
            .join("nv.1");
        let mut nv_1 = fs::File::create(nv_1)?;
        let man = clap_mangen::Man::new(app);
        man.render_title(&mut nv_1)?;
        man.render_description_section(&mut nv_1)?;
        let mut options_section = vec![];
        man.render_options_section(&mut options_section)?;
        let options_section = String::from_utf8(options_section)?
            .replace("{n} ~ ~ ~", "")
            .replace("{n} ^ ~ ~ ~", "")
            .replace("[", "【\\fB")
            .replace("]", "\\fR】")
            .replace("【\\fBFILE\\fR】", "[FILE]..")
            .replace("【\\fB\\fIFILE\\fR\\fR】", "[FILE]..");
        use std::io::Write;
        write!(nv_1, "{options_section}")?;
        man.render_authors_section(&mut nv_1)?;
    }

    Ok(())
}
