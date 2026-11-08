//! Command-line grammar and global option handling.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use mk61_service::ServiceConfig;

/// Environment variable that overrides the sysfs root used for discovery.
///
/// This is a documented test hook. Production runs use [`DEFAULT_SYSFS_ROOT`].
/// The value must be an absolute path; a relative override is rejected instead
/// of being resolved against the process working directory.
pub const SYSFS_ROOT_ENV: &str = "MK61_SYSFS_ROOT";

/// Default sysfs mount point used when [`SYSFS_ROOT_ENV`] is unset.
pub const DEFAULT_SYSFS_ROOT: &str = mk61_service::DEFAULT_SYSFS_ROOT;

/// Linux controller for the Gamakay MK61 / SK61L keyboard.
#[derive(Debug, Parser)]
#[command(
    name = "mk61",
    version = mk61_core::VERSION,
    about = "Linux controller for the Gamakay MK61 / SK61L keyboard",
    long_about = "Linux controller for the Gamakay MK61 / SK61L keyboard.\n\n\
                  Run `mk61 help` for an overview of every planned command.",
    after_help = crate::output::HELP_FOOTER,
    after_long_help = crate::output::HELP_FOOTER,
    override_usage = "mk61 [GLOBAL OPTIONS] <COMMAND>",
    disable_help_subcommand = true,
    propagate_version = true
)]
pub struct Cli {
    /// Use this hidraw node instead of auto-detecting the keyboard.
    #[arg(long, global = true, value_name = "PATH")]
    pub device: Option<PathBuf>,

    /// Directory for backups and flash guard state.
    #[arg(long, global = true, value_name = "DIR")]
    pub state_dir: Option<PathBuf>,

    /// Print diagnostic details to stderr.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level commands available in the CLI.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print an overview of the planned commands.
    ///
    /// The overview lists every command, its arguments, and the current
    /// implementation status. Hardware is never accessed by this command.
    ///
    /// # Examples
    ///
    /// ```text
    /// mk61 help
    /// ```
    #[command(verbatim_doc_comment)]
    Help,

    /// List detected MK61 / SK61L keyboards.
    ///
    /// Scans `/sys/class/hidraw` for USB devices with vendor and product ID
    /// `1EA7:9018`, parses each report descriptor, and marks the vendor
    /// configuration interface. Product strings are sanitised before printing.
    /// This command is read-only: it never opens, reads from, or writes to a
    /// device node.
    ///
    /// Use `--device <PATH>` to inspect one explicit node. The path must be an
    /// absolute character device that is not a symbolic link; its VID/PID are
    /// still verified through sysfs.
    ///
    /// # Examples
    ///
    /// ```text
    /// mk61 list
    /// mk61 --device /dev/hidraw3 list
    /// ```
    #[command(verbatim_doc_comment)]
    List,

    /// LED lighting control.
    #[command(subcommand)]
    Led(LedCommand),

    /// Show device and capability status.
    ///
    /// Reports whether a daemon is supervising the keyboard, which keyboards
    /// were detected, and which capability switches the host granted. This
    /// command is read-only and never opens a device node.
    ///
    /// # Examples
    ///
    /// ```text
    /// mk61 status
    /// mk61 --device /dev/hidraw3 status
    /// ```
    #[command(verbatim_doc_comment)]
    Status,
}

/// LED lighting subcommands.
#[derive(Debug, Subcommand)]
pub enum LedCommand {
    /// List the LED effects supported by the keyboard.
    ///
    /// Prints the built-in effects with their colour slot counts and the custom
    /// per-key effect.
    ///
    /// # Examples
    ///
    /// ```text
    /// mk61 led effects
    /// ```
    #[command(verbatim_doc_comment)]
    Effects,

    /// Commit the active profile to flash.
    ///
    /// # Examples
    ///
    /// ```text
    /// mk61 led save
    /// ```
    #[command(verbatim_doc_comment)]
    Save,

