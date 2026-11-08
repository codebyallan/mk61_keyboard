//! On-board profile identifiers.
//!
//! The keyboard stores four independent settings profiles. The wire protocol
//! uses the numbers `1..=4`; this type makes it impossible to construct an
//! out-of-range profile by accident.

use std::fmt;

use crate::{Error, Result};

/// One of the four on-board settings profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Profile(u8);

impl Profile {
    /// Lowest valid profile number.
    pub const MIN: u8 = 1;

    /// Highest valid profile number.
    pub const MAX: u8 = 4;

    /// All valid profiles in ascending order.
    pub const ALL: [Self; 4] = [Self(1), Self(2), Self(3), Self(4)];

    /// Creates a profile from its wire value.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidProfile`] when `value` is outside `1..=4`.
    pub const fn new(value: u8) -> Result<Self> {
        if value >= Self::MIN && value <= Self::MAX {
            Ok(Self(value))
        } else {
            Err(Error::InvalidProfile(value))
        }
    }

    /// Returns the wire value (`1..=4`).
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Profile {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Profile> for u8 {
    fn from(profile: Profile) -> Self {
        profile.get()
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_all_valid_profiles() {
        for value in Profile::MIN..=Profile::MAX {
            assert_eq!(Profile::new(value).unwrap().get(), value);
        }
    }

    #[test]
    fn rejects_out_of_range_profiles() {
        assert_eq!(Profile::new(0), Err(Error::InvalidProfile(0)));
        assert_eq!(Profile::new(5), Err(Error::InvalidProfile(5)));
        assert_eq!(Profile::new(255), Err(Error::InvalidProfile(255)));
    }

    #[test]
    fn all_contains_every_profile_in_order() {
        assert_eq!(Profile::ALL.map(Profile::get), [1, 2, 3, 4]);
    }

    #[test]
    fn conversions_round_trip() {
        let profile = Profile::try_from(3).unwrap();
        assert_eq!(u8::from(profile), 3);
        assert_eq!(profile.to_string(), "3");
    }
}
