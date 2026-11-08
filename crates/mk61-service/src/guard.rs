//! Flash-commit cooldown state.
//!
//! Persisting and checking the timestamp is implemented in a later milestone.
//! This type fixes the state file location and cooldown now:
//! `$STATE_DIR/flash.state`, one successful commit per configured cooldown.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{ServiceConfig, ServiceError};

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

    /// Returns an error when a commit was recorded within the cooldown window.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::FlashCooldown`] when the remaining time has not
    /// elapsed, and [`ServiceError::Device`] on I/O failure.
    pub fn check(&self, now: SystemTime) -> Result<(), ServiceError> {
        if let Ok(contents) = fs::read_to_string(&self.path) {
            if let Ok(stored) = contents.trim().parse::<u128>() {
                let stored_time = UNIX_EPOCH + Duration::from_nanos(stored as u64);
                if let Ok(elapsed) = now.duration_since(stored_time) {
                    if elapsed < self.cooldown {
                        return Err(ServiceError::FlashCooldown {
                            remaining: self.cooldown - elapsed,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Records a successful commit timestamp.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::Device`] when the state file cannot be written.
    pub fn record(&self, now: SystemTime) -> Result<(), ServiceError> {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let nanos = now
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        fs::write(&self.path, nanos.to_string())
            .map_err(|e| ServiceError::Device(mk61_device::DeviceError::Io(e)))?;
        Ok(())
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