    /// Set the brightness (0..=10).
    ///
    /// # Examples
    ///
    /// ```text
    /// mk61 led brightness 8 --save
    /// ```
    #[command(verbatim_doc_comment)]
    Brightness {
        /// Brightness value (0..=10).
        value: u8,
        /// Commit the change to flash after verification.
        #[arg(long)]
        save: bool,
    },

    /// Set the speed (2..=102, lower = faster).
    ///
    /// # Examples
    ///
    /// ```text
    /// mk61 led speed 50 --save
    /// ```
    #[command(verbatim_doc_comment)]
    Speed {
        /// Speed value (2..=102).
        value: u8,
        /// Commit the change to flash after verification.
        #[arg(long)]
        save: bool,
    },

    /// Set the direction (0..=3).
    ///
    /// # Examples
    ///
    /// ```text
    /// mk61 led direction 1 --save
    /// ```
    #[command(verbatim_doc_comment)]
    Direction {
        /// Direction value (0..=3).
        value: u8,
        /// Commit the change to flash after verification.
        #[arg(long)]
        save: bool,
    },

    /// Set palette colours for a built-in effect.
    ///
    /// Applies the colours to the active profile in RAM. Pass `--save` to commit
    /// the change to flash after a read-back verification.
    ///
    /// # Examples
    ///
    /// ```text
    /// mk61 led color wave FF0000 00FF00 0000FF --save
    /// ```
    #[command(verbatim_doc_comment)]
    Color {
        /// Effect name or numeric index.
        effect: String,
        /// One or more RRGGBB colours (repeated across the effect's slots).
        #[arg(value_name = "RRGGBB", required = true)]
        colors: Vec<String>,
        /// Commit the change to flash after verification.
        #[arg(long)]
        save: bool,
    },

    /// Select an LED effect.
    ///
    /// Applies the effect to the active profile in RAM. Pass `--save` to commit
    /// the change to flash after a read-back verification.
    ///
    /// # Examples
    ///
    /// ```text
    /// mk61 led effect wave
    /// mk61 led effect 5 --save
    /// ```
    #[command(verbatim_doc_comment)]
    Effect {
        /// Effect name or numeric index.
        spec: String,
        /// Commit the change to flash after verification.
        #[arg(long)]
        save: bool,
    },
}

impl Cli {
    /// Builds the service configuration implied by the global options.
    ///
    /// Explicit `--device` and `--state-dir` values override the defaults;
    /// safety settings such as the flash cooldown keep their standard values.
    /// The sysfs root is resolved from the hidden `MK61_SYSFS_ROOT` override and
    /// falls back to `/sys` only when it is unset or empty.
    pub fn service_config(&self) -> ServiceConfig {
        let mut config = ServiceConfig::default();
        if let Some(device) = &self.device {
            config.device_path = Some(device.clone());
        }
        if let Some(state_dir) = &self.state_dir {
            config.state_dir = state_dir.clone();
        }
        if let Ok(sysfs_root) = self.resolve_sysfs_root() {
            config.sysfs_root = sysfs_root;
        }
        config
    }

    /// Resolves the sysfs root used by device discovery.
    ///
    /// Returns the value of the hidden [`SYSFS_ROOT_ENV`] variable or the
    /// system default. Relative values are rejected so a misconfigured test or
    /// deployment cannot silently scan an unexpected tree.
    pub fn resolve_sysfs_root(&self) -> io::Result<PathBuf> {
        sysfs_root_from_env()
    }
}

/// Reads the sysfs root override from the environment.
///
/// A missing or empty variable selects [`DEFAULT_SYSFS_ROOT`]. Relative values
/// are rejected with [`io::ErrorKind::InvalidInput`] so a test or deployment
/// cannot accidentally scan a path relative to the current directory.
///
/// # Errors
///
/// Returns an [`io::Error`] when the override is present but not absolute.
pub fn sysfs_root_from_env() -> io::Result<PathBuf> {
    sysfs_root_from(std::env::var_os(SYSFS_ROOT_ENV))
}

