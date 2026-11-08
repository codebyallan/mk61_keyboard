//! Linux-side access to the Gamakay MK61 / SK61L keyboard.
//!
//! This crate owns everything that talks to the operating system: enumerating
//! hidraw nodes through sysfs, matching the vendor configuration interface, and
//! moving 264-byte feature reports. Protocol buffers themselves live in
//! `mk61-core`; this crate only carries bytes.
//!
//! Discovery is implemented; the real hidraw transport arrives in a later
//! milestone. A deterministic [`MockTransport`] is always available for tests and
//! future GUI demos. This is the only crate allowed to contain `unsafe` code,
//! and every such block must carry a `SAFETY` comment.
//!
//! Sysfs and device strings are untrusted. Discovery reads bounded amounts of
//! data and product strings are sanitised with [`sanitize_usb_string`] before
//! they appear in [`DeviceInfo`].

pub mod descriptor;
pub mod discovery;
pub mod mock;
pub mod sanitize;
pub mod transport;

mod error;

pub use descriptor::{
    CONFIG_REPORT_ID, DescriptorError, HidCollection, HidDescriptor, InterfaceDescriptor,
    MAX_REPORT_DESCRIPTOR_BYTES, parse_report_descriptor,
};
pub use discovery::{
    DeviceInfo, UeventError, UeventInfo, discover, discover_one, parse_hid_uevent,
};
pub use error::DeviceError;
pub use mock::MockTransport;
pub use sanitize::{MAX_USB_STRING_CHARS, sanitize_usb_string};
pub use transport::{FEATURE_REPORT_ID, FEATURE_REPORT_SIZE, HidrawTransport, Transport};

/// Default Linux sysfs mount point used for discovery.
pub const DEFAULT_SYSFS_ROOT: &str = "/sys";

/// USB vendor ID of Gamakay devices.
pub const VENDOR_ID: u16 = 0x1EA7;

/// USB product ID of the MK61 / SK61L.
pub const PRODUCT_ID: u16 = 0x9018;

/// Product string reported by the reference device.
pub const PRODUCT_NAME: &str = "SK61L";
