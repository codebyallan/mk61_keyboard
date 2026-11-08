//! Discovery of supported keyboards on the local system.
//!
//! Discovery is read-only and based entirely on sysfs:
//!
//! ```text
//! <sysfs-root>/class/hidraw/hidrawN/device/uevent
//! <sysfs-root>/class/hidraw/hidrawN/device/report_descriptor
//! ```
//!
//! The sysfs root is a parameter so tests can use an ordinary temporary
//! directory instead of `/sys`. The CLI reads the hidden `MK61_SYSFS_ROOT`
//! override at its edge and passes the resulting path here.
//!
//! # Untrusted data
//!
//! Everything below sysfs is untrusted. Reads are bounded, malformed uevent
//! lines are skipped during a scan, malformed descriptors mark the node as
//! non-config instead of failing discovery, and product strings are sanitised
//! before they reach [`DeviceInfo`].

use std::fs;
use std::io::Read;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

use crate::descriptor::{MAX_REPORT_DESCRIPTOR_BYTES, parse_report_descriptor};
use crate::sanitize::sanitize_usb_string;
use crate::{DeviceError, PRODUCT_ID, VENDOR_ID};

/// Maximum number of uevent bytes read from sysfs.
const MAX_UEVENT_BYTES: u64 = 16 * 1024;

/// Metadata parsed from a hidraw node's `device/uevent` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UeventInfo {
    /// HID bus type, normally `0x0003` for USB.
    pub bus: u16,
    /// USB vendor ID.
    pub vendor_id: u16,
    /// USB product ID.
    pub product_id: u16,
    /// Raw product string, if the kernel exposes one.
    pub name: Option<String>,
}

/// Errors produced while parsing a hidraw `uevent` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UeventError {
    /// The file does not contain a `HID_ID` line.
    #[error("missing HID_ID line")]
    MissingHidId,

    /// The `HID_ID` line is not `BUS:VENDOR:PRODUCT` in hexadecimal.
    #[error("malformed HID_ID line")]
    MalformedHidId,
}

/// Metadata for a HID node that may be an MK61 configuration interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Path to the hidraw node, for example `/dev/hidraw3`.
    pub path: PathBuf,
    /// USB vendor ID.
    pub vendor_id: u16,
    /// USB product ID.
    pub product_id: u16,
    /// Sanitised product string, when the kernel exposes one.
    pub product: Option<String>,
    /// Whether the report descriptor contains the vendor configuration
    /// collection (usage page `0xFF01`, usage `0x0001`).
    pub is_config_interface: bool,
    /// Feature report ID of the vendor collection, when the descriptor exposes
    /// one. The verified MK61 value is `0x07`.
    pub report_id: Option<u8>,
}

impl DeviceInfo {
    /// Returns whether this node has the supported MK61 / SK61L vendor and
    /// product IDs.
    pub fn is_mk61(&self) -> bool {
        self.vendor_id == VENDOR_ID && self.product_id == PRODUCT_ID
    }

    /// Returns whether this node has the supported IDs and exposes the vendor
    /// configuration interface.
    pub fn is_supported(&self) -> bool {
        self.is_mk61() && self.is_config_interface
    }
}

/// Enumerates MK61 / SK61L hidraw nodes below `sysfs_root`.
///
/// Every node whose `HID_ID` matches the supported vendor and product IDs is
/// returned, including nodes that do not expose the configuration interface.
/// Such nodes are still useful metadata and are marked with
/// [`DeviceInfo::is_config_interface`] set to `false`.
///
/// A missing `class/hidraw` directory is treated as an empty system. Individual
/// nodes with unreadable or malformed metadata are skipped so one broken device
/// cannot hide the others.
///
/// # Errors
///
/// Returns [`DeviceError::InvalidSysfsRoot`] when `sysfs_root` is relative and
/// [`DeviceError::Io`] for unexpected errors while listing the class directory.
pub fn discover(sysfs_root: &Path) -> Result<Vec<DeviceInfo>, DeviceError> {
    require_absolute_root(sysfs_root)?;
    let class_directory = sysfs_root.join("class").join("hidraw");

    let entries = match fs::read_dir(&class_directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(DeviceError::Io(error)),
    };

    let mut devices = Vec::new();
    for entry in entries {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if !is_hidraw_node_name(&name) {
            continue;
        }
        if let Some(device) = read_node(sysfs_root, &name) {
            devices.push(device);
        }
    }

    devices.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(devices)
}

