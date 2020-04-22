use crate::{ui, Component, Project};

use git2::build::RepoBuilder;
use git2::{Cred, FetchOptions, RemoteCallbacks};
use std::io::prelude::*;
use std::io::{BufReader, Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;
use std::{env, fs, io};
// use timeout_readwrite::TimeoutReader;

#[derive(Debug)]
pub enum ProjectError {
    Io(io::Error),
}

pub struct ComponentMessage {
    component: Component,
    body: String,
}

struct Worker {
    pub component: Component,
    pub handler: thread::JoinHandle<()>,
    pub kill_signal: Sender<()>,
}

fn create_command(c: &crate::Command, component: &Component, root_path: &PathBuf) -> Command {
    let mut root_path = root_path.clone();
    let mut cmd = Command::new(&c.command);
    c.args.iter().for_each(|a| {
        cmd.arg(a);
    });
    for (k, v) in &c.env {
        cmd.env(k, v);
    }
    for (k, v) in &component.env {
        cmd.env(k, v);
    }
    let dir = component
        .start
        .clone()
        .dir
        .unwrap_or(component.get_path())
        .clone();
    root_path.push(dir);
    cmd.current_dir(root_path);
    cmd
}

fn spawn_component(
    component: Component,
    data_tx: Sender<ComponentMessage>,
    root_path: &PathBuf,
) -> Result<Worker, Error> {
    let data_tx = data_tx.clone();
    let (tx, rx) = mpsc::channel();
    let c = component.clone();
    let rp = root_path.clone();
    let th = thread::spawn(move || {
        loop {
            let mut cmd = create_command(&c.start, &c, &rp);
            if let Some(delay) = c.delay {
                thread::sleep(Duration::from_secs(delay));
            }
            let stdout_result: Result<std::process::ChildStdout, io::Error> =
                cmd.stdout(Stdio::piped()).spawn().and_then(|mut child| {
                    child.stdout.take().ok_or(Error::new(
                        ErrorKind::Other,
                        "Could not create process pipe",
                    ))
                });
            match stdout_result {
                Ok(stdout) => {
                    // let buf = BufReader::new(TimeoutReader::new(stdout, Duration::new(1, 0)));
                    let buf = BufReader::new(stdout);
                    let _ = buf.lines().try_for_each(|line| match line {
                        Ok(body) => {
                            let payload = ComponentMessage {
                                component: c.clone(),
                                body,
                            };
                            match data_tx.send(payload) {
                                Ok(_) => {}
                                Err(e) => println!("Error sending output: {}", e),
                            };
                            Some(())
                        }
                        Err(_) => match rx.try_recv() {
                            Ok(_) | Err(mpsc::TryRecvError::Disconnected) => Some(()), // currently not checking for kill signals
                            Err(mpsc::TryRecvError::Empty) => Some(()),
                        },
                    });
                    ui::system_message(format!("Component shutdown: {}", c.name));
                }
                Err(e) => {
                    ui::system_error(format!("Could not load component {}", c.name));
                    println!("{}", e);
                }
            };
            if c.retry == false {
                break;
            }
            ui::system_message(format!(
                "Restarting {} in {} seconds",
                &c.name,
                c.delay.unwrap_or(1)
            ));
            thread::sleep(Duration::from_secs(c.delay.unwrap_or(1)));
        }
    });

    Ok(Worker {
        kill_signal: tx,
        component,
        handler: th,
    })
}

pub fn run_command(c: &crate::Command, cmp: &Component, root_path: &PathBuf) {
    let mut cmd = create_command(c, cmp, &root_path);
    ui::system_message(format!("Executing: {}", c));
    match cmd
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdout.take().ok_or(Error::new(
                ErrorKind::Other,
                "Could not create process pipe",
            ))
        })
        .and_then(|stdout| Ok(BufReader::new(stdout).lines()))
    {
        Ok(lines) => lines.for_each(|line| {
            ui::component_message(cmp, line.unwrap());
        }),
        Err(e) => {
            ui::system_error("Command Error".into());
            println!("{}", e);
        }
    }
}

pub fn clone_repo(component: Component, root_path: &PathBuf) -> Result<(), Error> {
    let mut root_path = root_path.clone();
    if Path::new(&component.get_path()).exists() {
        return Result::Err(Error::new(
            ErrorKind::Other,
            format!("Directory already exists at {}", component.get_path()),
        ));
    }
    fs::create_dir_all(component.get_path())?;
    ui::system_message(format!(
        "Cloning {} from {} into {}",
        component.clone().name,
        component.clone().repo,
        component.get_path(),
    ));

    let mut builder = RepoBuilder::new();
    let mut callbacks = RemoteCallbacks::new();
    let mut fetch_options = FetchOptions::new();

    callbacks.credentials(|_, _, _| {
        let user: String = env::var("GIT_USER").unwrap_or("".into());
        let pass: String = env::var("GIT_PAT").unwrap_or("".into());
        Cred::userpass_plaintext(&user, &pass)
    });

    fetch_options.remote_callbacks(callbacks);
    builder.fetch_options(fetch_options);

    root_path.push(component.get_path());

    match builder.clone(&component.repo, &root_path) {
        Ok(_) => Ok(()),
        Err(e) => Result::Err(Error::new(
            ErrorKind::Other,
            format!("Could not clone repository: {}", e),
        )),
    }
}

pub fn run_project(fname: &PathBuf, tags: Option<Vec<String>>) {
    let mut fname = fname.clone();
    match Project::load(&fname) {
        Err(e) => {
            ui::system_error("Could not load project".into());
            println!("{}", e);
        }
        Ok(project) => {
            let (tx, rx) = mpsc::channel();

            let components = match tags {
                Some(t) => project
                    .components
                    .clone()
                    .into_iter()
                    .filter({ |x| x.has_tag(t.clone()) })
                    .collect(),
                None => project.components.clone(),
            };

            fname.pop();

            for c in components.iter() {
                match spawn_component(c.clone(), tx.clone(), &fname) {
                    Ok(_) => ui::system_message(format!("Started {}", c.name)),
                    Err(e) => ui::system_error(format!("Failed to start {}: {}", c.name, e)),
                }
            }

            loop {
                let msg = rx.recv().unwrap();
                ui::component_message(&msg.component, msg.body);
            }
        }
    };
}

pub fn setup_project(fname: &PathBuf) {
    let mut fname = fname.clone();
    match Project::load(&fname) {
        Err(e) => {
            ui::system_error("Could not load project".into());
            println!("{}", e);
        }

        Ok(project) => {
            fname.pop();
            for cmp in project.components.into_iter() {
                match clone_repo(cmp.clone(), &fname) {
                    Ok(_) => ui::system_message(format!("{} cloned", cmp.clone().name)),
                    Err(e) => ui::system_error(format!("Skipping clone: {}", e)),
                }
                for cmd in &cmp.init {
                    run_command(&cmd, &cmp, &fname);
                }
            }
        }
    };
}
