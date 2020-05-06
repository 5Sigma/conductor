![CI](https://github.com/5Sigma/conductor/workflows/CI/badge.svg)

# Conductor

Conductor is a simple task runner/launcher. Its goal is to make it easier to launch more complicated stacks in a development environment. Automatically launching backend, front end, and services at once. While also aggregating all their output together into a single process.


## Installation

Binaries are available under the releases.


## Configuration

Conductor is configured by creating a `conductor.yml` file. Usually at the root of the project(s).


### Example configuration:

``` yaml
name: MyApp
components: 
- name: api-server
  tags: 
  - web
  - api
  color: Blue
  repo: https://github.com/me/elixir-backend.git
  start:
    env:
      MIX_ENV: dev
    command: mix
    args: 
      - phx.server
  init:
    - command: mix 
      args: 
      - deps.get
    - command: mix
      args:
      - compile
- name: web
  tags: 
  - web
  color: Purple
  start:
    command: npm
    args: 
    - start
  env:
    FORCE_COLOR: 1
  repo: https://github.com/me/react-frontend.git
  init:
  - command: yarn
    args: 
    - install


```


## Usage

Running the binary in a folder containing a conductor.yml (or any subfolder of that folder). Will run the entire stack.

``` sh
conductor 
```

The Setup subcommand will clone repos and run their init commands

``` sh
conductor setup
```

A single component can be executed using the run subcommand

``` sh
conductor run web
```


Tags can also be specified to limit the execution to a set of components

``` sh
conductor run --tags=web,api
```