/// Inspects one explicit hidraw node.
///
/// The path must be absolute, must not be a symbolic link, and must refer to an
/// existing character device. The node is never opened: VID/PID and descriptor
/// metadata are read from the matching sysfs entry. The device must have the
/// supported MK61 vendor and product IDs.
///
/// # Errors
///
/// Returns a [`DeviceError`] variant when the path fails validation, the sysfs
/// metadata is missing or malformed, or the node is not a supported MK61.
pub fn discover_one(sysfs_root: &Path, device_path: &Path) -> Result<DeviceInfo, DeviceError> {
    require_absolute_root(sysfs_root)?;

    if !device_path.is_absolute() {
        return Err(DeviceError::DevicePathNotAbsolute(
            device_path.to_path_buf(),
        ));
    }

    let link_metadata =
        fs::symlink_metadata(device_path).map_err(|source| DeviceError::DevicePathIo {
            path: device_path.to_path_buf(),
            source,
        })?;
    if link_metadata.file_type().is_symlink() {
        return Err(DeviceError::DevicePathIsSymlink(device_path.to_path_buf()));
    }

    let metadata = fs::metadata(device_path).map_err(|source| DeviceError::DevicePathIo {
        path: device_path.to_path_buf(),
        source,
    })?;
    if !metadata.file_type().is_char_device() {
        return Err(DeviceError::NotCharacterDevice(device_path.to_path_buf()));
    }

    let Some(name) = device_path.file_name().and_then(|name| name.to_str()) else {
        return Err(DeviceError::InvalidDeviceName(device_path.to_path_buf()));
    };
    if name.is_empty() || name == "." || name == ".." {
        return Err(DeviceError::InvalidDeviceName(device_path.to_path_buf()));
    }

    let node_directory = sysfs_root.join("class").join("hidraw").join(name);
    let uevent_path = node_directory.join("device").join("uevent");
    let uevent_bytes =
        read_bounded(&uevent_path, MAX_UEVENT_BYTES).map_err(|source| DeviceError::SysfsIo {
            path: uevent_path.clone(),
            source,
        })?;
    let uevent_text = String::from_utf8_lossy(&uevent_bytes);
    let uevent = parse_hid_uevent(&uevent_text).map_err(|error| DeviceError::MalformedSysfs {
        path: uevent_path,
        reason: error.to_string(),
    })?;

    if uevent.vendor_id != VENDOR_ID || uevent.product_id != PRODUCT_ID {
        return Err(DeviceError::UnsupportedDevice(device_path.to_path_buf()));
    }

    let descriptor = read_bounded(
        &node_directory.join("device").join("report_descriptor"),
        MAX_REPORT_DESCRIPTOR_BYTES,
    )
    .ok()
    .and_then(|bytes| parse_report_descriptor(&bytes).ok());

    Ok(device_info(device_path.to_path_buf(), uevent, descriptor))
}

