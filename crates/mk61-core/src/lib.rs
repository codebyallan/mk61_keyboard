//! Pure domain model for the Gamakay MK61 / SK61L keyboard.
//!
//! This crate contains no I/O and forbids `unsafe` code. Everything that can be
//! expressed as a value transformation belongs here: protocol buffers, LED
//! effects, colour palettes, settings profiles, key maps and diffs. Keeping this
//! layer pure makes it fast to unit-test and reusable from a CLI, a GUI, or a
//! simulator.
//!
//! The public API grows milestone by milestone. At the moment it exposes the
//! crate version, the domain error type, the on-board profile identifier, the
//! LED colour type and the built-in effect catalogue.

#![forbid(unsafe_code)]

mod color;
mod effect;
mod error;
mod keys;
mod profile;
mod protocol;

pub use color::Rgb;
pub use effect::{
    CUSTOM, CUSTOM_EFFECT_ID, ColorSlots, EFFECT_COUNT, EFFECTS, Effect, WAVE_COLOR_COUNT,
    custom_effect, effect_by_index, effect_by_name, effect_color_slots, resolve_effect,
};
pub use error::{Error, Result};
pub use keys::{KEY_NAMES, KEY_POSITIONS, KEY_ROWS, key_column, key_name, key_position, key_row};
pub use profile::Profile;
pub use protocol::*;

/// Version of the core crate, taken from `Cargo.toml` at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_package_metadata() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn version_is_not_empty() {
        assert!(!VERSION.is_empty());
    }
}
