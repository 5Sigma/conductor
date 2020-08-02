use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, PartialEq, Clone)]
pub struct Group {
  pub name: String,
  pub components: Vec<String>,
  #[serde(default)]
  pub env: HashMap<String, String>,
}
