//! `mk61` command-line entry point.
//!
//! The binary is a thin presentation layer: it parses arguments, calls service
//! use-cases, and renders results. Protocol constants and device access live in
//! the other workspace crates.

#![forbid(unsafe_code)]

mod cli;
mod output;

use std::error::Error;
use std::io;
use std::process::ExitCode;

use clap::Parser;
use mk61_service::{DeviceCatalog, collect_status};

use crate::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mk61: error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    // Resolve global options now so defaults and path handling are exercised.
    // Later milestones pass this configuration to device use-cases.
    let config = cli.service_config();
    let sysfs_root = cli.resolve_sysfs_root()?;
    let Cli { command, .. } = cli;

    match command {
        Command::Help => output::print_overview(&mut io::stdout().lock())?,
        Command::List => {
            let catalog = DeviceCatalog::new(&sysfs_root);
            let devices = match &config.device_path {
                Some(path) => vec![catalog.inspect(path)?],
                None => catalog.list()?,
            };
            output::print_devices(&mut io::stdout().lock(), &devices)?;
        }
        Command::Status => {
            let report = collect_status(&config)?;
            output::print_status(&mut io::stdout().lock(), &report)?;
        }
    }

    Ok(())
}
