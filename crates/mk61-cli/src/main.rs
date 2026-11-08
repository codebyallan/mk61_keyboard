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
use mk61_service::DeviceCatalog;

use crate::cli::{Cli, Command, sysfs_root_from_env};

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
    let _config = cli.service_config();
    let Cli {
        device, command, ..
    } = cli;

    match command {
        Command::Help => output::print_overview(&mut io::stdout().lock())?,
        Command::List => {
            let catalog = DeviceCatalog::new(sysfs_root_from_env()?);
            let devices = match device {
                Some(path) => vec![catalog.inspect(&path)?],
                None => catalog.list()?,
            };
            output::print_devices(&mut io::stdout().lock(), &devices)?;
        }
    }

    Ok(())
}
