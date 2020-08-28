use actix::prelude::*;
use crate::{component, Project};
use std::path::PathBuf;
use std::Collection

struct Component {
  pub project: Project
  pub component: Component,
  pub root_path: PathBuf,
}

impl Actor for ComponentWorker {

}