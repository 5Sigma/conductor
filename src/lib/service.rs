use serde::Deserialize;

#[derive(Clone, Deserialize, PartialEq)]
pub enum ServiceType {
  DockerContainer,
}

impl Default for ServiceType {
  fn default() -> Self {
    ServiceType::DockerContainer
  }
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct Service {
  pub service_type: ServiceType,
  pub name: String,
}

impl Default for Service {
  fn default() -> Self {
    Service {
      name: String::from(""),
      service_type: ServiceType::default(),
    }
  }
}
