use crate::git;
use crate::Command;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Deserialize, PartialEq)]
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

#[derive(Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct Component {
  pub name: String,
  pub path: Option<String>,
  pub color: TerminalColor,
  pub env: HashMap<String, String>,
  pub repo: Option<String>,
  pub delay: Option<u64>,
  pub start: Command,
  pub init: Vec<Command>,
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
      repo: None,
      color: TerminalColor::Yellow,
      delay: None,
      start: Command::default(),
      tags: vec![],
      init: vec![],
      retry: true,
      services: vec![],
    }
  }
}

impl Component {
  pub fn has_tag(&self, tags: &[&str]) -> bool {
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
