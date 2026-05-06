use crate::config;
use crate::platform;
use crate::state::State;
use std::path::Path;
use std::process::ExitCode;

pub fn run(repo: Option<&Path>) -> anyhow::Result<ExitCode> {
    // Platform detection
    match platform::detect() {
        Ok(platform) => {
            println!("Platform:  {platform}");
            if !platform.managers.is_empty() {
                let names: Vec<_> = platform.managers.iter().map(|m| m.name.as_str()).collect();
                println!("Managers:  {}", names.join(", "));
            }
        }
        Err(e) => {
            eprintln!("Platform:  {e}");
            return Ok(ExitCode::from(4));
        }
    }

    // State info (machine identity and repo path)
    let state = State::load().unwrap_or_default();
    if let Some(id) = state.machine_id() {
        println!("Machine:   {id}");
    }
    if let Some(path) = state.repo_path() {
        println!("Repo:      {path}");
    }

    // Config validity
    match config::find_config_with_repo(repo) {
        Ok((_path, config)) => {
            println!("Config:    nostos.toml (valid)");
            if let Some(df) = &config.dotfiles {
                println!("Dotfiles:  source={}, target={}", df.source, df.target);
            }
            if !config.tools.is_empty() {
                println!("Tools:     {} declared (not yet active)", config.tools.len());
            }
            if !config.hooks.is_empty() {
                println!("Hooks:     {} declared (not yet active)", config.hooks.len());
            }
        }
        Err(config::Error::NotFound { .. }) => {
            println!("Config:    not found (no nostos.toml in current directory)");
        }
        Err(e) => {
            eprintln!("Config:    invalid — {e}");
            return Ok(ExitCode::from(3));
        }
    }

    Ok(ExitCode::SUCCESS)
}
