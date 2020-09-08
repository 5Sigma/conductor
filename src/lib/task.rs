use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Task {
  pub name: String,
  pub path: PathBuf,
  pub commands: Vec<String>,
  pub env: HashMap<String, String>,
}

impl Task {
  pub fn new(
    name: &str,
    path: &PathBuf,
    commands: Vec<String>,
    env: HashMap<String, String>,
  ) -> Self {
    let mut task = Task {
      name: name.into(),
      path: path.into(),
      commands,
      env,
    };
    task.commands.reverse();
    task
  }
}

impl Iterator for Task {
  type Item = String;

  fn next(&mut self) -> Option<String> {
    self.commands.pop()
  }
}
