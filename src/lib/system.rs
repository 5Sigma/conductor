use crate::{ui, Component, Project};
#[cfg(not(target_os = "windows"))]
use rs_docker::Docker;
use std::io;
use std::io::prelude::*;
use std::io::{BufReader, Error, ErrorKind};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;
// use timeout_readwrite::TimeoutReader;

#[derive(Debug)]
pub enum ProjectError {
  Io(io::Error),
}

pub struct ComponentMessage {
  component: Component,
  body: String,
}

struct Worker {
  pub component: Component,
  pub handler: thread::JoinHandle<()>,
  pub kill_signal: Sender<()>,
}

fn create_command(c: &crate::Command, component: &Component, root_path: &PathBuf) -> Command {
  let mut cmd = Command::new(
    expand_str::expand_string_with_env(&c.command).unwrap_or_else(|_| c.command.clone()),
  );
  c.args.iter().for_each(|a| {
    cmd.arg(expand_str::expand_string_with_env(a).unwrap_or_else(|_| a.clone()));
  });
  for (k, v) in &c.env {
    cmd.env(
      k,
      expand_str::expand_string_with_env(&v).unwrap_or_else(|_| v.clone()),
    );
  }
  for (k, v) in &component.env {
    cmd.env(
      k,
      expand_str::expand_string_with_env(&v).unwrap_or_else(|_| v.clone()),
    );
  }
  let dir = component
    .clone()
    .start
    .dir
    .unwrap_or_else(|| component.get_path().to_str().unwrap_or("").into());
  let mut root_path = root_path.clone();
  root_path.push(&expand_str::expand_string_with_env(&dir).unwrap_or_else(|_| dir.clone()));
  cmd.current_dir(root_path);
  cmd
}

fn spawn_component(
  component: Component,
  data_tx: Sender<ComponentMessage>,
  root_path: &PathBuf,
) -> Result<Worker, Error> {
  let data_tx = data_tx;
  let (tx, rx) = mpsc::channel();
  let c = component.clone();
  let rp = root_path.clone();
  let th = thread::spawn(move || {
    loop {
      let mut cmd = create_command(&c.start, &c, &rp);
      if let Some(delay) = c.delay {
        thread::sleep(Duration::from_secs(delay));
      }
      let stdout_result: Result<std::process::ChildStdout, io::Error> =
        cmd.stdout(Stdio::piped()).spawn().and_then(|mut child| {
          child
            .stdout
            .take()
            .ok_or_else(|| Error::new(ErrorKind::Other, "Could not create process pipe"))
        });
      match stdout_result {
        Ok(stdout) => {
          let buf = BufReader::new(stdout);
          let _ = buf.lines().try_for_each(|line| match line {
            Ok(body) => {
              let payload = ComponentMessage {
                component: c.clone(),
                body,
              };
              match data_tx.send(payload) {
                Ok(_) => {}
                Err(e) => println!("Error sending output: {}", e),
              };
              Some(())
            }
            Err(_) => match rx.try_recv() {
              Ok(_) | Err(mpsc::TryRecvError::Disconnected) => Some(()), // currently not checking for kill signals
              Err(mpsc::TryRecvError::Empty) => Some(()),
            },
          });
          ui::system_message(format!("Component shutdown: {}", c.name));
        }
        Err(e) => {
          ui::system_error(format!("Could not load component {}", c.name));
          println!("{}", e);
        }
      };
      if !c.retry {
        break;
      }
      ui::system_message(format!(
        "Restarting {} in {} seconds",
        &c.name,
        c.delay.unwrap_or(1)
      ));
      thread::sleep(Duration::from_secs(c.delay.unwrap_or(1)));
    }
  });

  Ok(Worker {
    kill_signal: tx,
    component,
    handler: th,
  })
}

pub fn run_command(c: &crate::Command, cmp: &Component, root_path: &PathBuf) {
  let mut cmd = create_command(c, cmp, &root_path);
  ui::system_message(format!("Executing: {}", c));

  match cmd
    .stdout(Stdio::piped())
    .spawn()
    .and_then(|mut child| {
      child
        .stdout
        .take()
        .ok_or_else(|| Error::new(ErrorKind::Other, "Could not create process pipe"))
    })
    .map(|stdout| BufReader::new(stdout).lines())
  {
    Ok(lines) => lines.for_each(|line| {
      ui::component_message(cmp, line.unwrap());
    }),
    Err(e) => {
      ui::system_error("Command Error".into());
      println!("{}", e);
    }
  }
}

