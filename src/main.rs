use clap::{App, Arg, SubCommand};
use conductor::{
  get_components, run_component, run_project, setup_project, shutdown_component_services,
  shutdown_project_services, ui,
};
use std::env;
use std::path::{Path, PathBuf};

fn main() {
  ctrlc::set_handler(move || {
    ui::system_message("Shutting down".into());
  })
  .expect("Could not setup handler");
  let version = format!(
    "{}.{}.{}{}",
    env!("CARGO_PKG_VERSION_MAJOR"),
    env!("CARGO_PKG_VERSION_MINOR"),
    env!("CARGO_PKG_VERSION_PATCH"),
    option_env!("CARGO_PKG_VERSION_PRE").unwrap_or("")
  );
  let args = App::new("Conductor")
    .version(&*version)
    .author("Joe Bellus <joe@5sigma.io>")
    .about("Conductor orchistraites running local development environments for applications that have many seperate projects. The project structure is defined in a configuration file and conductor can be used to launch and initialize all the projects at once.")
    .display_order(1)
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
        .display_order(1)
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
        .display_order(1)
        .arg(
          Arg::with_name("tags")
            .short("t")
            .long("tags")
            .help("limit the operation to only components with a specific tag")
            .value_name("TAG1,TAG2")
            .takes_value(true),
        )
        .arg(
            Arg::with_name("component")
                .index(1)
                .help("a specific component to execute")
        )
        .alias("play")
        .alias("start"),
    );

  let cmp_commands = match find_config("conductor.yml") {
    Some(config_fp) => {
      let cmps = get_components(&config_fp);
      cmps
        .iter()
        .map(|c| {
          SubCommand::with_name(&*c.name)
            .about("Run component")
            .display_order(10)
        })
        .collect()
    }
    _ => vec![],
  };

  let args = args.subcommands(cmp_commands);
  let matches = args.get_matches();

  let config_file = if let Some(cf) = matches.value_of("config") {
    String::from(cf)
  } else {
    "conductor.yml".into()
  };

  // Dynamic subcommands
  match find_config(&config_file) {
    Some(config_fp) => {
      let tags: Option<Vec<&str>> = match matches.value_of("tags") {
        Some(tags_r) => Some(tags_r.split(',').map(|i| i).collect()),
        _ => None,
      };

      let cmps = get_components(&config_fp);
      if let Some(direct_cmp) = cmps.iter().find(|x| x.name == matches.subcommand().0) {
        run_component(&config_fp, &direct_cmp.name);
        shutdown_component_services(&config_fp, &direct_cmp.name);
        return;
      }

      match matches.subcommand() {
        ("setup", _) => setup_project(&config_fp),
        ("run", Some(m)) => {
          if let Some(cmp) = m.value_of("component") {
            run_component(&config_fp, cmp);
            shutdown_component_services(&config_fp, cmp);
          } else {
            run_project(&config_fp, tags.clone());
            shutdown_project_services(&config_fp, tags);
          }
        }
        _ => {
          run_project(&config_fp, tags.clone());
          shutdown_project_services(&config_fp, tags);
        }
      };
    }
    None => ui::system_error(format!("Could not find config file {}", config_file)),
  };
}

fn find_config(config: &str) -> Option<PathBuf> {
  env::current_dir()
    .map(|dir| find_file(&dir, config))
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
      break None;
    }
  }
}
