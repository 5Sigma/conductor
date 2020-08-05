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
}

impl Project {
  pub fn load(path: &PathBuf) -> Result<Self, std::io::Error> {
    let config = fs::read_to_string(path)?;
    let p =
      serde_yaml::from_str::<Project>(&config).map_err(|e| Error::new(ErrorKind::Other, e))?;
    Ok(p)
  }
  #[allow(dead_code)]
  pub fn service_by_name(&self, name: &str) -> Option<Service> {
    match self.services.iter().find(|s| s.name == *name) {
      Some(s) => Some(s.clone()),
      None => None,
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
    }
  }
}
