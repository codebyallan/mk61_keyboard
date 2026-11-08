//! Wire protocol for the MK61 / SK61L vendor configuration interface.
//!
//! The keyboard is controlled through 264-byte feature reports. Byte zero is
//! always the report ID (`0x07`); byte one is the command; byte two is the
//! first parameter; bytes 8..264 carry a 256-byte settings profile. This
//! module keeps the layout constants and pure helpers that build and interpret
//! those reports, so the service layer can stay free of magic numbers.

use crate::Error;

/// Size of a full feature report, in bytes.
pub const REPORT_SIZE: usize = 264;

/// Offset of the first payload byte within a feature report.
pub const PAYLOAD_OFFSET: usize = 8;

/// Size of the profile payload carried by a feature report.
pub const PAYLOAD_SIZE: usize = 256;

/// Report ID of the vendor configuration feature report.
pub const REPORT_ID: u8 = 0x07;

/// Number of keys in a custom layout.
pub const CUSTOM_KEYS: usize = 70;
/// Number of custom slots supported by the firmware.
pub const CUSTOM_SLOTS: usize = 5;
/// Effect value stored in the custom effect plane.
pub const CUSTOM_EFFECT_STATIC: u8 = 2;

// Command bytes (report byte 1).
/// Read the active profile number.
pub const CMD_READ_ACTIVE_PROFILE: u8 = 0x81;
/// Read a profile block.
pub const CMD_READ_PROFILE_BLOCK: u8 = 0x82;
/// Write a profile block (RAM, or flash when committed).
pub const CMD_WRITE_PROFILE_BLOCK: u8 = 0x02;
/// Read/write custom effect colour data.
pub const CMD_CUSTOM_COLORS: u8 = 0x08;
/// Read/write custom effect frame data.
pub const CMD_CUSTOM_FRAME: u8 = 0x09;
/// Read/write custom effect metadata.
pub const CMD_CUSTOM_EFFECT: u8 = 0x0A;

// Profile block offsets (absolute within the 264-byte report).
/// LED effect mode byte.
pub const BLOCK_MODE: usize = 11;
/// Brightness byte.
pub const BLOCK_BRIGHTNESS: usize = 12;
/// Direction byte.
pub const BLOCK_DIRECTION: usize = 13;
/// Speed byte.
pub const BLOCK_SPEED: usize = 19;
/// First spectrum flag byte.
pub const BLOCK_SPECTRUM_START: usize = 32;
/// Last spectrum flag byte (inclusive).
pub const BLOCK_SPECTRUM_END: usize = 40;
/// Custom layout colour count.
pub const BLOCK_CUSTOM_COUNT: usize = 134;
/// Custom layout current index.
pub const BLOCK_CUSTOM_CURRENT: usize = 135;
/// Base offset of the planar colour table.
pub const BLOCK_COLOR_BASE: usize = 92;

/// A 264-byte feature report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    bytes: [u8; REPORT_SIZE],
}

impl Report {
    /// Reconstructs a report from a raw byte buffer.
    ///
    /// # Panics
    ///
    /// Does not panic; buffers shorter than [`REPORT_SIZE`] are not accepted
    /// by the type system (the input is a fixed-size array).
    #[must_use]
    pub fn from_bytes(bytes: [u8; REPORT_SIZE]) -> Self {
        Self { bytes }
    }

    /// Creates an empty report with the given command and first parameter.
    #[must_use]
    pub fn new(command: u8, param0: u8) -> Self {
        let mut bytes = [0_u8; REPORT_SIZE];
        bytes[0] = REPORT_ID;
        bytes[1] = command;
        bytes[2] = param0;
        Self { bytes }
    }

    /// Returns the command byte.
    #[must_use]
    pub const fn command(&self) -> u8 {
        self.bytes[1]
    }

    /// Returns the first parameter byte.
    #[must_use]
    pub const fn param0(&self) -> u8 {
        self.bytes[2]
    }

