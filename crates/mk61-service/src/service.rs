//! Service configuration and composition root.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use crate::ServiceError;
use mk61_core::{
    BLOCK_CUSTOM_COUNT, BLOCK_CUSTOM_CURRENT, BLOCK_MODE, BLOCK_SPECTRUM_END, BLOCK_SPECTRUM_START,
    CMD_READ_ACTIVE_PROFILE, CMD_READ_PROFILE_BLOCK, CMD_WRITE_PROFILE_BLOCK, Effect, Profile,
    Report, Rgb, apply_effect, effect_color_slots, write_color,
};
use mk61_device::{DEFAULT_SYSFS_ROOT, Transport};

/// Minimum time between two flash commits.
pub const DEFAULT_FLASH_COOLDOWN: Duration = Duration::from_secs(30);

/// Number of read attempts before a response is considered unusable.
pub const DEFAULT_READ_RETRIES: u32 = 3;

/// Configuration shared by every service operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    /// Explicit hidraw path, or `None` to auto-detect a device.
    pub device_path: Option<PathBuf>,
    /// Linux sysfs mount used by device discovery.
    pub sysfs_root: PathBuf,
    /// Directory holding backups and the flash guard state.
    pub state_dir: PathBuf,
    /// Minimum time between two successful flash commits.
    pub flash_cooldown: Duration,
    /// Number of attempts when reading a report back from the device.
    pub read_retries: u32,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            device_path: None,
            sysfs_root: PathBuf::from(DEFAULT_SYSFS_ROOT),
            state_dir: default_state_dir(),
            flash_cooldown: DEFAULT_FLASH_COOLDOWN,
            read_retries: DEFAULT_READ_RETRIES,
        }
    }
}

