use crate::{Component, Project};
use crossbeam::channel::{after, unbounded, Receiver, Select, Sender};
use log::{debug, info};
use std::collections::HashMap;
use std::io::prelude::*;
use std::io::BufReader;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct Supervisor {
  workers: Arc<Mutex<Vec<Worker>>>,
  project: Project,
}

impl Supervisor {
  pub fn new(project: &Project) -> Self {
    Supervisor {
      workers: Arc::new(Mutex::new(vec![])),
      project: project.clone(),
    }
  }

  pub fn spawn_component_by_name(&self, name: &str) {
    if let Some(c) = self.project.find_component(name) {
      self.spawn_component(c);
    }
  }

  pub fn spawn_component(&self, component: &Component) {
    let (data_sender, data_receiver) = unbounded();
    let (kill_tx, kill_rx) = unbounded();
    let worker = Worker {
      project: self.project.clone(),
      running: true,
      completed: false,
      component: component.clone(),
      data_receiver,
      kill_signal: kill_tx,
    };

    for service_name in component.services.iter() {
      if let Some(service) = self.project.service_by_name(service_name) {
        match service.start() {
          Ok(_) => {
            let _ = data_sender.send(ComponentEvent::service_start(
              component.clone(),
              service_name.clone(),
            ));
          }
          Err(e) => {
            let _ = data_sender.send(ComponentEvent::error(
              component.clone(),
              format!("Could not start service [{}]: {}", service_name, e),
            ));
          }
        };
      }
    }

    let component = component.clone();
    let root_path = self.project.root_path.clone();

    thread::spawn(move || {
      if let Some(delay) = component.delay {
        thread::sleep(Duration::from_secs(delay));
      }

      // Build the start command

      let mut cmd = std::process::Command::new(expand_env(&component.start.command));
      // Setup the environment variables
      let mut env: HashMap<String, String> = HashMap::new();
      env.extend(component.env.clone());
      env.extend(component.start.env.clone());
      for (k, v) in env {
        cmd.env(k, expand_env(&v));
      }

      // Process command arguments
      for a in &component.start.args {
        cmd.arg(expand_env(a));
      }

      let dir = component
        .clone()
        .start
        .dir
        .unwrap_or_else(|| component.get_path().to_str().unwrap_or("").into());
      let mut root_path = root_path.clone();
      root_path.push(expand_env(&dir));
      cmd.current_dir(root_path);

      // setup STDOUT and STDERR pipes

      let (reader, writer) = match os_pipe::pipe() {
        Ok((reader, writer)) => (reader, writer),
        Err(e) => {
          let _ = data_sender.send(ComponentEvent::error(
            component.clone(),
            format!("Could not open pipes: {}", e),
          ));
          return;
        }
      };
      let writer_clone = match writer.try_clone() {
        Ok(w) => w,
        Err(e) => {
          let _ = data_sender.send(ComponentEvent::error(
            component.clone(),
            format!("Could not open pipes: {}", e),
          ));
          return;
        }
      };

      let mut child: std::process::Child = match cmd.stdout(writer).stderr(writer_clone).spawn() {
        Ok(c) => c,
        Err(e) => {
          let _ = data_sender.send(ComponentEvent::error(
            component.clone(),
            format!("Could not spawn process: {}", e),
          ));
          return;
        }
      };

      let _ = data_sender.send(ComponentEvent::start(component.clone()));
      let buf = get_buff_reader(reader);
      let _ = buf.lines().try_for_each(|line| match line {
        Ok(body) => {
          let _ = data_sender.send(ComponentEvent::output(component.clone(), body));
          Ok(())
        }
        Err(_) => {
          if let Ok(None) = child.try_wait() {
            // Timeout occured while writing. Need to somehow figure out how to detect the child has excited here.
            Ok(())
          } else {
            match kill_rx.try_recv() {
              Ok(_) => Err(()),
              Err(_) => Ok(()),
            }
          }
        }
      });
    });

    let mut workers = self.workers.lock().unwrap();
    workers.push(worker);
  }

