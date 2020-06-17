use git2::build::RepoBuilder;
use git2::Repository;
use git2::{Cred, FetchOptions, RemoteCallbacks};
use std::env;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::Path;

pub fn clone_repo(repo_url: &str, root_path: &Path) -> Result<Repository, Error> {
  if root_path.exists() {
    return Result::Err(Error::new(
      ErrorKind::Other,
      format!(
        "Directory already exists at {}",
        root_path.to_str().unwrap_or("unkown")
      ),
    ));
  }
  fs::create_dir_all(root_path)?;
  let mut builder = RepoBuilder::new();
  let mut callbacks = RemoteCallbacks::new();
  let mut fetch_options = FetchOptions::new();

  callbacks.credentials(|_, _, _| {
    let user: String = env::var("GIT_USER").unwrap_or_else(|_| "".into());
    let pass: String = env::var("GIT_PAT").unwrap_or_else(|_| "".into());
    Cred::userpass_plaintext(&user, &pass)
  });

  fetch_options.remote_callbacks(callbacks);
  builder.fetch_options(fetch_options);

  builder.clone(repo_url, &root_path).map_err(|e| {
    Error::new(
      ErrorKind::Other,
      format!("Could not clone repository: {}", e),
    )
  })
}
