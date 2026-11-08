//! Domain errors shared by the upper layers.

/// Errors produced while validating or transforming domain values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A profile number was outside the valid range `1..=4`.
    #[error("profile must be between 1 and 4, got {0}")]
    InvalidProfile(u8),

    /// The requested LED effect is not known to this build.
    #[error("unknown LED effect")]
    UnknownEffect,

    /// The requested LED effect has no colour slots.
    #[error("LED effect has no colour slots")]
    EffectWithoutColors,

    /// An RGB colour value was malformed.
    #[error("invalid RGB colour, expected RRGGBB")]
    InvalidColor,

    /// The requested number of colours is outside the valid range.
    #[error("invalid LED colour count")]
    InvalidColorCount,
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

    #[test]
    fn led_errors_are_descriptive() {
        assert_eq!(Error::UnknownEffect.to_string(), "unknown LED effect");
        assert_eq!(
            Error::EffectWithoutColors.to_string(),
            "LED effect has no colour slots"
        );
        assert_eq!(
            Error::InvalidColor.to_string(),
            "invalid RGB colour, expected RRGGBB"
        );
        assert_eq!(
            Error::InvalidColorCount.to_string(),
            "invalid LED colour count"
        );
    }
}
