use crate::Component;
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
}

impl Project {
    pub fn load(path: &PathBuf) -> Result<Self, std::io::Error> {
        let config = fs::read_to_string(path)?;
        let p = serde_yaml::from_str::<Project>(&config)
            .map_err(|e| Error::new(ErrorKind::Other, e))?;

        Ok(p)
    }
}

impl Default for Project {
    fn default() -> Self {
        Project {
            name: "Unnamed Project".into(),
            components: vec![],
        }
    }
}
