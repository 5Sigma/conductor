use serde::Deserialize;

/// The type of the service. Currently only Docker is supported.
#[derive(Clone, Deserialize, PartialEq)]
pub enum ServiceType {
  DockerContainer,
}

impl Default for ServiceType {
  fn default() -> Self {
    ServiceType::DockerContainer
  }
}

/// Services are external support systems used by the component. Currently only docker containers
/// are supported. Support for services is also limited to MacOS and Linux platforms.
#[derive(Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct Service {
  pub service_type: ServiceType,
  pub container: Option<String>,
  pub name: String,
}

impl Default for Service {
  fn default() -> Self {
    Service {
      name: String::from(""),
      container: None,
      service_type: ServiceType::default(),
    }
  }
}

impl Service {
  pub fn get_container_name(&self) -> String {
    self.container.as_ref().unwrap_or(&self.name).clone()
  }
}
