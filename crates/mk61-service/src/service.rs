//! Service configuration and composition root.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use mk61_device::{DEFAULT_SYSFS_ROOT, Transport};

/// Minimum time between two flash commits.
pub const DEFAULT_FLASH_COOLDOWN: Duration = Duration::from_secs(30);

/// Number of read attempts before a response is considered unusable.
pub const DEFAULT_READ_RETRIES: u32 = 3;

/// Configuration shared by every service operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    /// Explicit hidraw path, or `None` to auto-detect a device.
    pub device_path: Option<PathBuf>,
    /// Linux sysfs mount used by device discovery.
    pub sysfs_root: PathBuf,
    /// Directory holding backups and the flash guard state.
    pub state_dir: PathBuf,
    /// Minimum time between two successful flash commits.
    pub flash_cooldown: Duration,
    /// Number of attempts when reading a report back from the device.
    pub read_retries: u32,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            device_path: None,
            sysfs_root: PathBuf::from(DEFAULT_SYSFS_ROOT),
            state_dir: default_state_dir(),
            flash_cooldown: DEFAULT_FLASH_COOLDOWN,
            read_retries: DEFAULT_READ_RETRIES,
        }
    }
}

/// Returns the default state directory for backups and flash guard state.
///
/// Follows the XDG base directory specification:
/// `$XDG_STATE_HOME/mk61`, falling back to `$HOME/.local/state/mk61`. Relative
/// or empty values are ignored, as required by the specification. If neither
/// variable yields an absolute path (for example in a minimal container), a
/// per-system temporary directory is used as a last resort.
pub fn default_state_dir() -> PathBuf {
    state_dir_from(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

fn state_dir_from(xdg_state_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
    if let Some(directory) = non_empty_path(xdg_state_home) {
        return directory.join("mk61");
    }
    if let Some(directory) = non_empty_path(home) {
        return directory.join(".local").join("state").join("mk61");
    }
    std::env::temp_dir().join("mk61")
}

fn non_empty_path(value: Option<OsString>) -> Option<PathBuf> {
    value
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Coordinates protocol operations with device transport and safety state.
///
/// The type is intentionally thin in this milestone: it holds the configuration
/// and transport that later use-cases will consume. Constructing a service does
/// not touch the device.
#[derive(Debug)]
pub struct Service<T> {
    config: ServiceConfig,
    transport: T,
}

impl<T: Transport> Service<T> {
    /// Creates a service from configuration and an already open transport.
    pub fn new(config: ServiceConfig, transport: T) -> Self {
        Self { config, transport }
    }

    /// Returns the configuration used by this service.
    pub fn config(&self) -> &ServiceConfig {
        &self.config
    }

    /// Returns the transport owned by this service.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Returns mutable access to the transport owned by this service.
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mk61_device::MockTransport;

    #[test]
    fn default_config_uses_the_verified_safety_values() {
        let config = ServiceConfig::default();

        assert_eq!(config.flash_cooldown, Duration::from_secs(30));
        assert_eq!(config.read_retries, 3);
        assert_eq!(config.state_dir.file_name().unwrap(), "mk61");
    }

    #[test]
    fn xdg_state_home_takes_precedence() {
        let directory = state_dir_from(
            Some(OsString::from("/var/lib/user-state")),
            Some(OsString::from("/home/user")),
        );

        assert_eq!(directory, PathBuf::from("/var/lib/user-state/mk61"));
    }

    #[test]
    fn home_is_the_fallback() {
        let directory = state_dir_from(None, Some(OsString::from("/home/user")));

        assert_eq!(directory, PathBuf::from("/home/user/.local/state/mk61"));
    }

    #[test]
    fn empty_variables_are_ignored() {
        let directory = state_dir_from(Some(OsString::new()), Some(OsString::from("/home/user")));

        assert_eq!(directory, PathBuf::from("/home/user/.local/state/mk61"));
    }

    #[test]
    fn relative_state_variables_are_ignored() {
        let directory = state_dir_from(
            Some(OsString::from("relative-state")),
            Some(OsString::from("/home/user")),
        );

        assert_eq!(directory, PathBuf::from("/home/user/.local/state/mk61"));
    }

    #[test]
    fn service_keeps_configuration_and_transport_together() {
        let config = ServiceConfig::default();
        let mut service = Service::new(config.clone(), MockTransport::new());

        assert_eq!(service.config(), &config);
        service
            .transport_mut()
            .write_feature(&[0x07])
            .expect("mock writes cannot fail");
        assert_eq!(service.transport().writes(), &[vec![0x07]]);
    }
}
