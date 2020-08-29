use crate::task::Task;
use crate::{ui, Component, Project};
use crossbeam::channel::{after, unbounded, Receiver, Select, Sender};
use log::{debug, info, warn};
use std::collections::{HashMap, HashSet};
use std::io::prelude::*;
use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use subprocess::{Exec, Popen, Redirection};

struct ReadOutAdapter(Arc<Mutex<Popen>>);

impl Read for ReadOutAdapter {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    self.0.lock().unwrap().stdout.as_mut().unwrap().read(buf)
  }
}

/// Supervisor controls the exection of tasks and components. It handles launching them,
/// tracking them, relaunching them on failure, and managing all the reading threads.
pub struct Supervisor {
  workers: Arc<Mutex<Vec<Worker>>>,
  project: Project,
}

impl Supervisor {
  /// Sets up a new supervisor instance.
  pub fn new(project: &Project) -> Self {
    Supervisor {
      workers: Arc::new(Mutex::new(vec![])),
      project: project.clone(),
    }
  }

  /// Returns an iterator that will run all services that a component depends on.
  pub fn run_component_services(&self, component: &Component) -> crate::service::ServiceLauncher {
    let services = component
      .services
      .iter()
      .map(|sn| self.project.service_by_name(sn))
      .flatten()
      .collect();
    crate::service::ServiceLauncher::new(services)
  }

  /// Returns an iterator that will run all services that a component depends on.
  pub fn shutdown_component_services(
    &self,
    component: &Component,
  ) -> crate::service::ServiceTerminator {
    let services = component
      .services
      .iter()
      .map(|sn| self.project.service_by_name(sn))
      .flatten()
      .collect();
    crate::service::ServiceTerminator::new(services)
  }

  /// Runs a single command for a task. This is a blocking operation
  /// tasks are not run in parallel.
  pub fn run_task_command(&self, task: &Task, cmd: String) {
    let mut root_path = self.project.root_path.clone();
    root_path.push(expand_env(task.path.to_str().unwrap()));
    let mut env: HashMap<_, _> = std::env::vars().collect();
    env.extend(task.env.clone());
    let env_vars: Vec<(String, String)> =
      env.into_iter().map(|(k, v)| (k, expand_env(&v))).collect();
    ui::system_message(cmd.clone());
    let stream = Exec::shell(cmd)
      .env_extend(&env_vars[..])
      .cwd(root_path)
      .stdout(Redirection::Pipe)
      .stderr(Redirection::Merge)
      .stream_stdout()
      .unwrap();

    let reader = BufReader::new(stream);
    let _ = reader.lines().for_each(|line| {
      if let Ok(body) = line {
        ui::task_message(&task, body);
      }
    });
  }

  /// Spawns a component by creating a shell and running its start command. Sets up a thread
  /// for reading the output and a thred for minitoring for kill signals.
  /// This also creates a worker instance and sets up the pipeline for events to be read from
  /// Supervisor::init()
  pub fn spawn_component(&self, component: &Component, extra_env: HashMap<String, String>) {
    let (data_sender, data_receiver) = unbounded();
    let (kill_tx, kill_rx) = unbounded();
    let worker = Worker {
      project: self.project.clone(),
      extra_env: extra_env.clone(),
      running: true,
      completed: false,
      component: component.clone(),
      data_receiver,
      kill_signal: kill_tx,
    };

    for service in self.run_component_services(component) {
      match service {
        Ok(service) => {
          let _ = data_sender.send(ComponentEvent::service_start(
            component.clone(),
            service.name.clone(),
          ));
        }
        Err((service, e)) => {
          let _ = data_sender.send(ComponentEvent::error(
            component.clone(),
            format!("Could not start service [{}]: {}", service.name, e),
          ));
        }
      }
    }

    let component = component.clone();
    let mut root_path = self.project.root_path.clone();
    info!("starting spawn thread for {}", &component.name);
    thread::spawn(move || {
      if let Some(delay) = component.delay {
        thread::sleep(Duration::from_secs(delay));
      }

      // Setup the environment variables
      let mut env: HashMap<_, _> = std::env::vars().collect();
      env.extend(component.env.clone());
      env.extend(extra_env);
      let env_vars: Vec<(String, String)> =
        env.into_iter().map(|(k, v)| (k, expand_env(&v))).collect();
      root_path.push(expand_env(component.get_path().to_str().unwrap()));
      // Create the execution command and shell
      let exec = Exec::shell(component.start.clone())
        .env_extend(&env_vars[..])
        .cwd(root_path)
        .stdout(Redirection::Pipe)
        .stderr(Redirection::Merge);

      // Execute the process and return a popen. This goes into an Arc and a mutex so the
      // kill signal can poll and kill, while we pass the reading stream into a seperate thread.
      //  We also setup a stream adapter and a bufreader to read out the data from the reading thread.
      let popen = Arc::new(Mutex::new(exec.popen().unwrap()));
      let stream = ReadOutAdapter(Arc::clone(&popen));
      let _ = data_sender.send(ComponentEvent::start(component.clone()));
      let reader = BufReader::new(stream);

      let sender = data_sender.clone();
      let cmp = component.clone();
      // spawn the reading thread that will read the stdout of the process until the popen goes out of scope
      // which occures either as a result of the process exiting or the kill signal being received.
      std::thread::spawn(move || {
        let c = cmp.clone();
        let _ = reader.lines().for_each(|line| {
          if let Ok(body) = line {
            let _ = sender.send(ComponentEvent::output(c.clone(), body));
          } else {
            warn!("Error reading from reader");
          }
        });
      });

      loop {
        thread::sleep(Duration::from_millis(200));
        let mut p = popen.lock().unwrap();
        if let Ok(Some(_)) = p.wait_timeout(Duration::new(0, 0)) {
          if !component.keep_alive {
            info!("Component has exited");
            break;
          }
        }
        if let Ok(()) = kill_rx.try_recv() {
          info!("killing process");
          break;
        }
      }
      let mut p = popen.lock().unwrap();
      let _ = p.kill();
      info!("ending read loop");
      let _ = data_sender.send(ComponentEvent::shutdown(component.clone()));
    });

    let workers = &mut self.workers.lock().unwrap();
    workers.push(worker);
  }

