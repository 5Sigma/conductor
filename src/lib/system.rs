use crate::{ui, Component, Project};
use crossbeam::channel::{unbounded, Sender};
#[cfg(not(target_os = "windows"))]
use rs_docker::Docker;
use std::collections::HashMap;
use std::io;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// The general error returned by various functions in the system module.
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

#[derive(Debug, PartialEq)]
pub enum ComponentEventBody {
  Output { body: String },
  ComponentStart,
  ComponentShutdown,
  ServiceStart { service_name: String },
  ServiceShutdown { service_name: String },
  ComponentError { body: String },
}

/// Used to send events from a running component. Holds a copy of the component itself as well
/// as the event that occured.
#[derive(Debug, PartialEq)]
pub struct ComponentEvent {
  pub component: Component,
  pub body: ComponentEventBody,
}

impl ComponentEvent {
  pub fn output(component: Component, body: String) -> Self {
    ComponentEvent {
      component,
      body: ComponentEventBody::Output { body },
    }
  }
  pub fn error(component: Component, body: String) -> Self {
    ComponentEvent {
      component,
      body: ComponentEventBody::ComponentError { body },
    }
  }
  pub fn start(component: Component) -> Self {
    ComponentEvent {
      component,
      body: ComponentEventBody::ComponentStart,
    }
  }
  pub fn shutdown(component: Component) -> Self {
    ComponentEvent {
      component,
      body: ComponentEventBody::ComponentShutdown,
    }
  }
  pub fn service_start(component: Component, service_name: String) -> Self {
    ComponentEvent {
      component,
      body: ComponentEventBody::ServiceStart { service_name },
    }
  }
  pub fn service_shutdown(component: Component, service_name: String) -> Self {
    ComponentEvent {
      component,
      body: ComponentEventBody::ServiceShutdown { service_name },
    }
  }
}

/// Worker is used to encapsulate a reference to a running component.
/// It holds a reference to the component, the thread handler, and the kill_signal
/// that is used to allow the component to exit.
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

/// Expands a string using environment variables.
/// Environment variables are detected as %VAR% and replaced with the coorisponding
/// environment variable value
fn expand_env(str: &str) -> String {
  expand_str::expand_string_with_env(str).unwrap_or_else(|_| str.to_string())
}

/// Converts a conductor::Command to a process::Command for a given component. Additional environment variables can also
/// be passed in. These are used to override any existing variables from the Component or the Command.
fn create_command(
  c: &crate::Command,
  component: &Component,
  root_path: &PathBuf,
  extra_env: HashMap<String, String>,
) -> Command {
  let mut cmd = Command::new(expand_env(&c.command));
  let mut env: HashMap<String, String> = HashMap::new();

  env.extend(component.env.clone());
  env.extend(c.env.clone());
  env.extend(extra_env);

  for (k, v) in env {
    cmd.env(k, expand_env(&v));
  }

  for a in &c.args {
    cmd.arg(expand_env(a));
  }

  let dir = component
    .clone()
    .path
    .unwrap_or_else(|| component.get_path().to_str().unwrap_or("").into());
  let mut root_path = root_path.clone();
  root_path.push(expand_env(&dir));
  cmd.current_dir(root_path);
  cmd
}

// fn spawn_component(
//   project: &Project,
//   component: &Component,
//   data_tx: Sender<ComponentEvent>,
//   root_path: &PathBuf,
//   env: HashMap<String, String>,
// ) -> Result<Worker, SystemError> {
//   let data_tx = data_tx;
//   let (tx, rx) = unbounded();
//   let c = component.clone();
//   let rp = root_path.clone();

//   for service_name in &c.services {
//     if let Some(service) = project.service_by_name(&service_name.clone()) {
//       match start_container(&service.get_container_name()) {
//         Ok(_) => {
//           let _ = data_tx.send(ComponentEvent::service_start(
//             c.clone(),
//             service_name.clone(),
//           ));
//         }
//         Err(e) => {
//           let _ = data_tx.send(ComponentEvent::error(
//             c.clone(),
//             format!("Error starting service {}: {}", &service.name, e),
//           ));
//         }
//       };
//     } else {
//       let _ = data_tx.send(ComponentEvent::error(
//         c.clone(),
//         format!("Could not find service definition: {}", service_name),
//       ));
//     }
//   }

