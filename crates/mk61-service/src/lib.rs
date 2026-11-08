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
pub mod status;

pub use backup::BackupStore;
pub use catalog::DeviceCatalog;
pub use clock::{Clock, SystemClock};
pub use error::ServiceError;
pub use guard::FlashGuard;
pub use mk61_core::{
    CUSTOM, CUSTOM_EFFECT_ID, ColorSlots, EFFECT_COUNT, EFFECTS, Effect, WAVE_COLOR_COUNT,
    custom_effect, effect_by_index, effect_by_name, effect_color_slots, resolve_effect,
};
pub use mk61_device::{
    DEFAULT_SYSFS_ROOT, DeviceError, DeviceInfo, HidrawTransport, PRODUCT_ID, VENDOR_ID,
    sanitize_usb_string,
};
pub use service::{
    ApplyLedEffectResult, DEFAULT_FLASH_COOLDOWN, DEFAULT_READ_RETRIES, Service, ServiceConfig,
    default_state_dir, led_effects,
};
pub use status::{
    CapabilityStatus, LED_CONTROL_ENV, PROFILE_WRITE_ENV, StatusReport, capabilities_from_env,
    capabilities_from_flags, collect_status,
};