/// Parses the `HID_ID` and `HID_NAME` lines of a hidraw `uevent` file.
///
/// Hexadecimal digits are accepted in either case and may carry leading zeros.
/// Unknown lines are ignored and `HID_NAME` is optional. A `HID_NAME` value is
/// returned unmodified; callers must pass it through
/// [`crate::sanitize_usb_string`] before display.
///
/// # Errors
///
/// Returns [`UeventError::MissingHidId`] when no `HID_ID` line is present and
/// [`UeventError::MalformedHidId`] when its value is not three hexadecimal
/// fields.
pub fn parse_hid_uevent(text: &str) -> Result<UeventInfo, UeventError> {
    let mut hid_id = None;
    let mut name = None;

    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(value) = line.strip_prefix("HID_ID=") {
            hid_id = Some(parse_hid_id(value)?);
        } else if let Some(value) = line.strip_prefix("HID_NAME=") {
            name = Some(value.to_owned());
        } else if line.starts_with("HID_ID") && !line.contains('=') {
            return Err(UeventError::MalformedHidId);
        }
    }

    let (bus, vendor_id, product_id) = hid_id.ok_or(UeventError::MissingHidId)?;
    Ok(UeventInfo {
        bus,
        vendor_id,
        product_id,
        name,
    })
}

fn parse_hid_id(value: &str) -> Result<(u16, u16, u16), UeventError> {
    let mut parts = value.trim().split(':');
    let bus = parts
        .next()
        .and_then(parse_hex_u16)
        .ok_or(UeventError::MalformedHidId)?;
    let vendor_id = parts
        .next()
        .and_then(parse_hex_u16)
        .ok_or(UeventError::MalformedHidId)?;
    let product_id = parts
        .next()
        .and_then(parse_hex_u16)
        .ok_or(UeventError::MalformedHidId)?;

    if parts.next().is_some() {
        return Err(UeventError::MalformedHidId);
    }

    Ok((bus, vendor_id, product_id))
}

fn parse_hex_u16(value: &str) -> Option<u16> {
    let value = value.trim();
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u16::from_str_radix(value, 16).ok()
}

fn read_node(sysfs_root: &Path, name: &str) -> Option<DeviceInfo> {
    let node_directory = sysfs_root.join("class").join("hidraw").join(name);
    let uevent_bytes = read_bounded(
        &node_directory.join("device").join("uevent"),
        MAX_UEVENT_BYTES,
    )
    .ok()?;
    let uevent_text = String::from_utf8_lossy(&uevent_bytes);
    let uevent = parse_hid_uevent(&uevent_text).ok()?;

    if uevent.vendor_id != VENDOR_ID || uevent.product_id != PRODUCT_ID {
        return None;
    }

    let descriptor = read_bounded(
        &node_directory.join("device").join("report_descriptor"),
        MAX_REPORT_DESCRIPTOR_BYTES,
    )
    .ok()
    .and_then(|bytes| parse_report_descriptor(&bytes).ok());

    Some(device_info(
        Path::new("/dev").join(name),
        uevent,
        descriptor,
    ))
}

fn device_info(
    path: PathBuf,
    uevent: UeventInfo,
    descriptor: Option<crate::descriptor::HidDescriptor>,
) -> DeviceInfo {
    let product = uevent
        .name
        .map(|name| sanitize_usb_string(name.as_bytes()))
        .filter(|name| !name.is_empty());
    let (is_config_interface, report_id) = match descriptor {
        Some(descriptor) if descriptor.is_mk61_config() => (true, descriptor.mk61_report_id()),
        _ => (false, None),
    };

    DeviceInfo {
        path,
        vendor_id: uevent.vendor_id,
        product_id: uevent.product_id,
        product,
        is_config_interface,
        report_id,
    }
}

fn require_absolute_root(sysfs_root: &Path) -> Result<(), DeviceError> {
    if sysfs_root.is_absolute() {
        Ok(())
    } else {
        Err(DeviceError::InvalidSysfsRoot(sysfs_root.to_path_buf()))
    }
}