    /// Returns the full report as a byte slice.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; REPORT_SIZE] {
        &self.bytes
    }

    /// Returns a mutable view of the full report.
    pub fn as_bytes_mut(&mut self) -> &mut [u8; REPORT_SIZE] {
        &mut self.bytes
    }

    /// Copies the profile payload into a fresh array.
    #[must_use]
    pub fn payload_copy(&self) -> [u8; PAYLOAD_SIZE] {
        let mut out = [0_u8; PAYLOAD_SIZE];
        out.copy_from_slice(&self.bytes[PAYLOAD_OFFSET..PAYLOAD_OFFSET + PAYLOAD_SIZE]);
        out
    }

    /// Overwrites the profile payload.
    pub fn set_payload(&mut self, payload: &[u8; PAYLOAD_SIZE]) {
        self.bytes[PAYLOAD_OFFSET..PAYLOAD_OFFSET + PAYLOAD_SIZE].copy_from_slice(payload);
    }
}

/// Reads the active profile number from a `CMD_READ_ACTIVE_PROFILE` response.
///
/// # Errors
///
/// Returns [`Error::InvalidProfile`] when the byte is outside `1..=4`.
pub fn parse_active_profile(report: &Report) -> crate::Result<crate::Profile> {
    crate::Profile::new(report.param0())
}

/// Applies an LED effect to a profile block.
///
/// Sets the mode byte to `effect.index`. Effects without colour slots leave
/// the existing colour table untouched. When switching away from the custom
/// effect, the custom count byte is reset to `255` (clean).
pub fn apply_effect(block: &mut [u8; REPORT_SIZE], effect: &crate::Effect, previous_mode: u8) {
    if previous_mode == crate::CUSTOM_EFFECT_ID && effect.index != crate::CUSTOM_EFFECT_ID {
        block[BLOCK_CUSTOM_COUNT] = 255;
    }
    // A non-custom effect must leave the commit flag clean; an armed (count=1)
    // profile makes the firmware ignore mode changes.
    if effect.index != crate::CUSTOM_EFFECT_ID {
        block[BLOCK_CUSTOM_COUNT] = 255;
    }
    block[BLOCK_MODE] = effect.index;
}

/// Writes a colour into the planar R/G/B table of a profile block.
///
/// `slots` describes the block's colour table. `index` is zero-based within
/// that table. Returns [`Error::InvalidColorCount`] when `index` is out of
/// range.
pub fn write_color(
    block: &mut [u8; REPORT_SIZE],
    slots: crate::ColorSlots,
    index: usize,
    color: crate::Rgb,
) -> Result<(), Error> {
    if index >= slots.count {
        return Err(Error::InvalidColorCount);
    }
    let base = slots.offset;
    let plane = slots.count;
    block[base + index] = color.r;
    block[base + plane + index] = color.g;
    block[base + 2 * plane + index] = color.b;
    Ok(())
}

/// Reads a colour from the planar R/G/B table of a profile block.
#[must_use]
pub fn read_color(
    block: &[u8; REPORT_SIZE],
    slots: crate::ColorSlots,
    index: usize,
) -> Option<crate::Rgb> {
    if index >= slots.count {
        return None;
    }
    let base = slots.offset;
    let plane = slots.count;
    Some(crate::Rgb::new(
        block[base + index],
        block[base + plane + index],
        block[base + 2 * plane + index],
    ))
}

