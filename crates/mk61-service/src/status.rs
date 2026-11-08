//! Device and capability status reporting.
//!
//! Status answers two questions without touching a device node: which keyboards
//! are present, and which capabilities the host granted. The report is returned
//! as data so the CLI renders it and a future GUI can display it.

use mk61_device::DeviceInfo;

use crate::ServiceConfig;
use crate::ServiceError;

/// Environment variable that toggles host-driven LED frames and effects.
pub const LED_CONTROL_ENV: &str = "KBDD_ENABLE_LED";

/// Environment variable that toggles profile and flash writes.
pub const PROFILE_WRITE_ENV: &str = "KBDD_ENABLE_PROFILE_WRITE";

/// Runtime capability switches, sourced from the environment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapabilityStatus {
    /// Whether host-driven LED frames and effects are allowed.
    pub led_control: bool,
    /// Whether profile and flash writes are allowed.
    pub profile_writes: bool,
}

impl CapabilityStatus {
    /// Returns a status where both capabilities are disabled.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            led_control: false,
            profile_writes: false,
        }
    }
}

/// Returns the capability switches for explicit flags.
#[must_use]
pub fn capabilities_from_flags(led_control: bool, profile_writes: bool) -> CapabilityStatus {
    CapabilityStatus {
        led_control,
        profile_writes,
    }
}

/// Reads the capability switches from the process environment.
#[must_use]
pub fn capabilities_from_env() -> CapabilityStatus {
    CapabilityStatus {
        led_control: env_flag(LED_CONTROL_ENV),
        profile_writes: env_flag(PROFILE_WRITE_ENV),
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| {
        value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
    })
}

/// Report produced by `mk61 status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusReport {
    /// Whether a supervising daemon process is running. The standalone CLI
    /// build talks HID directly, so this is always `false` today.
    pub daemon_running: bool,
    /// Discovered keyboard nodes.
    pub devices: Vec<DeviceInfo>,
    /// Capability switches as seen by the host.
    pub capabilities: CapabilityStatus,
}

impl StatusReport {
    /// Returns the number of discovered devices.
    #[must_use]
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Returns the supported MK61 devices in the report.
    pub fn supported_devices(&self) -> impl Iterator<Item = &DeviceInfo> + '_ {
        self.devices.iter().filter(|device| device.is_supported())
    }
}

/// Collects the current status from the configured source.
///
/// # Errors
///
/// Returns [`ServiceError::Device`] when the device source cannot be read.
pub fn collect_status(config: &ServiceConfig) -> Result<StatusReport, ServiceError> {
    let catalog = crate::DeviceCatalog::new(&config.sysfs_root);
    let devices = match &config.device_path {
        Some(device_path) => vec![catalog.inspect(device_path)?],
        None => catalog.list()?,
    };

    Ok(StatusReport {
        daemon_running: false,
        devices,
        capabilities: capabilities_from_env(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use mk61_device::{CONFIG_REPORT_ID, DeviceError};

    const UEVENT: &str = "HID_ID=0003:00001EA7:00009018\nHID_NAME=SK61L SK61L\n";
    const DESCRIPTOR: &[u8] = &[
        0x06,
        0x01,
        0xFF,
        0x09,
        0x01,
        0xA1,
        0x01,
        0x85,
        CONFIG_REPORT_ID,
        0x75,
        0x08,
        0x96,
        0x07,
        0x01,
        0xB1,
        0x02,
        0xC0,
    ];

    struct FakeSysfs {
        root: PathBuf,
    }

    impl FakeSysfs {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);

            let root = std::env::temp_dir().join(format!(
                "mk61-status-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("class/hidraw")).unwrap();
            Self { root }
        }

        fn add(&self, name: &str) {
            let node = self.root.join("class/hidraw").join(name).join("device");
            fs::create_dir_all(&node).unwrap();
            fs::write(node.join("uevent"), UEVENT).unwrap();
            fs::write(node.join("report_descriptor"), DESCRIPTOR).unwrap();
        }
    }

    impl Drop for FakeSysfs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn config_with_root(root: &std::path::Path) -> ServiceConfig {
        ServiceConfig {
            sysfs_root: root.to_path_buf(),
            ..ServiceConfig::default()
        }
    }

    #[test]
    fn capabilities_default_to_disabled() {
        let capabilities = capabilities_from_flags(false, false);

        assert!(!capabilities.led_control);
        assert!(!capabilities.profile_writes);
    }

    #[test]
    fn capabilities_accept_explicit_flags() {
        let capabilities = capabilities_from_flags(true, true);

        assert!(capabilities.led_control);
        assert!(capabilities.profile_writes);
    }

    #[test]
    fn status_reports_the_discovered_devices() {
        let sysfs = FakeSysfs::new();
        sysfs.add("hidraw7");
        let config = config_with_root(&sysfs.root);

        let report = collect_status(&config).unwrap();

        assert!(!report.daemon_running);
        assert_eq!(report.device_count(), 1);
        assert_eq!(report.supported_devices().count(), 1);
        assert_eq!(report.devices[0].path, std::path::Path::new("/dev/hidraw7"));
        assert_eq!(
            report.devices[0].report_id,
            Some(mk61_device::CONFIG_REPORT_ID)
        );
    }

    #[test]
    fn status_without_devices_reports_an_empty_system() {
        let config = ServiceConfig {
            sysfs_root: PathBuf::from("/nonexistent-mk61-sysfs"),
            ..ServiceConfig::default()
        };

        let report = collect_status(&config).unwrap();

        assert!(report.devices.is_empty());
        assert_eq!(report.device_count(), 0);
    }

    #[test]
    fn status_inspects_an_explicit_device() {
        let sysfs = FakeSysfs::new();
        sysfs.add("null");
        let config = ServiceConfig {
            device_path: Some(std::path::PathBuf::from("/dev/null")),
            sysfs_root: sysfs.root.clone(),
            ..ServiceConfig::default()
        };

        let report = collect_status(&config).unwrap();

        assert_eq!(report.device_count(), 1);
    }

    #[test]
    fn status_rejects_a_relative_sysfs_root() {
        let config = ServiceConfig {
            sysfs_root: PathBuf::from("relative/sys"),
            ..ServiceConfig::default()
        };

        assert!(matches!(
            collect_status(&config),
            Err(ServiceError::Device(DeviceError::InvalidSysfsRoot(_)))
        ));
    }
}
