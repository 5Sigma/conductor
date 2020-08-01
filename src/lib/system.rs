use crate::{ui, Component, Project};
#[cfg(not(target_os = "windows"))]
use rs_docker::Docker;
use std::io;
use std::io::prelude::*;
use std::io::{BufReader, Error, ErrorKind};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
pub enum ProjectError {
  Io(io::Error),
}

#[derive(Debug)]
pub struct SystemError {
  message: String,
}

impl SystemError {
  fn new(message: &str) -> Self {
    SystemError {
      message: message.into(),
    }
  }
}

impl std::fmt::Display for SystemError {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    write!(f, "{}", self.message)
  }
}

impl std::error::Error for SystemError {
  fn description(&self) -> &str {
    &self.message
  }
}

pub struct ComponentMessage {
  component: Component,
  body: String,
}

struct Worker {
  pub component: Component,
  pub handler: thread::JoinHandle<Result<(), SystemError>>,
  pub kill_signal: Sender<()>,
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
  Ok("".into())
}

#[cfg(target_os = "windows")]
pub fn stop_container(name: &str) -> io::Result<String> {
  ui::system_error("Services are not supported on windows".into());
  Ok("".into())
}

#[cfg(not(target_os = "windows"))]
pub fn get_buff_reader(out: std::process::ChildStdout) -> Box<dyn BufRead> {
  Box::new(BufReader::new(timeout_readwrite::TimeoutReader::new(
    out,
    Duration::new(1, 0),
  )))
}

