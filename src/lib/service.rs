use rs_docker::Docker;
use serde::Deserialize;
use std::io;

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
    pub fn start(&self) -> io::Result<String> {
        start_container(&self.get_container_name())
    }
    pub fn stop(&self) -> io::Result<String> {
        stop_container(&self.get_container_name())
    }
}

fn start_container(name: &str) -> io::Result<String> {
    let mut docker = Docker::connect("unix:///var/run/docker.sock")?;
    docker.start_container(name)
}

fn stop_container(name: &str) -> io::Result<String> {
    let mut docker = Docker::connect("unix:///var/run/docker.sock")?;
    docker.stop_container(name)
}

pub struct ServiceLauncher {
    services: Vec<Service>,
}

impl Iterator for ServiceLauncher {
    type Item = Result<Service, (Service, std::io::Error)>;

    fn next(&mut self) -> Option<Result<Service, (Service, std::io::Error)>> {
        match self.services.pop() {
            Some(service) => match service.start() {
                Ok(_) => Some(Ok(service)),
                Err(e) => Some(Err((service, e))),
            },
            None => None,
        }
    }
}

impl ServiceLauncher {
    pub fn new(services: Vec<Service>) -> ServiceLauncher {
        ServiceLauncher { services }
    }
}

pub struct ServiceTerminator {
    services: Vec<Service>,
}

impl Iterator for ServiceTerminator {
    type Item = Result<Service, (Service, std::io::Error)>;

    fn next(&mut self) -> Option<Result<Service, (Service, std::io::Error)>> {
        match self.services.pop() {
            Some(service) => match service.stop() {
                Ok(_) => Some(Ok(service)),
                Err(e) => Some(Err((service, e))),
            },
            None => None,
        }
    }
}

impl ServiceTerminator {
    pub fn new(services: Vec<Service>) -> ServiceTerminator {
        ServiceTerminator { services }
    }
}
