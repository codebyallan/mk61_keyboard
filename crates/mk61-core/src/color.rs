//! RGB colour representation and strict parsing.

use std::fmt;

use crate::Error;

/// A single RGB colour with channels in the range `0..=255`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Rgb {
    /// Creates a colour from validated channel values.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parses a strict `RRGGBB` hexadecimal colour.
    ///
    /// A single leading `#` is accepted. Anything else — wrong length, missing
    /// digits, or values above `0xFF` — is rejected with [`Error::InvalidColor`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidColor`] when the input is not a valid `RRGGBB`.
    pub fn parse(input: &str) -> std::result::Result<Self, Error> {
        let digits = input.strip_prefix('#').unwrap_or(input);
        if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::InvalidColor);
        }
        let value = u32::from_str_radix(digits, 16).map_err(|_| Error::InvalidColor)?;
        Ok(Self {
            r: ((value >> 16) & 0xFF) as u8,
            g: ((value >> 8) & 0xFF) as u8,
            b: (value & 0xFF) as u8,
        })
    }

    /// Returns the colour channels as `(r, g, b)`.
    #[must_use]
    pub const fn channels(self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }

    /// Returns the colour as packed RGB bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strict_rrggbb_colours() {
        assert_eq!(Rgb::parse("FF0000").unwrap(), Rgb::new(0xFF, 0x00, 0x00));
        assert_eq!(Rgb::parse("00ff00").unwrap(), Rgb::new(0x00, 0xFF, 0x00));
        assert_eq!(Rgb::parse("#1a2B3c").unwrap(), Rgb::new(0x1A, 0x2B, 0x3C));
    }

    #[test]
    fn rejects_malformed_colours() {
        assert!(Rgb::parse("F00").is_err());
        assert!(Rgb::parse("FF00000").is_err());
        assert!(Rgb::parse("GG0000").is_err());
        assert!(Rgb::parse("FF0000FF").is_err());
        assert!(Rgb::parse("").is_err());
    }

    #[test]
    fn channels_and_bytes_are_consistent() {
        let colour = Rgb::new(0x12, 0x34, 0x56);
        assert_eq!(colour.channels(), (0x12, 0x34, 0x56));
        assert_eq!(colour.bytes(), [0x12, 0x34, 0x56]);
        assert_eq!(colour.to_string(), "123456");
    }
}
