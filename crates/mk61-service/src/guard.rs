//! Flash-commit cooldown state.
//!
//! Persisting and checking the timestamp is implemented in a later milestone.
//! This type fixes the state file location and cooldown now:
//! `$STATE_DIR/flash.state`, one successful commit per configured cooldown.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::ServiceConfig;

/// Prevents rapid repeated writes to the keyboard's flash memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashGuard {
    path: PathBuf,
    cooldown: Duration,
}

impl FlashGuard {
    /// Creates a guard that stores its timestamp at `path` and enforces
    /// `cooldown` between commits.
    pub fn new(path: impl Into<PathBuf>, cooldown: Duration) -> Self {
        Self {
            path: path.into(),
            cooldown,
        }
    }

    /// Creates a guard at `<config.state_dir>/flash.state`.
    pub fn from_config(config: &ServiceConfig) -> Self {
        Self::new(config.state_dir.join("flash.state"), config.flash_cooldown)
    }

    /// Returns the state file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the configured cooldown.
    pub fn cooldown(&self) -> Duration {
        self.cooldown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_the_state_file_and_cooldown_from_config() {
        let config = ServiceConfig {
            state_dir: PathBuf::from("/state/mk61"),
            flash_cooldown: Duration::from_secs(45),
            ..ServiceConfig::default()
        };
        let guard = FlashGuard::from_config(&config);

        assert_eq!(guard.path(), Path::new("/state/mk61/flash.state"));
        assert_eq!(guard.cooldown(), Duration::from_secs(45));
    }
}
