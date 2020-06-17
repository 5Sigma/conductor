use crate::{ui, Component, Project};

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
  let mut cmd = Command::new(&c.command);
  c.args.iter().for_each(|a| {
    cmd.arg(a);
  });
  for (k, v) in &c.env {
    cmd.env(k, v);
  }
  for (k, v) in &component.env {
    cmd.env(k, v);
  }
  let dir = component
    .clone()
    .start
    .dir
    .unwrap_or_else(|| component.get_path().to_str().unwrap_or("").into());
  let mut root_path = root_path.clone();
  root_path.push(dir);
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
          // let buf = BufReader::new(TimeoutReader::new(stdout, Duration::new(1, 0)));
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
    .and_then(|stdout| Ok(BufReader::new(stdout).lines()))
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
      let (tx, rx) = mpsc::channel();
      let mut root_path = fname.clone();
      root_path.pop();

      let components = match tags {
        Some(t) => project
          .components
          .into_iter()
          .filter({ |x| x.has_tag(&t.clone()) })
          .collect(),
        None => project.components,
      };

      for c in components.iter() {
        match spawn_component(c.clone(), tx.clone(), &root_path) {
          Ok(_) => ui::system_message(format!("Started {}", c.name)),
          Err(e) => ui::system_error(format!("Failed to start {}: {}", c.name, e)),
        }
      }

      loop {
        let msg = rx.recv().unwrap();
        ui::component_message(&msg.component, msg.body);
      }
    }
  };
}

pub fn run_component(fname: &PathBuf, component_name: &str) {
  let mut fname = fname.clone();
  match Project::load(&fname) {
    Err(e) => {
      ui::system_error("Could not load project".into());
      println!("{}", e)
    }
    Ok(project) => {
      let (tx, rx) = mpsc::channel();
      fname.pop();

      if let Some(c) = project
        .components
        .into_iter()
        .find(|x| x.name == component_name)
      {
        match spawn_component(c.clone(), tx, &fname) {
          Ok(_) => ui::system_message(format!("Started {}", c.name)),
          Err(e) => ui::system_error(format!("Failed to start {}: {}", c.name, e)),
        }
        loop {
          let msg = rx.recv().unwrap();
          ui::component_message(&msg.component, msg.body);
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
