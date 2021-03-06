use crate::supervisor::Supervisor;
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
  pub tasks: HashMap<String, Vec<String>>,
  pub root_path: PathBuf,
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
  pub fn service_by_name(&self, name: &str) -> Option<Service> {
    match self
      .services
      .iter()
      .find(|s| s.name.to_lowercase() == *name.to_lowercase())
    {
      Some(s) => Some(s.clone()),
      None => None,
    }
  }

  pub fn filter_names(&mut self, names: Vec<String>) {
    self.components = self
      .clone()
      .components
      .into_iter()
      .filter(|c| {
        names
          .iter()
          .any(|n| n.to_lowercase() == c.name.to_lowercase())
      })
      .collect();
  }

  pub fn filter_tags(&mut self, tags: &[&str]) {
    self.components = self
      .clone()
      .components
      .into_iter()
      .filter(|c| c.has_tags(tags))
      .collect();
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
    for c in self.components.iter() {
      for (task_name, cmds) in c.tasks.clone().into_iter() {
        if name.to_lowercase() == format!("{}:{}", c.name, task_name).to_lowercase() {
          return Some((
            c.clone(),
            Task::new(name, &c.get_path(), cmds, c.env.clone()),
          ));
        }
      }
    }
    None
  }

  fn find_project_task(&self, name: &str) -> Option<Task> {
    for (task_name, cmds) in self.tasks.clone().into_iter() {
      if name.to_lowercase() == task_name.to_lowercase() {
        return Some(Task::new(name, &self.root_path, cmds, HashMap::new()));
      }
    }
    None
  }

  pub fn run(&self) {
    let supr = Supervisor::new(self);
    for c in self.components.iter() {
      supr.spawn_component(&c, HashMap::new());
    }
    supr.init();
  }

  pub fn run_names(&self, names: Vec<String>) -> Result<(), String> {
    // If a component was ran we need to invoke Supervisor::init at the end
    let mut cmp_running = false;
    // If a task has was ran we wont invoke Supervisor::init but we will still respond
    // that we have handled the operation so that we dont default to running everything in the project
    let mut task_running = false;
    let supr = Supervisor::new(self);

    for name in names.iter() {
      if let Some(task) = self.find_project_task(name) {
        let t = task.clone();
        for cmd in task {
          supr.run_task_command(&t, cmd.clone());
        }
        task_running = true;
        continue;
      }
    }

    for name in names.iter() {
      if let Some((component, task)) = self.find_component_task(name) {
        let t = task.clone();
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
        for cmd in task {
          supr.run_task_command(&t, cmd.clone());
        }
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
      let task = Task::new(&cmp.name, &cmp_path, cmp.init.clone(), cmp.env.clone());
      match cmp.clone_repo(&cmp_path) {
        Ok(_) => {
          crate::ui::system_message(format!("{} cloned", cmp.clone().name));
          for cmd in &cmp.init {
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
      tasks: HashMap::new(),
    }
  }
}