//   let th = thread::spawn(move || -> Result<(), SystemError> {
//     loop {
//       let mut retry = c.retry;
//       if let Some(delay) = c.delay {
//         thread::sleep(Duration::from_secs(delay));
//       }

//       let mut cmd = create_command(&c.start, &c, &rp, env.to_owned());
//       let (reader, writer) =
//         os_pipe::pipe().map_err(|_| SystemError::new("Could not open stdout/err pipe"))?;
//       let writer_clone = writer
//         .try_clone()
//         .map_err(|_| SystemError::new("Could not clone output pipe"))?;
//       let mut child: std::process::Child = cmd
//         .stdout(writer)
//         .stderr(writer_clone)
//         .spawn()
//         .map_err(|e| SystemError::new(&format!("Could not spawn process: {}", e)))?;

//       let _ = data_tx.send(ComponentEvent::start(c.clone()));

//       let buf = get_buff_reader(reader);
//       let _ = buf.lines().try_for_each(|line| match line {
//         Ok(body) => {
//           let payload = ComponentEvent::output(c.clone(), body);
//           if let Err(e) = data_tx.send(payload) {
//             println!("Error sending output: {}", e)
//           }
//           match rx.try_recv() {
//             Ok(_) | Err(crossbeam::channel::TryRecvError::Disconnected) => {
//               retry = false;
//               let _ = child.kill();
//               Err(())
//             }
//             Err(crossbeam::channel::TryRecvError::Empty) => Ok(()),
//           }
//         }
//         Err(_) => match rx.try_recv() {
//           Ok(_) | Err(crossbeam::channel::TryRecvError::Disconnected) => {
//             if let Err(e) = child.kill() {
//               let _ = data_tx.send(ComponentEvent::error(
//                 c.clone(),
//                 format!("Could not kill process: {}", e),
//               ));
//             }
//             retry = false;
//             Err(())
//           }
//           Err(crossbeam::channel::TryRecvError::Empty) => Ok(()),
//         },
//       });

//       drop(child);
//       let _ = data_tx.send(ComponentEvent::shutdown(c.clone()));

//       match rx.try_recv() {
//         Ok(_) | Err(crossbeam::channel::TryRecvError::Disconnected) => {
//           retry = false;
//         }
//         Err(crossbeam::channel::TryRecvError::Empty) => {}
//       }
//       if !retry {
//         break Ok(());
//       }
//       thread::sleep(Duration::from_secs(c.delay.unwrap_or(1)));
//     }
//   });

//   Ok(Worker {
//     kill_signal: tx,
//     component: component.clone(),
//     handler: th,
//   })
// }

pub fn run_command(c: &crate::Command, cmp: &Component, root_path: &PathBuf) {
  let mut cmd = create_command(c, cmp, &root_path, HashMap::new());
  ui::system_message(format!("Executing: {}", c));

  match cmd
    .stdout(Stdio::piped())
    .spawn()
    .map_err(|e| SystemError::new(&format!("Error spawning process: {}", e)))
    .and_then(|mut child| {
      child
        .stdout
        .take()
        .ok_or_else(|| SystemError::new("Could not create output pipe."))
    })
    .map_err(|e| SystemError::new(&format!("Could not spawn child: {}", e)))
    .map(|stdout| BufReader::new(stdout).lines())
  {
    Ok(lines) => lines.for_each(|line| {
      ui::component_message(cmp, line.unwrap());
    }),
    Err(e) => {
      ui::system_error("Command Error".into());
      ui::system_error(format!("{}", e));
    }
  }
}

// /// runs components defined in the project. If no tags are specified all default components are executed.
// /// if a set of tags are specified only those components which contain those tags are executed.
// pub fn run_project(fname: &PathBuf, tags: Option<Vec<&str>>) -> Result<(), SystemError> {
//   let project = Project::load(&fname)
//     .map_err(|e| SystemError::new(&format!("Failed to load project definition: {}", e)))?;
//   let mut root_path = fname.clone();
//   root_path.pop();

//   let has_tags = tags.is_some();
//   let component_names: Vec<String> = match tags {
//     Some(t) => project
//       .clone()
//       .components
//       .into_iter()
//       .filter(|x| x.has_tags(&t.clone()) || (!has_tags && x.default))
//       .map(|x| x.name)
//       .collect(),
//     None => project
//       .clone()
//       .components
//       .into_iter()
//       .filter(|x| x.default)
//       .map(|x| x.name)
//       .collect(),
//   };
//   run_components(&project, &root_path, component_names, HashMap::new())
// }