  /// Starts the main run loop for the launched components.
  /// Begins a blocking read of all events comming from all components and outputing them through
  /// the ui module. Retriable components will also be relaunched here.
  pub fn init(&self) {
    let workers_lock = Arc::clone(&self.workers);
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let _ = ctrlc::set_handler(move || {
      r.store(false, Ordering::SeqCst);
      crate::ui::system_message("shutting down".into());
      info!("ctrl-c signal caught");
      let mut workers = workers_lock.lock().unwrap();
      for w in workers.iter_mut() {
        w.completed = true;
        if w.running {
          info!("sending kill signal");
          let _ = w.kill_signal.send(());
        }
      }
      drop(workers);
    });

    let workers_lock = Arc::clone(&self.workers);
    loop {
      let mut workers = workers_lock.lock().unwrap();

      // If there are workers present and all of them have completed we can
      // hault.
      if workers.len() > 0 && workers.iter().all(|i| i.completed) {
        drop(workers);
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
            crate::ui::system_message(format!(
              "Component started [{}] {}",
              workers.len(),
              msg.component.name
            ));
            debug!(
              "Current workers: {:?}",
              workers
                .iter()
                .map(|w| w.component.name.clone())
                .collect::<Vec<String>>()
            );
          }
          ComponentEventBody::ComponentError { body } => crate::ui::system_error(format!(
            "Component error [{}]: {}",
            msg.component.name, body
          )),
          ComponentEventBody::ServiceStart { service_name } => {
            crate::ui::system_message(format!("Service started {}", service_name))
          }
          ComponentEventBody::ComponentShutdown => {
            crate::ui::system_message(format!("Component shutdown {}", msg.component.name));
            if msg.component.retry && !running_workers[index].completed {
              info!("component {} as retry enabled", &msg.component.name);
              // We need to drop workers here to release the lock because spawn_component will attempt to
              // get a lock.
              let extra_env = running_workers[index].extra_env.clone();
              drop(workers);
              if running.load(Ordering::SeqCst) {
                self.spawn_component(&msg.component.clone(), extra_env);
              }
              continue;
            } else {
              info!("component {} has completed", &msg.component.name);
              running_workers[index].completed = true;
            }
          }
        },
        Err(_) => {
          // The worker's data channel erorred/closed mark this worker as no longer running.
          info!("channel closed marking worker complete");
          running_workers[index].running = false;
          running_workers[index].completed = true;
          let _ = running_workers[index].kill_signal.send(());
        }
      };
    }

    // Using a hash set here to get unique service names so we
    // shutdown each one once.
    let mut services = HashSet::new();
    let workers = self.workers.lock().unwrap();
    for worker in workers.iter() {
      for service_name in worker.component.services.iter() {
        services.insert(service_name);
      }
    }
    for service_name in services {
      if let Some(service) = self.project.service_by_name(service_name) {
        let _ = service.stop();
      }
      crate::ui::system_message(format!("Service stopped {}", service_name))
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
  pub extra_env: HashMap<String, String>,
}

#[derive(Debug, PartialEq)]
enum ComponentEventBody {
  Output { body: String },
  ComponentStart,
  ComponentShutdown,
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
}

/// Expands a string using environment variables.
/// Environment variables are detected as %VAR% and replaced with the coorisponding
/// environment variable value
fn expand_env(str: &str) -> String {
  expand_str::expand_string_with_env(str).unwrap_or_else(|_| str.to_string())
}
