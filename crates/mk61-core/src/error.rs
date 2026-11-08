//! Domain errors shared by the upper layers.

/// Errors produced while validating or transforming domain values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A profile number was outside the valid range `1..=4`.
    #[error("profile must be between 1 and 4, got {0}")]
    InvalidProfile(u8),
}

/// Convenience result type for the domain layer.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_profile_error_is_descriptive() {
        assert_eq!(
            Error::InvalidProfile(7).to_string(),
            "profile must be between 1 and 4, got 7"
        );
    }
}