/// Returns the default state directory for backups and flash guard state.
///
/// Follows the XDG base directory specification:
/// `$XDG_STATE_HOME/mk61`, falling back to `$HOME/.local/state/mk61`. Relative
/// or empty values are ignored, as required by the specification. If neither
/// variable yields an absolute path (for example in a minimal container), a
/// per-system temporary directory is used as a last resort.
pub fn default_state_dir() -> PathBuf {
    state_dir_from(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

fn state_dir_from(xdg_state_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
    if let Some(directory) = non_empty_path(xdg_state_home) {
        return directory.join("mk61");
    }
    if let Some(directory) = non_empty_path(home) {
        return directory.join(".local").join("state").join("mk61");
    }
    std::env::temp_dir().join("mk61")
}

fn non_empty_path(value: Option<OsString>) -> Option<PathBuf> {
    value
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Coordinates protocol operations with device transport and safety state.
///
/// The type is intentionally thin in this milestone: it holds the configuration
/// and transport that later use-cases will consume. Constructing a service does
/// not touch the device.
#[derive(Debug)]
pub struct Service<T> {
    config: ServiceConfig,
    transport: T,
}

impl<T: Transport> Service<T> {
    /// Creates a service from configuration and an already open transport.
    pub fn new(config: ServiceConfig, transport: T) -> Self {
        Self { config, transport }
    }

    /// Returns the configuration used by this service.
    pub fn config(&self) -> &ServiceConfig {
        &self.config
    }

    /// Returns the transport owned by this service.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Returns mutable access to the transport owned by this service.
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Reads the active profile number from the device.
    pub fn read_active_profile(
        &mut self,
        clock: &dyn crate::Clock,
    ) -> Result<Profile, ServiceError> {
        let mut report = Report::new(CMD_READ_ACTIVE_PROFILE, 0);
        self.transport.write_feature(report.as_bytes())?;
        clock.sleep(std::time::Duration::from_millis(100));
        let mut buffer = [0_u8; mk61_core::REPORT_SIZE];
        let transferred = self.transport.read_feature(&mut buffer)?;
        // The device prepends a zero byte to some responses ("shifted" report).
        if buffer[0] == 0 && buffer[1] == mk61_core::REPORT_ID {
            for i in 0..(mk61_core::REPORT_SIZE - 1) {
                buffer[i] = buffer[i + 1];
            }
            buffer[mk61_core::REPORT_SIZE - 1] = 0;
        }
        if transferred != mk61_core::REPORT_SIZE
            || buffer[0] != mk61_core::REPORT_ID
            || buffer[1] != CMD_READ_ACTIVE_PROFILE
            || buffer[2] < 1
            || buffer[2] > 4
        {
            return Err(ServiceError::Device(
                mk61_device::DeviceError::MalformedReport("active profile response mismatch"),
            ));
        }
        report = Report::from_bytes(buffer);
        Ok(Profile::new(report.param0())?)
    }

    /// Reads a 256-byte profile block from the device.
    pub fn read_profile_block(
        &mut self,
        clock: &dyn crate::Clock,
        profile: Profile,
    ) -> Result<[u8; mk61_core::REPORT_SIZE], ServiceError> {
        let mut report = Report::new(CMD_READ_PROFILE_BLOCK, profile.get());
        self.transport.write_feature(report.as_bytes())?;
        clock.sleep(std::time::Duration::from_millis(50));
        let mut buffer = [0_u8; mk61_core::REPORT_SIZE];
        let transferred = self.transport.read_feature(&mut buffer)?;
        // The device prepends a zero byte to some responses ("shifted" report).
        if buffer[0] == 0 && buffer[1] == mk61_core::REPORT_ID {
            for i in 0..(mk61_core::REPORT_SIZE - 1) {
                buffer[i] = buffer[i + 1];
            }
            buffer[mk61_core::REPORT_SIZE - 1] = 0;
        }
        if transferred != mk61_core::REPORT_SIZE
            || buffer[0] != mk61_core::REPORT_ID
            || buffer[1] != CMD_READ_PROFILE_BLOCK
            || buffer[2] != profile.get()
        {
            return Err(ServiceError::Device(
                mk61_device::DeviceError::MalformedReport("profile block response mismatch"),
            ));
        }
        report = Report::from_bytes(buffer);
        let _ = report;
        Ok(buffer)
    }

    /// Writes a profile block to device RAM and verifies read-back.
    pub fn write_profile_block_ram(
        &mut self,
        clock: &dyn crate::Clock,
        profile: Profile,
        block: &[u8; mk61_core::REPORT_SIZE],
    ) -> Result<(), ServiceError> {
        // Convert the read block (opcode 0x82) into a write report (opcode 0x02),
        // preserving every other byte, including params and fields.
        let mut report = *block;
        report[1] = CMD_WRITE_PROFILE_BLOCK;
        report[2] = profile.get();
        self.transport.write_feature(&report)?;
        clock.sleep(std::time::Duration::from_millis(300));

        // Read back and compare.
        let read_back = self.read_profile_block(clock, profile)?;
        // Compare from byte 3 onward, ignoring report id / opcode / profile
        // (the device echoes the read opcode 0x82, not our write opcode 0x02).
        if read_back[3..] != block[3..] {
            clock.sleep(std::time::Duration::from_millis(500));
            let read_back2 = self.read_profile_block(clock, profile)?;
            if read_back2[3..] != block[3..] {
                let mut diff = None;
                for (i, (a, b)) in read_back2.iter().zip(block.iter()).enumerate().skip(3) {
                    if a != b {
                        diff = Some((i, *a, *b));
                        break;
                    }
                }
                let _ = diff;
                let _ = diff;
                return Err(ServiceError::Device(
                    mk61_device::DeviceError::MalformedReport("profile block read-back mismatch"),
                ));
            }
        }
        Ok(())
    }

    /// Applies an LED effect, optionally committing to flash.
    pub fn apply_led_effect(
        &mut self,
        clock: &dyn crate::Clock,
        backup: &crate::BackupStore,
        guard: &mut crate::FlashGuard,
        spec: &str,
        save: bool,
    ) -> Result<ApplyLedEffectResult, ServiceError> {
        let effect = mk61_core::resolve_effect(spec).map_err(ServiceError::Core)?;
        let profile = self.read_active_profile(clock)?;
        let original_block = self.read_profile_block(clock, profile)?;
        let previous_mode = original_block[BLOCK_MODE];

        let mut block = original_block;
        apply_effect(&mut block, effect, previous_mode);
        // RAM apply + verify.
        self.write_profile_block_ram(clock, profile, &block)?;

        let mut saved = false;
        if save {
            guard.check(clock.now())?;
            // Backup original block before flash.
            backup.save("profile", &original_block)?;
            // Flash commit: write twice, count byte 1 then 255, verify each.
            let mut commit_block = block;
            commit_block[BLOCK_CUSTOM_COUNT] = 1;
            self.write_profile_block_ram(clock, profile, &commit_block)?;
            commit_block[BLOCK_CUSTOM_COUNT] = 255;
            self.write_profile_block_ram(clock, profile, &commit_block)?;
            guard.record(clock.now())?;
            saved = true;
        }

        Ok(ApplyLedEffectResult {
            profile,
            effect_name: effect.name.to_string(),
            saved,
        })
    }

    /// Sets palette colours for a built-in effect.
    pub fn apply_led_color(
        &mut self,
        clock: &dyn crate::Clock,
        backup: &crate::BackupStore,
        guard: &mut crate::FlashGuard,
        effect_spec: &str,
        colors: &[Rgb],
        save: bool,
    ) -> Result<ApplyLedColorResult, ServiceError> {
        if colors.is_empty() {
            return Err(ServiceError::Core(mk61_core::Error::InvalidColorCount));
        }
        let effect = mk61_core::resolve_effect(effect_spec).map_err(ServiceError::Core)?;
        let slots = effect_color_slots(effect).map_err(ServiceError::Core)?;
        let profile = self.read_active_profile(clock)?;
        let original_block = self.read_profile_block(clock, profile)?;
        let previous_mode = original_block[BLOCK_MODE];

        let mut block = original_block;
        // Repeat the supplied colours across the effect's slots.
        for index in 0..slots.count {
            let color = colors[index % colors.len()];
            write_color(&mut block, slots, index, color).map_err(ServiceError::Core)?;
        }
        // Keep the commit flag clean for non-custom effects.
        if effect.index != mk61_core::CUSTOM_EFFECT_ID {
            block[BLOCK_CUSTOM_COUNT] = 255;
        }
        let _ = previous_mode;

        // RAM apply + verify.
        self.write_profile_block_ram(clock, profile, &block)?;

        let mut saved = false;
        if save {
            guard.check(clock.now())?;
            backup.save("color", &original_block)?;
            let mut commit_block = block;
            commit_block[BLOCK_CUSTOM_COUNT] = 1;
            self.write_profile_block_ram(clock, profile, &commit_block)?;
            commit_block[BLOCK_CUSTOM_COUNT] = 255;
            self.write_profile_block_ram(clock, profile, &commit_block)?;
            guard.record(clock.now())?;
            saved = true;
        }

        Ok(ApplyLedColorResult {
            profile,
            effect_name: effect.name.to_string(),
            color_count: slots.count,
            saved,
        })
    }

    /// Sets the brightness (0..=10) of the active profile.
    pub fn apply_led_brightness(
        &mut self,
        clock: &dyn crate::Clock,
        backup: &crate::BackupStore,
        guard: &mut crate::FlashGuard,
        value: u8,
        save: bool,
    ) -> Result<ApplyLedFieldResult, ServiceError> {
        if value > 10 {
            return Err(ServiceError::Core(mk61_core::Error::InvalidColorCount));
        }
        self.apply_led_field(clock, backup, guard, LedField::Brightness, value, save)
    }

    /// Sets the speed (2..=102, lower = faster) of the active profile.
    pub fn apply_led_speed(
        &mut self,
        clock: &dyn crate::Clock,
        backup: &crate::BackupStore,
        guard: &mut crate::FlashGuard,
        value: u8,
        save: bool,
    ) -> Result<ApplyLedFieldResult, ServiceError> {
        if !(2..=102).contains(&value) {
            return Err(ServiceError::Core(mk61_core::Error::InvalidColorCount));
        }
        self.apply_led_field(clock, backup, guard, LedField::Speed, value, save)
    }

    /// Sets the direction (0..=3) of the active profile.
    pub fn apply_led_direction(
        &mut self,
        clock: &dyn crate::Clock,
        backup: &crate::BackupStore,
        guard: &mut crate::FlashGuard,
        value: u8,
        save: bool,
    ) -> Result<ApplyLedFieldResult, ServiceError> {
        if value > 3 {
            return Err(ServiceError::Core(mk61_core::Error::InvalidColorCount));
        }
        self.apply_led_field(clock, backup, guard, LedField::Direction, value, save)
    }

    /// Generic field application: read, modify one byte, write, verify, optionally commit.
    fn apply_led_field(
        &mut self,
        clock: &dyn crate::Clock,
        backup: &crate::BackupStore,
        guard: &mut crate::FlashGuard,
        field: LedField,
        value: u8,
        save: bool,
    ) -> Result<ApplyLedFieldResult, ServiceError> {
        let profile = self.read_active_profile(clock)?;
        let original_block = self.read_profile_block(clock, profile)?;

        let mut block = original_block;
        let offset = field.offset();
        block[offset] = value;
        if field != LedField::Brightness {
            // keep commit flag clean
        }
        block[BLOCK_CUSTOM_COUNT] = 255;

        self.write_profile_block_ram(clock, profile, &block)?;

        let mut saved = false;
        if save {
            guard.check(clock.now())?;
            backup.save("field", &original_block)?;
            let mut commit_block = block;
            commit_block[BLOCK_CUSTOM_COUNT] = 1;
            self.write_profile_block_ram(clock, profile, &commit_block)?;
            commit_block[BLOCK_CUSTOM_COUNT] = 255;
            self.write_profile_block_ram(clock, profile, &commit_block)?;
            guard.record(clock.now())?;
            saved = true;
        }

        Ok(ApplyLedFieldResult {
            profile,
            field_name: field.name().to_string(),
            value,
            saved,
        })
    }

    /// Commits the active profile to flash (standalone save).
    pub fn save_profile(
        &mut self,
        clock: &dyn crate::Clock,
        backup: &crate::BackupStore,
        guard: &mut crate::FlashGuard,
    ) -> Result<SaveProfileResult, ServiceError> {
        guard.check(clock.now())?;
        let profile = self.read_active_profile(clock)?;
        let cur = self.read_profile_block(clock, profile)?;
        // Arm: count 255 -> 1.
        let mut arm = cur;
        arm[BLOCK_CUSTOM_COUNT] = 1;
        arm[BLOCK_CUSTOM_CURRENT] = 0;
        self.write_profile_block_ram(clock, profile, &arm)?;
        clock.sleep(std::time::Duration::from_millis(200));
        // Clean: restores original (count 255).
        let clean = cur;
        self.write_profile_block_ram(clock, profile, &clean)?;
        guard.record(clock.now())?;
        backup.save("profile", &cur)?;
        Ok(SaveProfileResult { profile })
    }

    /// Applies a custom per-key layout (or clears it).
    pub fn apply_led_custom(
        &mut self,
        clock: &dyn crate::Clock,
        backup: &crate::BackupStore,
        _guard: &mut crate::FlashGuard,
        base: Option<mk61_core::Rgb>,
        assignments: &[(usize, mk61_core::Rgb)],
        clear: bool,
    ) -> Result<ApplyLedCustomResult, ServiceError> {
        let profile = self.read_active_profile(clock)?;
        let original_block = self.read_profile_block(clock, profile)?;

        if clear {
            // Clear: mode 15 -> 0 (if custom), count -> 255, commit.
            let mut block = original_block;
            if block[BLOCK_MODE] == mk61_core::CUSTOM_EFFECT_ID {
                block[BLOCK_MODE] = 0;
            }
            block[BLOCK_CUSTOM_COUNT] = 255;
            self.write_profile_block_ram(clock, profile, &block)?;
            backup.save("custom-clear", &original_block)?;
            return Ok(ApplyLedCustomResult {
                profile,
                cleared: true,
            });
        }

        // Build the per-key colour table (70 keys).
        let mut rgb = [[0_u8; 3]; mk61_core::CUSTOM_KEYS];
        if let Some(base_color) = base {
            for rgb_entry in rgb.iter_mut().take(mk61_core::CUSTOM_KEYS) {
                *rgb_entry = base_color.bytes();
            }
        }
        for (pos, color) in assignments {
            if *pos < mk61_core::CUSTOM_KEYS {
                rgb[*pos] = color.bytes();
            }
        }

        // Send the three slot reports (colour, frame, effect).
        let slots = mk61_core::build_custom_slot_reports(profile.get(), &rgb);
        for (i, slot_report) in slots.iter().enumerate() {
            self.transport.write_feature(slot_report)?;
            let delay = if i == 2 { 600 } else { 300 };
            clock.sleep(std::time::Duration::from_millis(delay));
        }

        // Activate custom mode: mode 15, count 1 (with arm transition if needed).
        let before_count = original_block[BLOCK_CUSTOM_COUNT];
        if before_count == 1 {
            // Arm first (count 255) to create a transition.
            let mut arm = original_block;
            arm[BLOCK_CUSTOM_COUNT] = 255;
            arm[BLOCK_MODE] = mk61_core::CUSTOM_EFFECT_ID;
            arm[BLOCK_CUSTOM_CURRENT] = 0;
            self.write_profile_block_ram(clock, profile, &arm)?;
        }
        let mut rep = original_block;
        // Convert read block (0x82) to write (0x02).
        rep[1] = CMD_WRITE_PROFILE_BLOCK;
        rep[2] = profile.get();
        rep[BLOCK_MODE] = mk61_core::CUSTOM_EFFECT_ID;
        rep[BLOCK_CUSTOM_COUNT] = 1;
        rep[BLOCK_CUSTOM_CURRENT] = 0;
        // Clear spectrum flags.
        #[allow(clippy::needless_range_loop, clippy::manual_range_contains)]
        for i in BLOCK_SPECTRUM_START..=BLOCK_SPECTRUM_END {
            rep[i] = 0;
        }
        self.write_profile_block_ram(clock, profile, &rep)?;

        backup.save("custom", &original_block)?;
        Ok(ApplyLedCustomResult {
            profile,
            cleared: false,
        })
    }

    /// Clears a custom per-key layout: mode 15 -> 0, count -> 255.
    pub fn clear_custom_effect(
        &mut self,
        clock: &dyn crate::Clock,
        profile: Profile,
    ) -> Result<(), ServiceError> {
        let mut block = self.read_profile_block(clock, profile)?;
        if block[BLOCK_MODE] == mk61_core::CUSTOM_EFFECT_ID {
            block[BLOCK_MODE] = 0;
        }
        block[BLOCK_CUSTOM_COUNT] = 255;
        self.write_profile_block_ram(clock, profile, &block)
    }
}

/// Result of applying an LED effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyLedEffectResult {
    /// Profile the effect was applied to.
    pub profile: Profile,
    /// Name of the applied effect.
    pub effect_name: String,
    /// Whether the change was committed to flash.
    pub saved: bool,
}

/// A profile field that can be changed in RAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedField {
    /// Brightness (0..=10).
    Brightness,
    /// Speed (2..=102, lower = faster).
    Speed,
    /// Direction (0..=3).
    Direction,
}

impl LedField {
    /// Absolute byte offset of the field within the profile report.
    #[must_use]
    pub const fn offset(self) -> usize {
        match self {
            LedField::Brightness => 12,
            LedField::Speed => 19,
            LedField::Direction => 13,
        }
    }

    /// Human-readable field name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            LedField::Brightness => "brightness",
            LedField::Speed => "speed",
            LedField::Direction => "direction",
        }
    }
}

