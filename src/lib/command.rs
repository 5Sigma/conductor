use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Deserialize, PartialEq)]
pub struct Command {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.command, self.args.join(" "))
    }
}

impl Default for Command {
    fn default() -> Self {
        Command {
            command: "".into(),
            args: vec![],
            dir: None,
            env: HashMap::new(),
        }
    }
}
