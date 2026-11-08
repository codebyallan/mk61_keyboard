//! Backup storage locations for profile blocks.
//!
//! Saving and pruning are implemented in a later milestone. This type fixes the
//! layout now so configuration and tests agree on where backups live:
//! `$STATE_DIR/backups/`.

use std::path::{Path, PathBuf};

use crate::ServiceConfig;

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
