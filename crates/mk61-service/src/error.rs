//! Service-layer errors.

use std::time::Duration;

/// Errors produced by keyboard use-cases.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// A pure domain validation failed.
    #[error(transparent)]
    Core(#[from] mk61_core::Error),

    /// The device could not be reached or returned unusable data.
    #[error(transparent)]
    Device(#[from] mk61_device::DeviceError),

    /// A flash commit was attempted during the cooldown window.
    #[error("flash commit blocked for another {remaining:?}")]
    FlashCooldown {
        /// Time left before another commit is allowed.
        remaining: Duration,
    },

    /// The operation is reserved for a later milestone.
    #[error("not implemented yet: {0}")]
    Unsupported(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_errors_are_transparently_wrapped() {
        let error = ServiceError::from(mk61_core::Error::InvalidProfile(0));
        assert_eq!(error.to_string(), "profile must be between 1 and 4, got 0");
    }

    #[test]
    fn cooldown_error_reports_the_remaining_time() {
        let error = ServiceError::FlashCooldown {
            remaining: Duration::from_secs(12),
        };
        assert_eq!(error.to_string(), "flash commit blocked for another 12s");
    }
}
