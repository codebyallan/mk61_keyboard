//! Pure domain model for the Gamakay MK61 / SK61L keyboard.
//!
//! This crate contains no I/O and forbids `unsafe` code. Everything that can be
//! expressed as a value transformation belongs here: protocol buffers, LED
//! effects, colour palettes, settings profiles, key maps and diffs. Keeping this
//! layer pure makes it fast to unit-test and reusable from a CLI, a GUI, or a
//! simulator.
//!
//! The public API grows milestone by milestone. At the moment it exposes the
//! crate version, the domain error type and the on-board profile identifier.

#![forbid(unsafe_code)]

mod error;
mod profile;

pub use error::{Error, Result};
pub use profile::Profile;

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
