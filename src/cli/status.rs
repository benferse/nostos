use crate::config;
use crate::platform;
use std::process::ExitCode;

pub fn run() -> anyhow::Result<ExitCode> {
    // Platform detection
    match platform::detect() {
        Ok(platform) => {
            println!("Platform: {platform}");
        }
        Err(e) => {
            eprintln!("Platform: {e}");
            return Ok(ExitCode::from(4));
        }
    }

    // Config validity
    match config::find_config() {
        Ok((_path, config)) => {
            if let Some(df) = &config.dotfiles {
                println!("Config:   nostos.toml (valid)");
                println!("Dotfiles: source={}, target={}", df.source, df.target);
            } else {
                println!("Config:   nostos.toml (no [dotfiles] section)");
            }
        }
        Err(config::Error::NotFound { .. }) => {
            println!("Config:   not found (no nostos.toml in current directory)");
        }
        Err(e) => {
            eprintln!("Config:   invalid — {e}");
            return Ok(ExitCode::from(3));
        }
    }

    Ok(ExitCode::SUCCESS)
}
