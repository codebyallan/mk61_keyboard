//! Backup storage locations for profile blocks.
//!
//! Saving and pruning are implemented in a later milestone. This type fixes the
//! layout now so configuration and tests agree on where backups live:
//! `$STATE_DIR/backups/`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{ServiceConfig, ServiceError};

/// Directory where 256-byte profile backups are stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupStore {
    directory: PathBuf,
}

impl BackupStore {
    /// Creates a store rooted at `directory`.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// Creates a store at `<config.state_dir>/backups`.
    pub fn from_config(config: &ServiceConfig) -> Self {
        Self::new(config.state_dir.join("backups"))
    }

    /// Returns the backup directory.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Persists `data` under a unique file name and returns its path.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::Device`] (wrapping the I/O error) when the
    /// directory cannot be created or the file cannot be written.
    pub fn save(&self, label: &str, data: &[u8]) -> Result<PathBuf, ServiceError> {
        fs::create_dir_all(&self.directory)
            .map_err(|e| ServiceError::Device(mk61_device::DeviceError::Io(e)))?;
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = self.directory.join(format!("{label}-{id}.bin"));
        fs::write(&path, data)
            .map_err(|e| ServiceError::Device(mk61_device::DeviceError::Io(e)))?;
        Ok(path)
    }

    /// Reads a previously stored backup.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::Device`] when the file cannot be read.
    pub fn load(&self, path: &Path) -> Result<Vec<u8>, ServiceError> {
        fs::read(path).map_err(|e| ServiceError::Device(mk61_device::DeviceError::Io(e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_backups_below_the_state_directory() {
        let config = ServiceConfig {
            state_dir: PathBuf::from("/state/mk61"),
            ..ServiceConfig::default()
        };

        assert_eq!(
            BackupStore::from_config(&config).directory(),
            Path::new("/state/mk61/backups")
        );
    }
}
