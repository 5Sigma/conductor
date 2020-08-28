use crate::supervisor::Supervisor;
use crate::Component;
use crate::Group;
use crate::Service;
use serde::Deserialize;
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
  #[allow(dead_code)]
  pub fn service_by_name(&self, name: &str) -> Option<Service> {
    match self.services.iter().find(|s| s.name == *name) {
      Some(s) => Some(s.clone()),
      None => None,
    }
  }

  pub fn filter_names(&mut self, names: Vec<String>) {
    self.components = self
      .clone()
      .components
      .into_iter()
      .filter(|c| names.iter().any(|n| n == &c.name))
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

  pub fn find_component(&self, name: &str) -> Option<&Component> {
    self.components.iter().find(|c| c.name == name)
  }

  pub fn find_group(&self, name: &str) -> Option<&Group> {
    self.groups.iter().find(|g| g.name == name)
  }

  pub fn find_components(&self, names: Vec<String>) -> Vec<&Component> {
    self
      .components
      .iter()
      .filter(|c| names.iter().all(|n| &c.name != n))
      .collect()
  }

  pub fn run(&self) {
    let supr = Supervisor::new(self);
    for c in self.components.iter() {
      supr.spawn_component(&c)
    }
    supr.init();
  }

  pub fn run_names(&self, names: Vec<String>) -> Result<(), String> {
    let mut found = false;
    let supr = Supervisor::new(self);
    for name in names.iter() {
      if let Some(component) = self.find_component(name) {
        supr.spawn_component(component);
        found = true;
        continue;
      }
    }
    for name in names.iter() {
      if let Some(group) = self.find_group(name) {
        for component_name in group.components.iter() {
          if let Some(component) = self.find_component(component_name) {
            found = true;
            supr.spawn_component(component);
            continue;
          }
        }
      }
    }
    if found == true {
      supr.init();
      Ok(())
    } else {
      Err("No components found".into())
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
    }
  }
}
