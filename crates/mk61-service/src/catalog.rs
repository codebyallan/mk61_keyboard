//! Device catalogue used by the CLI and future GUI.
//!
//! The catalogue is a thin, typed wrapper around [`mk61_device::discover`]. It
//! takes the sysfs root as a constructor parameter so callers can use the real
//! `/sys` mount or an isolated fake tree without changing any code.

use std::path::{Path, PathBuf};

use mk61_device::{DeviceInfo, discover, discover_one};

use crate::ServiceError;

/// Lists and inspects MK61 / SK61L hidraw nodes.
///
/// The type performs read-only sysfs access. It never opens, reads from, or
/// writes to a hidraw device node, which makes it safe for `mk61 list` and for a
/// graphical device picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCatalog {
    sysfs_root: PathBuf,
}

impl DeviceCatalog {
    /// Creates a catalogue rooted at `sysfs_root`.
    ///
    /// The path is validated when [`DeviceCatalog::list`] or
    /// [`DeviceCatalog::inspect`] is called; it must be absolute.
    pub fn new(sysfs_root: impl Into<PathBuf>) -> Self {
        Self {
            sysfs_root: sysfs_root.into(),
        }
    }

    /// Creates a catalogue using the system sysfs mount (`/sys`).
    pub fn system() -> Self {
        Self::new(mk61_device::DEFAULT_SYSFS_ROOT)
    }

    /// Returns the sysfs root used for discovery.
    pub fn sysfs_root(&self) -> &Path {
        &self.sysfs_root
    }

    /// Returns every MK61 / SK61L node below the configured sysfs root.
    ///
    /// Non-configuration interfaces are included and marked with
    /// [`DeviceInfo::is_config_interface`] set to `false`.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::Device`] when the sysfs root is relative or the
    /// class directory cannot be listed.
    pub fn list(&self) -> Result<Vec<DeviceInfo>, ServiceError> {
        Ok(discover(&self.sysfs_root)?)
    }

    /// Inspects one explicit hidraw node.
    ///
    /// The node is validated and identified through sysfs but is never opened.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::Device`] when the path is invalid, sysfs metadata
    /// is missing or malformed, or the node is not a supported MK61.
    pub fn inspect(&self, device_path: &Path) -> Result<DeviceInfo, ServiceError> {
        Ok(discover_one(&self.sysfs_root, device_path)?)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
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
                "mk61-service-catalog-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("class/hidraw")).unwrap();
            Self { root }
        }

        fn add_node(&self, name: &str) {
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

    #[test]
    fn list_returns_typed_devices_with_the_config_flag() {
        let sysfs = FakeSysfs::new();
        sysfs.add_node("hidraw7");
        let catalog = DeviceCatalog::new(&sysfs.root);

        let devices = catalog.list().unwrap();

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].path, Path::new("/dev/hidraw7"));
        assert!(devices[0].is_config_interface);
        assert_eq!(devices[0].report_id, Some(CONFIG_REPORT_ID));
    }

    #[test]
    fn inspect_uses_the_same_sysfs_root() {
        let sysfs = FakeSysfs::new();
        sysfs.add_node("null");
        let catalog = DeviceCatalog::new(&sysfs.root);

        let device = catalog.inspect(Path::new("/dev/null")).unwrap();

        assert_eq!(device.path, Path::new("/dev/null"));
        assert!(device.is_config_interface);
    }

    #[test]
    fn relative_sysfs_root_is_rejected_by_the_service() {
        let catalog = DeviceCatalog::new("relative/sys");

        assert!(matches!(
            catalog.list(),
            Err(ServiceError::Device(DeviceError::InvalidSysfsRoot(_)))
        ));
    }

    #[test]
    fn system_catalogue_uses_the_standard_mount() {
        assert_eq!(DeviceCatalog::system().sysfs_root(), Path::new("/sys"));
    }
}
