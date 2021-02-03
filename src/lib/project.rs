use crate::supervisor::{SupervisedCommand, Supervisor};
use crate::task::Task;
use crate::Component;
use crate::Group;
use crate::Service;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

#[derive(Deserialize, PartialEq, Clone)]
#[serde(default)]
pub struct Project {
  pub name: String,
  pub components: Vec<Component>,
  pub groups: Vec<Group>,
  pub services: Vec<Service>,
  pub tasks: Vec<Task>,
  pub root_path: PathBuf,
  pub env: HashMap<String, String>,
}

impl Project {
  pub fn load(path: &PathBuf) -> Result<Self, std::io::Error> {
    let config = fs::read_to_string(path)?;
    let mut p =
      serde_yaml::from_str::<Project>(&config).map_err(|e| Error::new(ErrorKind::Other, e))?;
    let mut root_path = path.clone();
    root_path.pop();
    p.root_path = root_path;
    Ok(p)
  }

  pub(crate) fn service_by_name(&self, name: &str) -> Option<Service> {
    match self
      .services
      .iter()
      .find(|s| s.name.to_lowercase() == *name.to_lowercase())
    {
      Some(s) => Some(s.clone()),
      None => None,
    }
  }

  pub fn filter_default(&mut self) {
    self.components = self
      .clone()
      .components
      .into_iter()
      .filter(|c| c.default)
      .collect();
  }

  fn find_component(&self, name: &str) -> Option<&Component> {
    self
      .components
      .iter()
      .find(|c| c.name.to_lowercase() == name.to_lowercase())
  }

  fn find_group(&self, name: &str) -> Option<&Group> {
    self
      .groups
      .iter()
      .find(|g| g.name.to_lowercase() == name.to_lowercase())
  }

  fn find_component_task(&self, name: &str) -> Option<(Component, Task)> {
    self.components.iter().find_map(|c| {
      c.tasks
        .iter()
        .find(|t| name.to_lowercase() == format!("{}:{}", c.name, t.name).to_lowercase())
        .map(|t| (c.clone(), t.clone()))
    })
  }

  fn find_project_task(&self, name: &str) -> Option<Task> {
    self
      .tasks
      .iter()
      .find(|t| t.name.to_lowercase() == name.to_lowercase())
      .cloned()
  }

  fn run_project_task(&self, supr: &Supervisor, task: &Task) -> Result<(), String> {
    self.run_task_names(supr, &task.dependencies)?;
    let mut env = self.env.clone();
    env.extend(task.env.clone());
    let cmd = SupervisedCommand {
      commands: task.commands.clone().into(),
      env,
      path: task.path.clone().unwrap_or(self.root_path.clone()),
      color: crate::component::TerminalColor::White,
      keep_alive: false,
      name: task.name.clone(),
      delay: None,
      retry: false,
    };
    supr.run(cmd)?;
    Ok(())
  }

  fn run_component_task(
    &self,
    supr: &Supervisor,
    component: &Component,
    task: &Task,
  ) -> Result<(), String> {
    self.run_task_names(supr, &task.dependencies)?;
    let mut env = self.env.clone();
    env.extend(component.env.clone());
    env.extend(task.env.clone());
    let cmd = SupervisedCommand {
      commands: task.commands.clone().into(),
      env,
      path: task.path.clone().unwrap_or_else(|| {
        let mut path = self.root_path.clone();
        path.push(component.get_path());
        path
      }),
      color: crate::component::TerminalColor::White,
      keep_alive: false,
      name: format!("{}:{}", component.name, task.name),
      delay: None,
      retry: false,
    };
    supr.run(cmd)?;
    Ok(())
  }

  pub fn run(&self) {
    let supr = Supervisor::new(self);
    for c in self.components.iter() {
      supr.spawn_component(&c, HashMap::new());
    }
    supr.init();
  }

  fn run_task_names(&self, supr: &Supervisor, names: &Vec<String>) -> Result<bool, String> {
    let mut task_running = false;
    names
      .iter()
      .map(|n| self.find_project_task(n))
      .flatten()
      .map(|task| {
        task_running = true;
        self.run_project_task(&supr, &task)
      })
      .collect::<Result<(), String>>()?;

    for name in names.iter() {
      if let Some((component, task)) = self.find_component_task(name) {
        supr
          .run_component_services(&component)
          .for_each(|result| match result {
            Ok(s) => {
              crate::ui::system_message(format!("Started service: {}", s.name));
            }
            Err((s, e)) => {
              crate::ui::system_message(format!("Could not start service [{}]: {}", s.name, e));
            }
          });
        self.run_component_task(&supr, &component, &task)?;
        supr
          .shutdown_component_services(&component)
          .for_each(|result| match result {
            Ok(s) => {
              crate::ui::system_message(format!("Shutdown service: {}", s.name));
            }
            Err((s, e)) => {
              crate::ui::system_message(format!("Could not stop service [{}]: {}", s.name, e));
            }
          });
        task_running = true;
        continue;
      }
    }
    Ok(task_running)
  }

  pub fn run_names(&self, names: Vec<String>) -> Result<(), String> {
    // If a component was ran we need to invoke Supervisor::init at the end
    let mut cmp_running = false;
    // If a task has was ran we wont invoke Supervisor::init but we will still respond
    // that we have handled the operation so that we dont default to running everything in the project
    let supr = Supervisor::new(self);

    let task_running = self.run_task_names(&supr, &names)?;

    for name in names.iter() {
      if let Some(component) = self.find_component(name) {
        supr.spawn_component(component, HashMap::new());
        cmp_running = true;
        continue;
      }
    }
    for name in names.iter() {
      if let Some(group) = self.find_group(name) {
        for component_name in group.components.iter() {
          if let Some(component) = self.find_component(component_name) {
            cmp_running = true;
            supr.spawn_component(component, group.env.clone());
            continue;
          }
        }
      }
    }

    if cmp_running {
      supr.init();
    }

    if cmp_running || task_running {
      Ok(())
    } else {
      Err("Nothing to run".into())
    }
  }

  pub fn setup(&self) {
    let supr = Supervisor::new(self);
    for cmp in self.components.iter() {
      if cmp.repo.is_none() {
        continue;
      }
      let mut cmp_path = self.root_path.clone();
      cmp_path.push(cmp.get_path());
      let task = Task {
        name: cmp.name.clone(),
        path: Some(cmp_path.clone()),
        commands: cmp.init.clone().into(),
        env: cmp.env.clone(),
        ..Task::default()
      };
      match cmp.clone_repo(&cmp_path) {
        Ok(_) => {
          crate::ui::system_message(format!("{} cloned", cmp.clone().name));
          for cmd in &mut cmp.init.clone() {
            supr.run_task_command(&task, cmd.clone());
          }
        }
        Err(e) => crate::ui::system_error(format!("Skipping clone: {}", e)),
      }
    }
  }
}

impl Default for Project {
  fn default() -> Self {
    Project {
      name: "Unnamed Project".into(),
      components: vec![],
      services: vec![],
      groups: vec![],
      root_path: "".into(),
      tasks: vec![],
      env: HashMap::new(),
    }
  }
}
