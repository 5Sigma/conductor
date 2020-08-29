use crate::git;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Deserialize, PartialEq, Debug)]
pub enum TerminalColor {
  Blue,
  Green,
  Yellow,
  Purple,
  White,
  Red,
  Cyan,
}

impl Default for TerminalColor {
  fn default() -> Self {
    TerminalColor::Yellow
  }
}

#[derive(Clone, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Component {
  pub name: String,
  pub path: Option<String>,
  pub keep_alive: bool,
  pub color: TerminalColor,
  pub env: HashMap<String, String>,
  pub tasks: HashMap<String, Vec<String>>,
  pub repo: Option<String>,
  pub delay: Option<u64>,
  pub start: String,
  pub init: Vec<String>,
  pub tags: Vec<String>,
  pub retry: bool,
  pub default: bool,
  pub services: Vec<String>,
}

impl Default for Component {
  fn default() -> Self {
    Component {
      name: "Unknown".into(),
      default: true,
      path: None,
      env: HashMap::new(),
      tasks: HashMap::new(),
      repo: None,
      color: TerminalColor::Yellow,
      delay: None,
      start: "".into(),
      tags: vec![],
      init: vec![],
      retry: false,
      keep_alive: false,
      services: vec![],
    }
  }
}

impl Component {
  pub fn has_tags(&self, tags: &[&str]) -> bool {
    if tags.is_empty() {
      return true;
    }
    self.tags.iter().any(|a| tags.iter().any(|b| a == b))
  }

  pub fn get_path(&self) -> PathBuf {
    let path_str = self.path.clone().unwrap_or_else(|| self.name.clone());
    Path::new(&path_str).to_owned()
  }

  pub fn clone_repo(&self, root_path: &Path) -> Result<(), std::io::Error> {
    match &self.repo {
      Some(repo) => git::clone_repo(&repo, root_path).map(|_| ()),
      None => Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "Repo not specified",
      )),
    }
  }
}
