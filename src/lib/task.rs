use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::Commands;
#[derive(Clone, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Task {
  pub name: String,
  pub description: Option<String>,
  pub dependencies: Vec<String>,
  pub path: Option<PathBuf>,
  pub commands: Commands,
  pub env: HashMap<String, String>,
}

impl Default for Task {
  fn default() -> Self {
    Self {
      name: "Unnamed".into(),
      description: None,
      path: None,
      dependencies: vec![],
      commands: Commands::Multiple(vec![]),
      env: HashMap::new(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn test_task_iterator() {
    let mut t = Task::default();
    t.commands = Commands::Multiple(vec![
      String::from("one"),
      String::from("two"),
      String::from("three"),
      String::from("four"),
    ]);
    assert_eq!(t.commands.into_iter().find(|o| o == "two").is_some(), true);
  }
}
