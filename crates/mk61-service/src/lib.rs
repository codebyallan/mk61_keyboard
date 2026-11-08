//! Application services for the MK61 keyboard.
//!
//! This layer turns pure domain operations into use-cases: read the active
//! profile, apply LED settings, back up the previous block, and commit to flash
//! only when the safety rules allow it. It returns typed results instead of
//! printing anything, so the same functions can back the CLI today and a GUI or
//! a simulator later.
//!
//! Filesystem access is limited to backups and the flash cooldown guard.
//! Time and sleep are injected through [`Clock`] so tests are deterministic.

#![forbid(unsafe_code)]

pub mod backup;
pub mod catalog;
pub mod clock;
mod error;
pub mod guard;
pub mod service;

pub use backup::BackupStore;
pub use catalog::DeviceCatalog;
pub use clock::{Clock, SystemClock};
pub use error::ServiceError;
pub use guard::FlashGuard;
pub use mk61_device::{DEFAULT_SYSFS_ROOT, DeviceInfo, PRODUCT_ID, VENDOR_ID, sanitize_usb_string};
pub use service::{
    DEFAULT_FLASH_COOLDOWN, DEFAULT_READ_RETRIES, Service, ServiceConfig, default_state_dir,
};