fn is_hidraw_node_name(name: &str) -> bool {
    name.strip_prefix("hidraw").is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?.take(limit);
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::descriptor::CONFIG_REPORT_ID;

    const MK61_UEVENT: &str =
        "DRIVER=hid-generic\nHID_ID=0003:00001EA7:00009018\nHID_NAME=SK61L SK61L\n";

    fn config_descriptor() -> Vec<u8> {
        vec![
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
        ]
    }

    struct FakeSysfs {
        root: PathBuf,
    }

    impl FakeSysfs {
        fn new(label: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);

            let root = std::env::temp_dir().join(format!(
                "mk61-device-{label}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("class/hidraw")).unwrap();
            Self { root }
        }

        fn add(&self, name: &str, uevent: &str, descriptor: Option<&[u8]>) {
            let node = self.root.join("class/hidraw").join(name);
            fs::create_dir_all(node.join("device")).unwrap();
            fs::write(node.join("device/uevent"), uevent).unwrap();
            if let Some(descriptor) = descriptor {
                fs::write(node.join("device/report_descriptor"), descriptor).unwrap();
            }
        }

        fn path(&self) -> &Path {
            &self.root
        }
    }

    impl Drop for FakeSysfs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn discovers_only_mk61_nodes_and_marks_the_config_interface() {
        let sysfs = FakeSysfs::new("discover");
        sysfs.add(
            "hidraw0",
            "HID_ID=0003:00001BCF:000008A0\nHID_NAME=Other keyboard\n",
            None,
        );
        sysfs.add("hidraw1", MK61_UEVENT, None);
        sysfs.add("hidraw2", MK61_UEVENT, Some(&config_descriptor()));

        let devices = discover(sysfs.path()).unwrap();

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].path, PathBuf::from("/dev/hidraw1"));
        assert!(devices[0].is_mk61());
        assert!(!devices[0].is_config_interface);
        assert_eq!(devices[0].product.as_deref(), Some("SK61L SK61L"));

        assert_eq!(devices[1].path, PathBuf::from("/dev/hidraw2"));
        assert!(devices[1].is_config_interface);
        assert!(devices[1].is_supported());
        assert_eq!(devices[1].report_id, Some(CONFIG_REPORT_ID));
    }

    #[test]
    fn malformed_descriptor_does_not_fail_discovery() {
        let sysfs = FakeSysfs::new("malformed-descriptor");
        sysfs.add("hidraw0", MK61_UEVENT, Some(&[0x06, 0x01]));

        let devices = discover(sysfs.path()).unwrap();

        assert_eq!(devices.len(), 1);
        assert!(!devices[0].is_config_interface);
        assert_eq!(devices[0].report_id, None);
    }

    #[test]
    fn skips_malformed_and_incomplete_uevents() {
        let sysfs = FakeSysfs::new("malformed-uevent");
        sysfs.add("hidraw0", "HID_NAME=missing id\n", None);
        sysfs.add("hidraw1", "HID_ID=0003:00001EA7\n", None);
        sysfs.add("hidraw2", "HID_ID=0003:ZZZZ:9018\n", None);
        sysfs.add("hidraw3", MK61_UEVENT, None);

        let devices = discover(sysfs.path()).unwrap();

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].path, PathBuf::from("/dev/hidraw3"));
    }

    #[test]
    fn missing_class_directory_is_an_empty_system() {
        let root = std::env::temp_dir().join(format!(
            "mk61-device-missing-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_dir_all(&root);

        assert!(discover(&root).unwrap().is_empty());
    }

    #[test]
    fn relative_sysfs_root_is_rejected() {
        assert!(matches!(
            discover(Path::new("sys")),
            Err(DeviceError::InvalidSysfsRoot(path)) if path == Path::new("sys")
        ));
    }

    #[test]
    fn parses_uevent_ids_case_insensitively_with_leading_zeros() {
        let info = parse_hid_uevent("HID_ID=0003:00001ea7:00009018\r\nHID_NAME=SK61L\r\n").unwrap();

        assert_eq!(info.bus, 0x0003);
        assert_eq!(info.vendor_id, VENDOR_ID);
        assert_eq!(info.product_id, PRODUCT_ID);
        assert_eq!(info.name.as_deref(), Some("SK61L"));
    }

    #[test]
    fn rejects_malformed_uevent_ids() {
        for text in [
            "",
            "HID_NAME=SK61L\n",
            "HID_ID=0003:00001EA7\n",
            "HID_ID=0003:00001EA7:00009018:00\n",
            "HID_ID=0003:not-hex:00009018\n",
            "HID_ID=0003:10000:00009018\n",
        ] {
            assert!(
                matches!(
                    parse_hid_uevent(text),
                    Err(UeventError::MissingHidId | UeventError::MalformedHidId)
                ),
                "unexpected result for {text:?}"
            );
        }
    }

    #[test]
    fn product_names_are_sanitised_in_device_info() {
        let sysfs = FakeSysfs::new("sanitise");
        sysfs.add(
            "hidraw0",
            "HID_ID=0003:00001EA7:00009018\nHID_NAME=SK61L\x1b[31m EVIL\x07\n",
            None,
        );

        let devices = discover(sysfs.path()).unwrap();

        assert_eq!(devices[0].product.as_deref(), Some("SK61L EVIL"));
    }

    #[test]
    fn explicit_path_must_be_absolute() {
        let sysfs = FakeSysfs::new("explicit-relative");
        assert!(matches!(
            discover_one(sysfs.path(), Path::new("dev/hidraw0")),
            Err(DeviceError::DevicePathNotAbsolute(_))
        ));
    }

    #[test]
    fn explicit_path_must_exist() {
        let sysfs = FakeSysfs::new("explicit-missing");
        assert!(matches!(
            discover_one(sysfs.path(), Path::new("/dev/mk61-does-not-exist")),
            Err(DeviceError::DevicePathIo { .. })
        ));
    }

    #[test]
    fn explicit_path_must_be_a_character_device() {
        let sysfs = FakeSysfs::new("explicit-file");
        let regular_file = sysfs.path().join("regular");
        fs::write(&regular_file, b"not a device").unwrap();

        assert!(matches!(
            discover_one(sysfs.path(), &regular_file),
            Err(DeviceError::NotCharacterDevice(_))
        ));
    }

    #[test]
    fn explicit_path_must_not_be_a_symlink() {
        let sysfs = FakeSysfs::new("explicit-symlink");
        let link = sysfs.path().join("device-link");
        std::os::unix::fs::symlink("/dev/null", &link).unwrap();

        assert!(matches!(
            discover_one(sysfs.path(), &link),
            Err(DeviceError::DevicePathIsSymlink(_))
        ));
    }

    #[test]
    fn explicit_character_device_is_verified_through_sysfs() {
        let sysfs = FakeSysfs::new("explicit-ok");
        sysfs.add("null", MK61_UEVENT, Some(&config_descriptor()));

        let device = discover_one(sysfs.path(), Path::new("/dev/null")).unwrap();

        assert_eq!(device.path, Path::new("/dev/null"));
        assert!(device.is_config_interface);
        assert_eq!(device.report_id, Some(CONFIG_REPORT_ID));
    }

    #[test]
    fn explicit_device_must_be_an_mk61() {
        let sysfs = FakeSysfs::new("explicit-other");
        sysfs.add(
            "null",
            "HID_ID=0003:00001BCF:000008A0\nHID_NAME=Other\n",
            None,
        );

        assert!(matches!(
            discover_one(sysfs.path(), Path::new("/dev/null")),
            Err(DeviceError::UnsupportedDevice(_))
        ));
    }

    #[test]
    fn explicit_device_requires_sysfs_metadata() {
        let sysfs = FakeSysfs::new("explicit-no-metadata");

        assert!(matches!(
            discover_one(sysfs.path(), Path::new("/dev/null")),
            Err(DeviceError::SysfsIo { .. })
        ));
    }

    #[test]
    fn explicit_device_requires_well_formed_sysfs_metadata() {
        let sysfs = FakeSysfs::new("explicit-bad-metadata");
        sysfs.add("null", "HID_NAME=broken\n", None);

        assert!(matches!(
            discover_one(sysfs.path(), Path::new("/dev/null")),
            Err(DeviceError::MalformedSysfs { .. })
        ));
    }
}
