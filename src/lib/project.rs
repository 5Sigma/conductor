use crate::Component;
use crate::Service;
use serde::Deserialize;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

#[derive(Deserialize, PartialEq)]
pub struct Project {
  #[serde(default)]
  pub name: String,
  #[serde(default)]
  pub components: Vec<Component>,
  #[serde(default)]
  pub services: Vec<Service>,
}

impl Project {
  pub fn load(path: &PathBuf) -> Result<Self, std::io::Error> {
    let config = fs::read_to_string(path)?;
    let p =
      serde_yaml::from_str::<Project>(&config).map_err(|e| Error::new(ErrorKind::Other, e))?;
    Ok(p)
  }

  pub fn service_by_name(&self, name: String) -> Option<Service> {
    match self.services.iter().find(|s| s.name == name) {
      Some(s) => return Some(s.clone()),
      None => return None,
    }
  }
}

impl Default for Project {
  fn default() -> Self {
    Project {
      name: "Unnamed Project".into(),
      components: vec![],
      services: vec![],
    }
  }
}
