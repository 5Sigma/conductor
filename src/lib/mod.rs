mod component;
mod git;
mod group;
mod project;
mod service;
mod supervisor;
mod task;

use component::*;
use group::*;
pub use project::Project;
use service::*;
pub mod ui;


use serde::Deserialize;
#[derive(Clone, Deserialize, PartialEq, Debug)]
#[serde(untagged)]
pub enum Commands {
  Single(String),
  Multiple(Vec<String>),
}

impl Into<Vec<String>> for Commands {
  fn into(self) -> Vec<String> {
    self.into_iter().collect()
  }
}

impl From<Vec<String>> for Commands {
    fn from(v: Vec<String>) -> Self {
        Self::Multiple(v)
    }
}

impl Iterator for Commands {
  type Item = String;


  fn next(&mut self) -> Option<String> {
    match self {
      Commands::Single(_) => {
        if let Commands::Single(s) = std::mem::replace(self, Commands::Multiple(vec![])) {
          Some(s.clone())
        } else {
          None
        }
      }
      Commands::Multiple(vs) => vs.pop(),
    }
  }
}
