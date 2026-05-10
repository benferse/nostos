use crate::config;
use crate::platform;
use crate::state::State;
use std::path::Path;
use std::process::ExitCode;

pub fn run(repo: Option<&Path>) -> anyhow::Result<ExitCode> {
    // Platform detection
    let platform = match platform::detect() {
        Ok(platform) => {
            println!("Platform:  {platform}");
            if !platform.managers.is_empty() {
                let names: Vec<_> = platform.managers.iter().map(|m| m.name.as_str()).collect();
                println!("Managers:  {}", names.join(", "));
            }
            Some(platform)
        }
        Err(e) => {
            eprintln!("Platform:  {e}");
            return Ok(ExitCode::from(4));
        }
    };

    // State info (machine identity and repo path)
    let state = State::load().unwrap_or_default();
    let machine_id = state.machine_id().map(|s| s.to_string());
    if let Some(ref id) = machine_id {
        println!("Machine:   {id}");
    } else {
        println!("Machine:   (not set)");
    }
    if let Some(path) = state.repo_path() {
        println!("Repo:      {path}");
    } else {
        println!("Repo:      (not set — run `nostos init`)");
    }

    // Config validity and active layers
    match config::find_config_with_repo(repo) {
        Ok((_path, config)) => {
            println!("Config:    nostos.toml (valid)");
            if let Some(df) = &config.dotfiles {
                println!("Dotfiles:  source={}, target={}", df.source, df.target);
            }
            if let Some(files) = &config.files {
                println!("Files:     source={}, target={}", files.source, files.target);
            }
            if !config.tools.is_empty() {
                println!("Tools:     {} declared (not yet active)", config.tools.len());
            }
            if !config.hooks.is_empty() {
                println!("Hooks:     {} declared (not yet active)", config.hooks.len());
            }

            // Active layers
            if let Some(ref platform) = platform {
                let os_name = platform.os.to_string();
                print_active_layers(&config, &os_name, machine_id.as_deref());
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

fn print_active_layers(config: &config::Config, os_name: &str, machine_id: Option<&str>) {
    let mut layers = vec!["base".to_string()];

    let has_platform_dotfiles = config
        .dotfiles
        .as_ref()
        .is_some_and(|df| df.platforms.get(os_name).is_some_and(|m| !m.is_empty()));
    let has_platform_files = config
        .files
        .as_ref()
        .is_some_and(|f| f.platforms.get(os_name).is_some_and(|m| !m.is_empty()));

    if has_platform_dotfiles || has_platform_files {
        layers.push(os_name.to_string());
    }

    if let Some(id) = machine_id {
        let has_machine_dotfiles = config
            .dotfiles
            .as_ref()
            .is_some_and(|df| df.machines.get(id).is_some_and(|m| !m.is_empty()));
        let has_machine_files = config
            .files
            .as_ref()
            .is_some_and(|f| f.machines.get(id).is_some_and(|m| !m.is_empty()));

        if has_machine_dotfiles || has_machine_files {
            layers.push(id.to_string());
        }
    }

    println!("Layers:    {}", layers.join(" + "));
}
