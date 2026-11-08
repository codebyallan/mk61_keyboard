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
use mk61_service::HidrawTransport;
use mk61_service::{
    BackupStore, DeviceCatalog, FlashGuard, SystemClock, collect_status, led_effects,
};
use std::io::Write;
use std::path::PathBuf;

use crate::cli::{Cli, Command, LedCommand};

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
        Command::Led(led_command) => match led_command {
            LedCommand::Effects => {
                let effects = led_effects();
                output::print_effects(&mut io::stdout().lock(), &effects)?;
            }
            LedCommand::Effect { spec, save } => {
                let device_path = resolve_device_path(&config)?;
                let transport =
                    HidrawTransport::open(&device_path).map_err(Box::<dyn Error>::from)?;
                let mut service = mk61_service::Service::new(config.clone(), transport);
                let backup = BackupStore::from_config(&config);
                let mut guard = FlashGuard::from_config(&config);
                let result =
                    service.apply_led_effect(&SystemClock, &backup, &mut guard, &spec, save)?;
                writeln!(
                    io::stdout().lock(),
                    "applied effect '{}' to profile {}{}",
                    result.effect_name,
                    result.profile,
                    if result.saved {
                        " (saved to flash)"
                    } else {
                        ""
                    }
                )?;
            }
            LedCommand::Color {
                effect,
                colors,
                save,
            } => {
                let parsed_colors: Vec<mk61_core::Rgb> = colors
                    .iter()
                    .map(|value| mk61_core::Rgb::parse(value))
                    .collect::<Result<_, _>>()
                    .map_err(Box::<dyn Error>::from)?;
                let device_path = resolve_device_path(&config)?;
                let transport =
                    HidrawTransport::open(&device_path).map_err(Box::<dyn Error>::from)?;
                let mut service = mk61_service::Service::new(config.clone(), transport);
                let backup = mk61_service::BackupStore::from_config(&config);
                let mut guard = mk61_service::FlashGuard::from_config(&config);
                let result = service.apply_led_color(
                    &mk61_service::SystemClock,
                    &backup,
                    &mut guard,
                    &effect,
                    &parsed_colors,
                    save,
                )?;
                writeln!(
                    io::stdout().lock(),
                    "set {} colour(s) for '{}' on profile {}{}",
                    result.color_count,
                    result.effect_name,
                    result.profile,
                    if result.saved {
                        " (saved to flash)"
                    } else {
                        ""
                    }
                )?;
            }
            LedCommand::Save => {
                let device_path = resolve_device_path(&config)?;
                let transport =
                    HidrawTransport::open(&device_path).map_err(Box::<dyn Error>::from)?;
                let mut service = mk61_service::Service::new(config.clone(), transport);
                let backup = mk61_service::BackupStore::from_config(&config);
                let mut guard = mk61_service::FlashGuard::from_config(&config);
                let result =
                    service.save_profile(&mk61_service::SystemClock, &backup, &mut guard)?;
                writeln!(
                    io::stdout().lock(),
                    "saved profile {} to flash",
                    result.profile
                )?;
            }
            LedCommand::Brightness { value, save } => {
                let device_path = resolve_device_path(&config)?;
                let transport =
                    HidrawTransport::open(&device_path).map_err(Box::<dyn Error>::from)?;
                let mut service = mk61_service::Service::new(config.clone(), transport);
                let backup = mk61_service::BackupStore::from_config(&config);
                let mut guard = mk61_service::FlashGuard::from_config(&config);
                let result = service.apply_led_brightness(
                    &mk61_service::SystemClock,
                    &backup,
                    &mut guard,
                    value,
                    save,
                )?;
                writeln!(
                    io::stdout().lock(),
                    "set brightness to {} on profile {}{}",
                    result.value,
                    result.profile,
                    if result.saved {
                        " (saved to flash)"
                    } else {
                        ""
                    }
                )?;
            }
            LedCommand::Speed { value, save } => {
                let device_path = resolve_device_path(&config)?;
                let transport =
                    HidrawTransport::open(&device_path).map_err(Box::<dyn Error>::from)?;
                let mut service = mk61_service::Service::new(config.clone(), transport);
                let backup = mk61_service::BackupStore::from_config(&config);
                let mut guard = mk61_service::FlashGuard::from_config(&config);
                let result = service.apply_led_speed(
                    &mk61_service::SystemClock,
                    &backup,
                    &mut guard,
                    value,
                    save,
                )?;
                writeln!(
                    io::stdout().lock(),
                    "set speed to {} on profile {}{}",
                    result.value,
                    result.profile,
                    if result.saved {
                        " (saved to flash)"
                    } else {
                        ""
                    }
                )?;
            }
            LedCommand::Direction { value, save } => {
                let device_path = resolve_device_path(&config)?;
                let transport =
                    HidrawTransport::open(&device_path).map_err(Box::<dyn Error>::from)?;
                let mut service = mk61_service::Service::new(config.clone(), transport);
                let backup = mk61_service::BackupStore::from_config(&config);
                let mut guard = mk61_service::FlashGuard::from_config(&config);
                let result = service.apply_led_direction(
                    &mk61_service::SystemClock,
                    &backup,
                    &mut guard,
                    value,
                    save,
                )?;
                writeln!(
                    io::stdout().lock(),
                    "set direction to {} on profile {}{}",
                    result.value,
                    result.profile,
                    if result.saved {
                        " (saved to flash)"
                    } else {
                        ""
                    }
                )?;
            }
        },
    }

    Ok(())
}

fn resolve_device_path(config: &mk61_service::ServiceConfig) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = &config.device_path {
        return Ok(path.clone());
    }
    let catalog = DeviceCatalog::new(&config.sysfs_root);
    let devices = catalog.list()?;
    let supported = devices.into_iter().find(|d| d.is_supported());
    match supported {
        Some(device) => Ok(device.path),
        None => Err(Box::<dyn Error>::from(mk61_service::ServiceError::Device(
            mk61_service::DeviceError::NotFound,
        ))),
    }
}