// /// runs one or more components by name. An additional set of environment variables can
// /// be passed in that will override any existing component or command environment settings.
// pub fn run_components(
//   project: &Project,
//   root_path: &PathBuf,
//   component_names: Vec<String>,
//   env: HashMap<String, String>,
// ) -> Result<(), SystemError> {
//   let running = Arc::new(AtomicBool::new(true));
//   let (tx, rx) = unbounded();
//   let components: Vec<&Component> = project
//     .components
//     .iter()
//     .filter(|c| component_names.contains(&c.name))
//     .collect();
//   if components.is_empty() {
//     return Err(SystemError::new("No components to run"));
//   }

//   let workers: Vec<Worker> = components
//     .iter()
//     .map(
//       |c| match spawn_component(&project, c, tx.clone(), root_path, env.clone()) {
//         Ok(w) => Some(w),
//         Err(e) => {
//           ui::system_error(format!("Could not start {}: {}", &c.name, e));
//           None
//         }
//       },
//     )
//     .filter_map(Option::Some)
//     .map(|c| c.unwrap())
//     .collect();

//   let signals: Vec<Sender<()>> = workers.iter().map(|w| w.kill_signal.clone()).collect();
//   let r = running.clone();
//   ctrlc::set_handler(move || {
//     r.store(false, Ordering::SeqCst);
//     for s in signals.to_owned() {
//       let _ = s.send(());
//     }
//   })
//   .expect("Could not setup handler");

//   while running.load(Ordering::SeqCst) {
//     while let Ok(msg) = rx.recv_timeout(Duration::from_secs(1)) {
//       match msg.body {
//         ComponentEventBody::Output { body } => ui::component_message(&msg.component, body),
//         ComponentEventBody::ServiceStart { service_name } => ui::system_message(format!(
//           "Starting service: {} for {}",
//           service_name, &msg.component.name
//         )),
//         ComponentEventBody::ServiceShutdown { service_name } => ui::system_message(format!(
//           "Shutting down service: {} for {}",
//           service_name, &msg.component.name
//         )),
//         ComponentEventBody::ComponentError { body } => ui::system_error(format!(
//           "Component error [{}]: {}",
//           &msg.component.name, body
//         )),
//         ComponentEventBody::ComponentStart => {
//           ui::system_message(format!("Component started: {}", &msg.component.name))
//         }
//         ComponentEventBody::ComponentShutdown => {
//           ui::system_message(format!("Component shutdown: {}", &msg.component.name))
//         }
//       }
//     }
//   }

//   for w in workers {
//     let _ = w.handler.join().unwrap();
//   }

//   for c in components {
//     shutdown_component_services(&project, &c.name)
//   }
//   Ok(())
// }