#[cfg(target_os = "windows")]
pub fn get_buff_reader(out: std::process::ChildStdout) -> Box<dyn BufRead> {
  Box::new(BufReader::new(stdout));
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
  project: &Project,
  component: &Component,
  data_tx: Sender<ComponentMessage>,
  root_path: &PathBuf,
) -> Result<Worker, Error> {
  let data_tx = data_tx;
  let (tx, rx) = mpsc::channel();
  let c = component.clone();
  let rp = root_path.clone();

  for service_name in &c.services {
    if let Some(service) = project.service_by_name(&service_name.clone()) {
      match start_container(&service.container.unwrap_or(service.name.clone())) {
        Ok(_) => ui::system_message(format!("Started service {}", &service.name)),
        Err(e) => ui::system_error(format!("Error starting service {}: {}", &service.name, e)),
      }
    } else {
      ui::system_error(format!(
        "Could not find service definition: {}",
        service_name
      ))
    }
  }

  let th = thread::spawn(move || -> Result<(), SystemError> {
    loop {
      let mut cmd = create_command(&c.start, &c, &rp);
      let mut retry = c.retry;
      if let Some(delay) = c.delay {
        thread::sleep(Duration::from_secs(delay));
      }

      let mut child: std::process::Child = cmd
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| SystemError::new(&format!("Could not spawn process: {}", e)))?;

      let stdout = child
        .stdout
        .take()
        .ok_or_else(|| SystemError::new("Could not create process pipe"))?;

      // let buf = BufReader::new(stdout);
      let buf = get_buff_reader(stdout);
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
          match rx.try_recv() {
            Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
              retry = false;
              let _ = child.kill();
              Err(())
            }
            Err(mpsc::TryRecvError::Empty) => Ok(()),
          }
        }
        Err(_) => match rx.try_recv() {
          Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
            if let Err(e) = child.kill() {
              ui::system_error(format!("Could not kill process: {}", e));
            }
            retry = false;
            Err(())
          }
          Err(mpsc::TryRecvError::Empty) => Ok(()),
        },
      });

      ui::system_message(format!("Component shutdown: {}", &c.name));

      match rx.try_recv() {
        Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
          retry = false;
        }
        Err(mpsc::TryRecvError::Empty) => {}
      }

      if !retry {
        break Ok(());
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
    component: component.clone(),
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

pub fn run_project(fname: &PathBuf, tags: Option<Vec<&str>>) -> Result<(), SystemError> {
  let project = Project::load(&fname)
    .map_err(|e| SystemError::new(&format!("Failed to load project definition: {}", e)))?;
  let mut root_path = fname.clone();
  root_path.pop();

  let component_names: Vec<String> = match tags {
    Some(t) => project
      .components
      .into_iter()
      .filter(|x| x.has_tag(&t.clone()))
      .map(|x| x.name)
      .collect(),
    None => project.components.into_iter().map(|x| x.name).collect(),
  };
  run_components(fname, component_names)
}

pub fn run_component(fname: &PathBuf, component_name: &str) -> Result<(), SystemError> {
  let (tx, rx) = mpsc::channel();
  let mut fname = fname.clone();
  let project = Project::load(&fname)
    .map_err(|e| SystemError::new(&format!("Failed to load project definition: {}", e)))?;
  fname.pop();
  let c = project
    .components
    .iter()
    .find(|x| x.name == component_name)
    .ok_or(SystemError::new(&format!(
      "No component definition for {}",
      &component_name,
    )))?;

  ui::system_message(format!("Component start: {}", &component_name));
  let worker = spawn_component(&project, &c, tx, &fname)
    .map_err(|e| SystemError::new(&format!("Failed to spawn component: {}", e)))?;

  let ksig = worker.kill_signal.clone();
  ctrlc::set_handler(move || match ksig.send(()) {
    Ok(_) => {}
    Err(_) => {}
  })
  .expect("Could not setup handler");

  while let Ok(msg) = rx.recv() {
    ui::component_message(&msg.component, msg.body)
  }

  let _ = worker.handler.join().unwrap();
  ui::system_message(format!("Component shutdown: {}", &component_name));
  shutdown_component_services(&project, component_name);
  Ok(())
}

pub fn run_components(fname: &PathBuf, component_names: Vec<String>) -> Result<(), SystemError> {
  let running = Arc::new(AtomicBool::new(true));
  let (tx, rx) = mpsc::channel();
  let mut fname = fname.clone();
  let project = Project::load(&fname)
    .map_err(|e| SystemError::new(&format!("Failed to load project definition: {}", e)))?;
  fname.pop();
  let components: Vec<&Component> = project
    .components
    .iter()
    .filter(|c| component_names.contains(&c.name))
    .collect();

  if components.is_empty() {
    return Err(SystemError::new("No components to run"));
  }

  let workers: Vec<Worker> = components
    .iter()
    .map(|c| {
      ui::system_message(format!("Component start: {}", &c.name));
      match spawn_component(&project, c, tx.clone(), &fname) {
        Ok(w) => Some(w),
        Err(e) => {
          ui::system_error(format!("Could not start {}: {}", &c.name, e));
          None
        }
      }
    })
    .filter_map(Option::Some)
    .map(|c| c.unwrap())
    .collect();

  let signals: Vec<Sender<()>> = workers.iter().map(|w| w.kill_signal.clone()).collect();
  let r = running.clone();
  ctrlc::set_handler(move || {
    r.store(false, Ordering::SeqCst);
    for s in signals.to_owned() {
      match s.send(()) {
        Ok(_) => {}
        Err(_) => {}
      }
    }
  })
  .expect("Could not setup handler");

  while running.load(Ordering::SeqCst) {
    while let Ok(msg) = rx.recv_timeout(Duration::from_secs(1)) {
      ui::component_message(&msg.component, msg.body)
    }
  }

  for w in workers {
    let _ = w.handler.join().unwrap();
  }

  for c in components {
    shutdown_component_services(&project, &c.name)
  }
  Ok(())
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

pub fn shutdown_project_services(project: &Project, tags: Option<Vec<&str>>) {
  let components = match tags {
    Some(t) => project
      .clone()
      .components
      .into_iter()
      .filter(|x| x.has_tag(&t.clone()))
      .collect(),
    None => project.clone().components,
  };

  for component in components {
    shutdown_component_services(project, &component.name)
  }
}

pub fn shutdown_component_services(project: &Project, component_name: &str) {
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
