use clap::{App, Arg, SubCommand};
use conductor::{run_component, run_components, run_project, setup_project, ui, Project};
use std::env;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
                .multiple(true)
                .help("a specific component to execute")
        )
        .alias("play")
        .alias("start"),
    );

  let args = match find_config("conductor.yml") {
    None => args,
    Some(local_config_fp) => {
      let project = Project::load(&local_config_fp)?;
      let cmp_commands: Vec<App> = project
        .components
        .iter()
        .map(|c| {
          SubCommand::with_name(&*c.name)
            .about("Run component")
            .display_order(10)
        })
        .collect();
      let args = args.subcommands(cmp_commands);
      let group_commands: Vec<App> = project
        .groups
        .iter()
        .map(|c| {
          SubCommand::with_name(&*c.name)
            .about("Run component group")
            .display_order(10)
        })
        .collect();
      args.subcommands(group_commands)
    }
  };

  let matches = args.get_matches();

  let config_fp = match matches.value_of("config") {
    Some(fp) => Some(fp.into()),
    None => find_config("conductor.yml"),
  }
  .ok_or(std::io::Error::new(
    std::io::ErrorKind::NotFound,
    "config not found",
  ))?;

  let project = Project::load(&config_fp)?;

  // Dynamic subcommands
  let tags: Option<Vec<&str>> = match matches.value_of("tags") {
    Some(tags_r) => Some(tags_r.split(',').map(|i| i).collect()),
    _ => None,
  };

  if let Some(direct_cmp) = &project
    .components
    .iter()
    .find(|x| x.name == matches.subcommand().0)
  {
    if let Err(e) = run_component(&config_fp, &direct_cmp.name) {
      ui::system_error(format!("{}", e))
    }
    return Ok(());
  }
  if let Some(direct_group) = &project
    .groups
    .into_iter()
    .find(|x| x.name == matches.subcommand().0)
  {
    if let Err(e) = run_components(&config_fp, direct_group.components.to_owned()) {
      ui::system_error(format!("{}", e))
    }
    return Ok(());
  }

  match matches.subcommand() {
    ("setup", _) => setup_project(&config_fp),
    ("run", Some(m)) => {
      let components: Vec<String> = m
        .values_of("component")
        .map(|c| c.collect())
        .unwrap_or(vec![])
        .into_iter()
        .map(|c| String::from(c))
        .collect();
      if !components.is_empty() {
        if let Err(e) = run_components(&config_fp, components) {
          ui::system_error(format!("{}", e))
        }
      } else {
        if let Err(e) = run_project(&config_fp, tags.clone()) {
          ui::system_error(format!("{}", e))
        }
      }
    }
    _ => {
      if let Err(e) = run_project(&config_fp, tags.clone()) {
        ui::system_error(format!("{}", e))
      }
    }
  };
  Ok(())
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
