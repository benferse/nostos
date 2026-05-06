//! nostos — a cross-platform dev-environment sync tool.
//!
//! nostos copies dotfiles from a repository to your home directory and
//! (in future releases) installs the tools defined in your config.  The
//! library crate exposes the building blocks used by the `nostos` binary:
//!
//! * [`config`] — parse and validate `nostos.toml`
//! * [`platform`] — detect the current OS, architecture, and Linux distro
//! * [`reconcile`] — compute and execute a dotfile sync plan
//! * [`state`] — persist a record of what nostos has applied
//! * [`cli`] — clap-based command-line interface wired to the above

/// Command-line interface: the [`cli::Cli`] struct and [`cli::run`] entry point.
pub mod cli;

/// Configuration loading and parsing for `nostos.toml`.
pub mod config;

/// Platform detection (OS, CPU architecture, Linux distro).
pub mod platform;

/// Dotfile reconciliation: plan and apply file-sync operations.
pub mod reconcile;

/// Persistent state tracking what nostos has applied on this machine.
pub mod state;