/// sets up a project from scratch. Clones the specfied repos for all components and runs all init commands.
pub fn setup_project(project: &Project, root_path: &PathBuf) {
  for cmp in project.components.iter() {
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

// /// returns a list of all components in the project.
// pub fn get_components(fname: &PathBuf) -> Vec<Component> {
//   match Project::load(&fname) {
//     Err(_) => {
//       ui::system_error("Could not load project".into());
//       vec![]
//     }
//     Ok(project) => project.components,
//   }
// }

// Shuts down all services defined in the project. If tags are passed in only services
// used by components with those tags will be shutdown.
// pub fn shutdown_project_services(project: &Project, tags: Option<Vec<&str>>) {
//   let components = match tags {
//     Some(t) => project
//       .clone()
//       .components
//       .into_iter()
//       .filter(|x| x.has_tags(&t.clone()))
//       .collect(),
//     None => project.clone().components,
//   };

//   for component in components {
//     shutdown_component_services(project, &component.name)
//   }
// }

// /// Shuts down all services used by a component.
// pub fn shutdown_component_services(project: &Project, component_name: &str) {
//   if let Some(c) = project.components.iter().find(|x| x.name == component_name) {
//     for service_name in &c.services {
//       if let Some(service) = project.service_by_name(&service_name) {
//         let name = service.get_container_name();
//         ui::system_message(format!("Stopping service {}", &name));
//         if let Err(e) = stop_container(&name) {
//           ui::system_error(format!("Could not stop service {}: {}", &service.name, e))
//         }
//       }
//     }
//   }
// }

// #[cfg(test)]
// mod test {
//   use super::spawn_component;
//   use super::{ComponentEvent, ComponentEventBody};
//   use crate::{Command, Component, Project};
//   use std::collections::HashMap;
//   use std::path::PathBuf;

//   #[cfg(not(target_os = "windows"))]
//   fn env_str(e: &str) -> String {
//     format!("${}", e)
//   }

//   #[cfg(target_os = "windows")]
//   fn env_str(e: &str) -> String {
//     format!("%{}%", e)
//   }

//   #[cfg(target_os = "windows")]
//   fn create_echo_command(echo: &str) -> Command {
//     Command {
//       command: "cmd".into(),
//       args: vec!["/C".into(), format!("echo {}", echo).into()],
//       ..Command::default()
//     }
//   }

//   #[cfg(not(target_os = "windows"))]
//   fn create_echo_command(echo: &str) -> Command {
//     Command {
//       command: "sh".into(),
//       args: vec!["-c".into(), format!("echo {}", echo).into()],
//       ..Command::default()
//     }
//   }
//   fn create_component(name: &str, cmd: Command) -> Component {
//     Component {
//       name: name.into(),
//       path: Some("./".into()),
//       start: cmd,
//       ..Component::default()
//     }
//   }

//   fn create_project() -> Project {
//     Project {
//       components: vec![create_component(
//         "testcomponent",
//         create_echo_command("some echo"),
//       )],
//       ..Project::default()
//     }
//   }

//   fn get_output(events: Vec<ComponentEvent>) -> Vec<String> {
//     events
//       .into_iter()
//       .map(|i| {
//         if let ComponentEventBody::Output { body } = &i.body {
//           body.clone()
//         } else {
//           "".into()
//         }
//       })
//       .filter(|i| i != "")
//       .collect()
//   }

//   #[test]
//   fn test_component_level_env_expansion() {
//     let (tx, rx) = crossbeam::channel::unbounded();
//     let mut project = create_project();
//     let mut env = HashMap::new();
//     let root_path: PathBuf = ".".into();
//     std::env::set_var("env1", "one");
//     env.insert("env2".into(), "%env1%two".into());
//     project.components[0].env = env;
//     project.components[0].start = create_echo_command(&env_str("env2"));

//     let _ = spawn_component(
//       &project,
//       &project.components[0],
//       tx,
//       &root_path,
//       HashMap::new(),
//     );

//     let msgs: Vec<ComponentEvent> = rx.iter().collect();
//     assert_eq!(msgs.len(), 3);
//     let lines = get_output(msgs);
//     assert_eq!(lines.len(), 1);
//     assert_eq!(lines[0], "onetwo");
//   }

//   #[test]
//   fn test_component_level_env() {
//     let (tx, rx) = crossbeam::channel::unbounded();
//     let mut project = create_project();
//     let mut env = HashMap::new();
//     let root_path: PathBuf = ".".into();
//     env.insert("env1".into(), "one".into());
//     env.insert("env2".into(), "two".into());
//     project.components[0].env = env;
//     project.components[0].start = create_echo_command(&env_str("env1"));

//     let _ = spawn_component(
//       &project,
//       &project.components[0],
//       tx,
//       &root_path,
//       HashMap::new(),
//     );

//     let msgs: Vec<ComponentEvent> = rx.iter().collect();
//     assert_eq!(msgs.len(), 3);
//     let lines = get_output(msgs);
//     assert_eq!(lines.len(), 1);
//     assert_eq!(lines[0], "one");
//   }

//   #[test]
//   fn test_spawn_component() -> Result<(), Box<dyn std::error::Error>> {
//     let project = Project {
//       components: vec![create_component(
//         "testcomponent",
//         create_echo_command("some echo"),
//       )],
//       ..Project::default()
//     };
//     let root_path: PathBuf = ".".into();
//     let (tx, rx) = crossbeam::channel::unbounded();
//     let _ = spawn_component(
//       &project,
//       &project.components[0],
//       tx,
//       &root_path,
//       HashMap::new(),
//     )?;

//     let msgs: Vec<ComponentEvent> = rx.iter().collect();
//     assert_eq!(msgs.len(), 3);
//     let lines = get_output(msgs);
//     assert_eq!(lines.len(), 1);
//     assert_eq!(lines[0], "some echo");

//     Ok(())
//   }
// }