/// Builds the three slot reports (colour, frame, effect) for a custom layout.
///
/// Returns reports for opcodes 0x08, 0x09 and 0x0A respectively, for slot 0.
#[must_use]
pub fn build_custom_slot_reports(
    profile: u8,
    rgb: &[[u8; 3]; CUSTOM_KEYS],
) -> [[u8; REPORT_SIZE]; 3] {
    const OP_CODES: [u8; 3] = [0x08, 0x09, 0x0A];
    let mut bufs = [[0_u8; 256]; 3];
    for k in 0..CUSTOM_KEYS {
        bufs[0][k] = rgb[k][0];
        bufs[0][CUSTOM_KEYS + k] = rgb[k][1];
        bufs[0][2 * CUSTOM_KEYS + k] = rgb[k][2];
        bufs[1][1 + k / 8] |= 1u8 << (k % 8);
        bufs[2][k] = CUSTOM_EFFECT_STATIC;
    }
    bufs[1][0] = 1; // one frame

    let mut out = [[0_u8; REPORT_SIZE]; 3];
    for i in 0..3 {
        let mut report = [0_u8; REPORT_SIZE];
        report[0] = REPORT_ID;
        report[1] = OP_CODES[i];
        report[2] = profile;
        report[3] = 0; // slot 0
        // params bytes 4..7 = 0
        report[8..8 + 256].copy_from_slice(&bufs[i]);
        out[i] = report;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Effect, Rgb, effect_by_index, effect_color_slots};

    #[test]
    fn report_carries_command_and_param() {
        let report = Report::new(CMD_READ_PROFILE_BLOCK, 3);
        assert_eq!(report.command(), CMD_READ_PROFILE_BLOCK);
        assert_eq!(report.param0(), 3);
        assert_eq!(report.as_bytes()[0], REPORT_ID);
    }

    #[test]
    fn payload_round_trips() {
        let mut report = Report::new(CMD_WRITE_PROFILE_BLOCK, 1);
        let mut payload = [0_u8; PAYLOAD_SIZE];
        payload[0] = 0xAB;
        payload[PAYLOAD_SIZE - 1] = 0xCD;
        report.set_payload(&payload);
        assert_eq!(report.payload_copy(), payload);
    }

    #[test]
    fn parse_active_profile_validates_range() {
        let report = Report::new(CMD_READ_ACTIVE_PROFILE, 2);
        let _ = report;
        let report = Report::new(CMD_READ_ACTIVE_PROFILE, 2);
        assert_eq!(parse_active_profile(&report).unwrap().get(), 2);

        let bad = Report::new(CMD_READ_ACTIVE_PROFILE, 9);
        assert!(parse_active_profile(&bad).is_err());
    }

    #[test]
    fn apply_effect_sets_mode_byte() {
        let mut block = [0_u8; REPORT_SIZE];
        let effect = effect_by_index(5).unwrap();
        apply_effect(&mut block, effect, 0);
        assert_eq!(block[BLOCK_MODE], 5);
    }

    #[test]
    fn apply_effect_resets_custom_count_when_leaving_custom() {
        let mut block = [0_u8; REPORT_SIZE];
        block[BLOCK_CUSTOM_COUNT] = 7;
        block[BLOCK_MODE] = 15;
        let effect = effect_by_index(0).unwrap();
        apply_effect(&mut block, effect, 15);
        assert_eq!(block[BLOCK_MODE], 0);
        assert_eq!(block[BLOCK_CUSTOM_COUNT], 255);
    }

    #[test]
    fn write_and_read_color_round_trip() {
        let mut block = [0_u8; REPORT_SIZE];
        let slots = effect_color_slots(&Effect {
            index: 0,
            name: "wave",
            description: "",
            colors: Some(crate::ColorSlots {
                offset: 92,
                count: 14,
            }),
        })
        .unwrap();
        write_color(&mut block, slots, 0, Rgb::new(0x12, 0x34, 0x56)).unwrap();
        write_color(&mut block, slots, 13, Rgb::new(0xAB, 0xCD, 0xEF)).unwrap();
        assert_eq!(
            read_color(&block, slots, 0),
            Some(Rgb::new(0x12, 0x34, 0x56))
        );
        assert_eq!(
            read_color(&block, slots, 13),
            Some(Rgb::new(0xAB, 0xCD, 0xEF))
        );
        assert!(write_color(&mut block, slots, 14, Rgb::new(0, 0, 0)).is_err());
    }
}