/// Resolves an optional [`SYSFS_ROOT_ENV`] value.
///
/// This is the pure part of [`sysfs_root_from_env`], split out so tests do not
/// need to mutate the process environment.
///
/// # Errors
///
/// Returns an [`io::Error`] when `value` is present and not absolute.
pub fn sysfs_root_from(value: Option<OsString>) -> io::Result<PathBuf> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(PathBuf::from(DEFAULT_SYSFS_ROOT));
    };

    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{SYSFS_ROOT_ENV} must be an absolute path, got {}",
                path.display()
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn parses_the_help_command() {
        let cli = Cli::try_parse_from(["mk61", "help"]).unwrap();
        assert!(matches!(cli.command, Command::Help));
    }

    #[test]
    fn parses_the_list_command() {
        let cli = Cli::try_parse_from(["mk61", "list"]).unwrap();
        assert!(matches!(cli.command, Command::List));
    }

    #[test]
    fn parses_the_status_command() {
        let cli = Cli::try_parse_from(["mk61", "status"]).unwrap();
        assert!(matches!(cli.command, Command::Status));
    }

    #[test]
    fn parses_the_led_effects_command() {
        let cli = Cli::try_parse_from(["mk61", "led", "effects"]).unwrap();
        assert!(matches!(cli.command, Command::Led(LedCommand::Effects)));
    }

    #[test]
    fn parses_the_led_color_command() {
        let cli =
            Cli::try_parse_from(["mk61", "led", "color", "wave", "FF0000", "00FF00", "--save"])
                .unwrap();
        match cli.command {
            Command::Led(LedCommand::Color {
                effect,
                colors,
                save,
            }) => {
                assert_eq!(effect, "wave");
                assert_eq!(colors, vec!["FF0000", "00FF00"]);
                assert!(save);
            }
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn global_options_override_the_service_defaults() {
        let cli = Cli::try_parse_from([
            "mk61",
            "--device",
            "/dev/hidraw4",
            "--state-dir",
            "/tmp/mk61-state",
            "--verbose",
            "help",
        ])
        .unwrap();

        let config = cli.service_config();
        assert_eq!(
            config.device_path.as_deref(),
            Some(Path::new("/dev/hidraw4"))
        );
        assert_eq!(config.state_dir, PathBuf::from("/tmp/mk61-state"));
        assert!(cli.verbose);
    }

    #[test]
    fn defaults_keep_auto_detection_and_safety_settings() {
        let cli = Cli::try_parse_from(["mk61", "help"]).unwrap();
        let config = cli.service_config();

        assert!(config.device_path.is_none());
        assert_eq!(config.sysfs_root, PathBuf::from(DEFAULT_SYSFS_ROOT));
        assert_eq!(config.flash_cooldown, mk61_service::DEFAULT_FLASH_COOLDOWN);
        assert_eq!(config.read_retries, mk61_service::DEFAULT_READ_RETRIES);
    }

    #[test]
    fn sysfs_root_defaults_to_the_system_mount() {
        assert_eq!(
            sysfs_root_from(None).unwrap(),
            PathBuf::from(DEFAULT_SYSFS_ROOT)
        );
        assert_eq!(
            sysfs_root_from(Some(OsString::new())).unwrap(),
            PathBuf::from(DEFAULT_SYSFS_ROOT)
        );
    }

    #[test]
    fn sysfs_root_accepts_absolute_overrides() {
        assert_eq!(
            sysfs_root_from(Some(OsString::from("/tmp/fake-sysfs"))).unwrap(),
            PathBuf::from("/tmp/fake-sysfs")
        );
    }

    #[test]
    fn sysfs_root_rejects_relative_overrides() {
        let error = sysfs_root_from(Some(OsString::from("relative/sysfs"))).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains(SYSFS_ROOT_ENV));
    }

    #[test]
    fn unknown_command_is_a_usage_error() {
        assert!(Cli::try_parse_from(["mk61", "bogus"]).is_err());
    }
}