pub fn run_project(fname: &PathBuf, tags: Option<Vec<&str>>) {
  match Project::load(&fname) {
    Err(e) => {
      ui::system_error("Could not load project".into());
      println!("{}", e);
    }
    Ok(project) => {
      let mut root_path = fname.clone();
      root_path.pop();

      let components = match tags {
        Some(t) => project
          .components
          .into_iter()
          .filter(|x| x.has_tag(&t.clone()))
          .collect(),
        None => project.components,
      };

      for c in components.iter() {
        run_component(&fname, &c.name);
      }
    }
  };
}

#[cfg(not(target_os = "windows"))]
pub fn start_container(name: &str) -> io::Result<String> {
  let mut docker = Docker::connect("unix:///var/run/docker.sock")?;
  docker.start_container(name)
}

#[cfg(not(target_os = "windows"))]
pub fn stop_container(name: &str) -> io::Result<String> {
  let mut docker = Docker::connect("unix:///var/run/docker.sock")?;
  docker.stop_container(name)
}

#[cfg(target_os = "windows")]
pub fn start_container(name: &str) -> io::Result<String> {
  Ok(("".into()))
}

#[cfg(target_os = "windows")]
pub fn stop_container(name: &str) -> io::Result<String> {
  ui::system_error("Services are not supported on windows".into());
  Ok(("".into()))
}

pub fn run_component(fname: &PathBuf, component_name: &str) {
  let mut fname = fname.clone();
  match Project::load(&fname) {
    Err(e) => {
      ui::system_error("Could not load project".into());
      ui::system_error(format!("{}", e));
    }
    Ok(project) => {
      let (tx, rx) = mpsc::channel();
      fname.pop();

      if let Some(c) = project.components.iter().find(|x| x.name == component_name) {
        for service_name in &c.services {
          if let Some(service) = project.service_by_name(&service_name.clone()) {
            match start_container(&service.container.unwrap_or(service.name.clone())) {
              Ok(_) => ui::system_message(format!("Started service {}", &service.name)),
              Err(e) => {
                ui::system_error(format!("Error starting service {}: {}", &service.name, e))
              }
            }
          } else {
            ui::system_error(format!(
              "Could not find service definition: {}",
              service_name
            ))
          }
        }

        match spawn_component(c.clone(), tx, &fname) {
          Ok(_) => ui::system_message(format!("Started {}", c.name)),
          Err(e) => ui::system_error(format!("Failed to start {}: {}", c.name, e)),
        }
        while let Ok(msg) = rx.recv() {
          ui::component_message(&msg.component, msg.body)
        }
      } else {
        ui::system_error(format!("Could not find component: {}", component_name))
      }
    }
  }
}

pub fn setup_project(fname: &PathBuf) {
  match Project::load(&fname) {
    Err(e) => {
      ui::system_error("Could not load project".into());
      println!("{}", e);
    }
    Ok(project) => {
      let mut root_path = fname.clone();
      root_path.pop();
      for cmp in project.components.into_iter() {
        let mut cmp_path = root_path.clone();
        cmp_path.push(cmp.get_path());
        match cmp.clone_repo(&cmp_path) {
          Ok(_) => {
            ui::system_message(format!("{} cloned", cmp.clone().name));
            for cmd in &cmp.init {
              run_command(&cmd, &cmp, &root_path);
            }
          }
          Err(e) => ui::system_error(format!("Skipping clone: {}", e)),
        }
      }
    }
  };
}

pub fn get_components(fname: &PathBuf) -> Vec<Component> {
  match Project::load(&fname) {
    Err(_) => {
      ui::system_error("Could not load project".into());
      vec![]
    }
    Ok(project) => project.components,
  }
}

pub fn shutdown_project_services(fname: &PathBuf, tags: Option<Vec<&str>>) {
  match Project::load(&fname) {
    Err(e) => {
      ui::system_error("Could not load project".into());
      ui::system_error(format!("{}", e));
    }
    Ok(project) => {
      let components = match tags {
        Some(t) => project
          .components
          .into_iter()
          .filter(|x| x.has_tag(&t.clone()))
          .collect(),
        None => project.components,
      };

      for component in components {
        shutdown_component_services(fname, &component.name)
      }
    }
  }
}

pub fn shutdown_component_services(fname: &PathBuf, component_name: &str) {
  match Project::load(&fname) {
    Err(e) => {
      ui::system_error("Could not load project".into());
      ui::system_error(format!("{}", e));
    }
    Ok(project) => {
      if let Some(c) = project.components.iter().find(|x| x.name == component_name) {
        for service_name in &c.services {
          if let Some(service) = project.service_by_name(&service_name) {
            let name = service.container.unwrap_or(service.name.clone());
            ui::system_message(format!("Stopping service {}", &service.name));
            if let Err(e) = stop_container(&name) {
              ui::system_error(format!("Could not stop service {}: {}", &service.name, e))
            }
          }
        }
      }
    }
  }
}