/// Result of setting palette colours for an LED effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyLedColorResult {
    /// Profile the colours were applied to.
    pub profile: Profile,
    /// Name of the effect whose colours were changed.
    pub effect_name: String,
    /// Number of colour slots written.
    pub color_count: usize,
    /// Whether the change was committed to flash.
    pub saved: bool,
}

/// Result of changing a single LED field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyLedFieldResult {
    /// Profile the field was applied to.
    pub profile: Profile,
    /// Name of the changed field.
    pub field_name: String,
    /// Value written.
    pub value: u8,
    /// Whether the change was committed to flash.
    pub saved: bool,
}

/// Result of committing the active profile to flash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveProfileResult {
    /// Profile that was committed.
    pub profile: Profile,
}

/// Result of applying a custom per-key layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyLedCustomResult {
    /// Profile the layout was applied to.
    pub profile: Profile,
    /// Whether the custom layout was cleared.
    pub cleared: bool,
}

/// Returns every LED effect the keyboard firmware supports.
///
/// The built-in effects come first (indices `0..=12`) and the custom per-key
/// layout effect (`15`) is appended last.
#[must_use]
pub fn led_effects() -> Vec<Effect> {
    let mut effects: Vec<Effect> = mk61_core::EFFECTS.to_vec();
    effects.push(*mk61_core::custom_effect());
    effects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackupStore, FlashGuard, SystemClock};
    use mk61_device::MockTransport;

    #[test]
    fn default_config_uses_the_verified_safety_values() {
        let config = ServiceConfig::default();

        assert_eq!(config.flash_cooldown, Duration::from_secs(30));
        assert_eq!(config.read_retries, 3);
        assert_eq!(config.state_dir.file_name().unwrap(), "mk61");
    }

    #[test]
    fn xdg_state_home_takes_precedence() {
        let directory = state_dir_from(
            Some(OsString::from("/var/lib/user-state")),
            Some(OsString::from("/home/user")),
        );

        assert_eq!(directory, PathBuf::from("/var/lib/user-state/mk61"));
    }

    #[test]
    fn home_is_the_fallback() {
        let directory = state_dir_from(None, Some(OsString::from("/home/user")));

        assert_eq!(directory, PathBuf::from("/home/user/.local/state/mk61"));
    }

    #[test]
    fn empty_variables_are_ignored() {
        let directory = state_dir_from(Some(OsString::new()), Some(OsString::from("/home/user")));

        assert_eq!(directory, PathBuf::from("/home/user/.local/state/mk61"));
    }

    #[test]
    fn relative_state_variables_are_ignored() {
        let directory = state_dir_from(
            Some(OsString::from("relative-state")),
            Some(OsString::from("/home/user")),
        );

        assert_eq!(directory, PathBuf::from("/home/user/.local/state/mk61"));
    }

    #[test]
    fn led_effects_contains_the_builtins_and_the_custom_effect() {
        let effects = led_effects();

        assert_eq!(effects.len(), 16);
        assert_eq!(effects[0].index, 0);
        assert_eq!(effects[0].name, "wave");
        assert_eq!(effects[12].index, 12);
        assert_eq!(effects[12].name, "lighting");
        assert_eq!(effects[15].index, 15);
        assert_eq!(effects[15].name, "custom");
    }

    #[test]
    fn service_keeps_configuration_and_transport_together() {
        let config = ServiceConfig::default();
        let mut service = Service::new(config.clone(), MockTransport::new());

        assert_eq!(service.config(), &config);
        service
            .transport_mut()
            .write_feature(&[0x07])
            .expect("mock writes cannot fail");
        assert_eq!(service.transport().writes(), &[vec![0x07]]);
    }

    fn report_bytes(command: u8, param0: u8, payload: &[u8]) -> Vec<u8> {
        let mut buf = vec![0_u8; mk61_core::REPORT_SIZE];
        buf[0] = mk61_core::REPORT_ID;
        buf[1] = command;
        buf[2] = param0;
        if !payload.is_empty() {
            buf[mk61_core::PAYLOAD_OFFSET..mk61_core::PAYLOAD_OFFSET + payload.len()]
                .copy_from_slice(payload);
        }
        buf
    }

    #[test]
    fn apply_led_effect_updates_ram_without_saving() {
        let config = ServiceConfig::default();
        let mut transport = MockTransport::new();
        // read active profile -> profile 1
        transport.push_response(report_bytes(CMD_READ_ACTIVE_PROFILE, 1, &[]));
        // read profile block (original)
        transport.push_response(report_bytes(
            CMD_READ_PROFILE_BLOCK,
            1,
            &[0_u8; mk61_core::PAYLOAD_SIZE],
        ));
        // read-back after RAM write (count byte now clean = 255)
        let mut read_back_payload = [0_u8; mk61_core::PAYLOAD_SIZE];
        read_back_payload[mk61_core::BLOCK_CUSTOM_COUNT - mk61_core::PAYLOAD_OFFSET] = 255;
        transport.push_response(report_bytes(CMD_READ_PROFILE_BLOCK, 1, &read_back_payload));

        let mut service = Service::new(config, transport);
        let backup_dir = std::env::temp_dir().join("mk61-test-backup-ram");
        let _ = std::fs::remove_dir_all(&backup_dir);
        let backup = BackupStore::new(&backup_dir);
        let mut guard = FlashGuard::new(backup_dir.join("flash.state"), DEFAULT_FLASH_COOLDOWN);

        let result = service
            .apply_led_effect(&SystemClock, &backup, &mut guard, "wave", false)
            .expect("apply should succeed");

        assert_eq!(result.profile.get(), 1);
        assert_eq!(result.effect_name, "wave");
        assert!(!result.saved);

        // Mode byte should have been set to 0 (wave) in the written block.
        let writes = service.transport().writes();
        // writes: 0x81, 0x82, 0x83, 0x82
        assert_eq!(writes.len(), 4);
        assert_eq!(writes[0][1], CMD_READ_ACTIVE_PROFILE);
        assert_eq!(writes[1][1], CMD_READ_PROFILE_BLOCK);
        assert_eq!(writes[2][1], CMD_WRITE_PROFILE_BLOCK);
        assert_eq!(writes[2][mk61_core::BLOCK_MODE], 0);
        assert_eq!(writes[3][1], CMD_READ_PROFILE_BLOCK);

        let _ = std::fs::remove_dir_all(&backup_dir);
    }

    #[test]
    fn apply_led_effect_rejects_unknown_effect() {
        let config = ServiceConfig::default();
        let mut service = Service::new(config, MockTransport::new());
        let backup_dir = std::env::temp_dir().join("mk61-test-backup-unknown");
        let backup = BackupStore::new(&backup_dir);
        let mut guard = FlashGuard::new(backup_dir.join("flash.state"), DEFAULT_FLASH_COOLDOWN);

        let err = service
            .apply_led_effect(&SystemClock, &backup, &mut guard, "nope", false)
            .unwrap_err();
        assert!(matches!(
            err,
            ServiceError::Core(mk61_core::Error::UnknownEffect)
        ));
    }

    #[test]
    fn apply_led_effect_saves_with_backup_and_cooldown() {
        let config = ServiceConfig::default();
        let mut transport = MockTransport::new();
        // read active profile
        transport.push_response(report_bytes(CMD_READ_ACTIVE_PROFILE, 1, &[]));
        // read original block
        transport.push_response(report_bytes(
            CMD_READ_PROFILE_BLOCK,
            1,
            &[0_u8; mk61_core::PAYLOAD_SIZE],
        ));
        // RAM write read-back (count byte now clean = 255)
        let mut ram_read_back = [0_u8; mk61_core::PAYLOAD_SIZE];
        ram_read_back[mk61_core::BLOCK_CUSTOM_COUNT - mk61_core::PAYLOAD_OFFSET] = 255;
        transport.push_response(report_bytes(CMD_READ_PROFILE_BLOCK, 1, &ram_read_back));
        // flash commit pass 1 read-back (count byte = 1)
        let mut pass1 = [0_u8; mk61_core::REPORT_SIZE];
        pass1[0] = mk61_core::REPORT_ID;
        pass1[1] = CMD_READ_PROFILE_BLOCK;
        pass1[2] = 1;
        pass1[BLOCK_CUSTOM_COUNT] = 1;
        transport.push_response(pass1.to_vec());
        // flash commit pass 2 read-back (count byte = 255)
        let mut pass2 = [0_u8; mk61_core::REPORT_SIZE];
        pass2[0] = mk61_core::REPORT_ID;
        pass2[1] = CMD_READ_PROFILE_BLOCK;
        pass2[2] = 1;
        pass2[BLOCK_CUSTOM_COUNT] = 255;
        transport.push_response(pass2.to_vec());

        let mut service = Service::new(config, transport);
        let backup_dir = std::env::temp_dir().join("mk61-test-backup-save");
        let _ = std::fs::remove_dir_all(&backup_dir);
        let backup = BackupStore::new(&backup_dir);
        let mut guard = FlashGuard::new(backup_dir.join("flash.state"), DEFAULT_FLASH_COOLDOWN);

        let result = service
            .apply_led_effect(&SystemClock, &backup, &mut guard, "wave", true)
            .expect("apply+save should succeed");

        assert!(result.saved);
        // A backup file should have been written.
        let entries: Vec<_> = std::fs::read_dir(backup.directory()).unwrap().collect();
        assert!(!entries.is_empty());

        let _ = std::fs::remove_dir_all(&backup_dir);
    }
}
