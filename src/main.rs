use clap::{App, Arg, SubCommand};
use conductor::{run_project, setup_project, ui};
use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let version = format!(
        "{}.{}.{}{}",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH"),
        option_env!("CARGO_PKG_VERSION_PRE").unwrap_or("")
    );
    let matches = App::new("Conductor")
    .version(&*version)
    .author("Joe Bellus <joe@5sigma.io>")
    .about("Conductor orchistraites running local development environments for applications that have many seperate projects. The project structure is defined in a configuration file and conductor can be used to launch and initialize all the projects at once.")
    .arg(
      Arg::with_name("config")
        .short("c")
        .long("config")
        .value_name("FILE")
        .default_value("conductor.yml")
        .help("The conductor project configuration")
        .takes_value(true),
    )
    .arg(
      Arg::with_name("tags")
        .short("t")
        .long("tags")
        .help("limit the operation to only components with a specific tag")
        .value_name("TAG1,TAG2")
        .takes_value(true),
    )
    .subcommand(
      SubCommand::with_name("setup")
        .about("clone and initialize the project")
        .arg(
          Arg::with_name("tags")
            .short("t")
            .long("tags")
            .help("limit the operation to only components with a specific tag")
            .value_name("TAG1,TAG2")
            .takes_value(true),
        )
        .alias("soundcheck")
        .alias("clone"),
    )
    .subcommand(
      SubCommand::with_name("run")
        .about("Launches all project components.")
        .arg(
          Arg::with_name("tags")
            .short("t")
            .long("tags")
            .help("limit the operation to only components with a specific tag")
            .value_name("TAG1,TAG2")
            .takes_value(true),
        )
        .alias("play")
        .alias("start"),
    )
    .get_matches();

    let config_file = if let Some(cf) = matches.value_of("config") {
        String::from(cf)
    } else {
        "conductor.yml".into()
    };

    match find_config(&config_file) {
        Some(config_fp) => {
            let tags: Option<Vec<&str>> = match matches.value_of("tags") {
                Some(tags_r) => Some(tags_r.split(',').map(|i| i).collect()),
                _ => None,
            };

            match matches.subcommand() {
                ("setup", _) => setup_project(&config_fp),
                ("run", _) => run_project(&config_fp, tags),
                _ => run_project(&config_fp, tags),
            };
        }
        None => ui::system_error(format!("Could not find config file {}", config_file)),
    };
}

fn find_config(config: &str) -> Option<PathBuf> {
    env::current_dir()
        .and_then(|dir| Ok(find_file(&dir, config)))
        .unwrap_or(None)
}

fn find_file(starting_directory: &Path, filename: &str) -> Option<PathBuf> {
    let mut path: PathBuf = starting_directory.into();
    let file = Path::new(&filename);

    loop {
        path.push(file);

        if path.is_file() {
            break Some(path);
        }

        if !(path.pop() && path.pop()) {
            // remove file && remove parent
            break None;
        }
    }
}
