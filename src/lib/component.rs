use crate::git;
use crate::task::Task;
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
  pub path: Option<PathBuf>,
  pub keep_alive: bool,
  pub color: TerminalColor,
  pub env: HashMap<String, String>,
  pub tasks: Vec<Task>,
  pub repo: Option<String>,
  pub delay: Option<u64>,
  pub start: String,
  pub init: Vec<String>,
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
      tasks: vec![],
      repo: None,
      color: TerminalColor::Yellow,
      delay: None,
      start: "".into(),
      init: vec![],
      retry: false,
      keep_alive: false,
      services: vec![],
    }
  }
}

impl Component {
  pub fn get_path(&self) -> PathBuf {
    let path_str = self
      .path
      .clone()
      .unwrap_or_else(|| self.name.clone().into());
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