  pub fn init(&self) {
    let workers_lock = Arc::clone(&self.workers);
    let th = std::thread::spawn(move || loop {
      let mut workers = workers_lock.lock().unwrap();

      // If there are workers present and all of them have completed we can
      // hault.
      if workers.len() > 0 && workers.iter().all(|i| i.completed) {
        break;
      }

      // If no workers have been added and or there are no workers currently running
      // we should sleep for moment and wait for a worker to get added to the pool.
      // This assumes init was called before a worker was spawned.
      if workers.len() == 0 || !workers.iter().any(|i| i.running) {
        thread::sleep(Duration::from_millis(500));
        drop(workers);
        continue;
      }

      // Get a list of all workers that are currently running.
      let mut running_workers = workers
        .iter_mut()
        .filter(|i| i.running)
        .collect::<Vec<&mut Worker>>();
      // build up a selector for all the receivers on the running workers.
      let mut sel = Select::new();
      for w in running_workers.iter() {
        sel.recv(&w.data_receiver);
      }
      let timeout = after(Duration::from_millis(500));
      sel.recv(&timeout);
      // select for a message from one of the workers that has an available message
      let oper = sel.select();
      let index = oper.index();
      if index == running_workers.len() {
        let _ = oper.recv(&timeout);
        debug!("Timeout reading from worker");
        drop(workers);
        continue;
      }

      match oper.recv(&running_workers[index].data_receiver) {
        Ok(msg) => match msg.body {
          ComponentEventBody::Output { body } => {
            crate::ui::component_message(&workers[index].component, body)
          }
          ComponentEventBody::ComponentStart => {
            crate::ui::system_message(format!("Component started {}", msg.component.name))
          }
          ComponentEventBody::ComponentError { body } => crate::ui::system_error(format!(
            "Component error [{}]: {}",
            msg.component.name, body
          )),
          ComponentEventBody::ServiceStart { service_name } => {
            crate::ui::system_message(format!("Service started {}", service_name))
          }
        },
        Err(_) => {
          // The worker's data channel erorred/closed mark this worker as no longer running.
          crate::ui::system_message(format!(
            "Component shutdown {}",
            running_workers[index].component.name
          ));
          running_workers[index].running = false;
          running_workers[index].completed = true;
        }
      };
    });

    let workers_lock = Arc::clone(&self.workers);
    let _ = ctrlc::set_handler(move || {
      crate::ui::system_message("shutting down".into());
      info!("ctrl-c signal caught");
      let workers = workers_lock.lock().unwrap();
      for w in workers.iter() {
        if w.running {
          info!("sending kill signal");
          let _ = w.kill_signal.send(());
        }
      }
    });

    // Join the init thread.
    let _ = th.join();
    let workers = self.workers.lock().unwrap();
    for worker in workers.iter() {
      for service_name in worker.component.services.iter() {
        if let Some(service) = self.project.service_by_name(service_name) {
          let _ = service.stop();
        }
        crate::ui::system_message(format!("Service stopped {}", service_name))
      }
    }
  }
}

struct Worker {
  pub project: Project,
  pub kill_signal: Sender<()>,
  pub running: bool,
  pub completed: bool,
  pub component: Component,
  pub data_receiver: Receiver<ComponentEvent>,
}

#[derive(Debug, PartialEq)]
enum ComponentEventBody {
  Output { body: String },
  ComponentStart,
  // ComponentShutdown,
  ServiceStart { service_name: String },
  // ServiceShutdown { service_name: String },
  ComponentError { body: String },
}

/// Used to send events from a running component. Holds a copy of the component itself as well
/// as the event that occured.
#[derive(Debug, PartialEq)]
struct ComponentEvent {
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
  // pub fn shutdown(component: Component) -> Self {
  //   ComponentEvent {
  //     component,
  //     body: ComponentEventBody::ComponentShutdown,
  //   }
  // }
  pub fn service_start(component: Component, service_name: String) -> Self {
    ComponentEvent {
      component,
      body: ComponentEventBody::ServiceStart { service_name },
    }
  }
  // pub fn service_shutdown(component: Component, service_name: String) -> Self {
  //   ComponentEvent {
  //     component,
  //     body: ComponentEventBody::ServiceShutdown { service_name },
  //   }
  // }
}

#[cfg(not(target_os = "windows"))]
pub fn get_buff_reader(out: os_pipe::PipeReader) -> Box<dyn BufRead> {
  Box::new(BufReader::new(timeout_readwrite::TimeoutReader::new(
    out,
    Duration::new(1, 0),
  )))
}

#[cfg(target_os = "windows")]
pub fn get_buff_reader(out: os_pipe::PipeReader) -> Box<dyn BufRead> {
  Box::new(BufReader::new(out))
}

/// Expands a string using environment variables.
/// Environment variables are detected as %VAR% and replaced with the coorisponding
/// environment variable value
fn expand_env(str: &str) -> String {
  expand_str::expand_string_with_env(str).unwrap_or_else(|_| str.to_string())
}
