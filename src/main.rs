use clap::{App, Arg, SubCommand};
use conductor::{ui, Project};
// use pty::fork::Fork;
use std::env;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Fork::from_ptmx().unwrap();

  let matches = handle_cli()?;
  if let Err(e) = run(matches) {
    println!("Error: {}", e)
  }
  Ok(())
}

fn run(matches: clap::ArgMatches<'_>) -> Result<(), std::boxed::Box<dyn std::error::Error>> {
  if matches.is_present("debug") {
    let _ = simple_logger::init_with_level(log::Level::Debug);
  }
  let config_fp = match matches.value_of("config") {
    Some(fp_str) => {
      let fp: PathBuf = fp_str.into();
      if fp.is_file() {
        Some(fp)
      } else {
        None
      }
    }
    None => find_config("conductor.yml"),
  }
  .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "config not found"))?;
  let mut project = Project::load(&config_fp)?;
  let mut root_path = config_fp;
  root_path.pop();


  if project
    .run_names(vec![matches.subcommand().0.to_string()])
    .is_ok()
  {
    return Ok(());
  }

  match matches.subcommand() {
    ("setup", _) => project.setup(),
    ("run", Some(m)) => {
      let component_names: Vec<String> = m
        .values_of("component")
        .map(|c| c.collect())
        .unwrap_or_else(Vec::new)
        .into_iter()
        .map(String::from)
        .collect();
      if !component_names.is_empty() {
        let _ = project.run_names(component_names);
        return Ok(());
      } else {
        if project.components.is_empty() {
          ui::system_error("No components to run".into());
          return Ok(());
        }
        project.filter_default();
        project.run();
      }
    }
    _ => {
      project.filter_default();
      project.run();
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

fn handle_cli<'a>() -> Result<clap::ArgMatches<'a>, Box<dyn std::error::Error>> {
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
        .help("The conductor project configuration")
        .takes_value(true),
    )
    .arg(
      Arg::with_name("debug")
        .short("v")
        .long("debug")
        .help("Enable debug logging")
    )
    .subcommand(
      SubCommand::with_name("setup")
        .about("clone and initialize the project")
        .display_order(1)
        .alias("soundcheck")
        .alias("clone"),
    )
    .subcommand(
      SubCommand::with_name("run")
        .about("Launches all project components.")
        .display_order(1)
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

      let mut cmds: Vec<App> = vec![];

      // PROJECT LEVEL TASKS
      if !project.tasks.is_empty() {
        cmds.push(SubCommand::with_name("   ").display_order(1000));
      }

      for task in project.tasks.iter() {
        cmds.push(
          SubCommand::with_name(&task.name)
            .display_order(1001)
            .about("Run project task"),
        );
      }

      // GROUPS

      if !project.groups.is_empty() {
        cmds.push(SubCommand::with_name("   ").display_order(1002));
      }

      for g in project.groups.iter() {
        cmds.push(
          SubCommand::with_name(&*g.name)
            .about("Run component group")
            .display_order(1003),
        );
      }

      // COMPONENTS && COMPONENT TASKS

      if !project.components.is_empty() {
        cmds.push(SubCommand::with_name("   ").display_order(1004));
      }

      for c in project.components.iter() {
        cmds.push(
          SubCommand::with_name(&*c.name)
            .display_order(1005)
            .about("Run component"),
        );
      }

      for component in project.components {
        for task in component.tasks {
          cmds.push(
            SubCommand::with_name(&format!("{}:{}", &component.name, &task.name))
              .about("Run component task")
              .display_order(1005),
          );
        }
      }

      args.subcommands(cmds)
    }
  };
  Ok(args.get_matches())
}
