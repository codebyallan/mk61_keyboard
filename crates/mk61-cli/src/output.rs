//! Human-readable output rendered by the CLI.

use std::io::{self, Write};

use mk61_service::{DeviceInfo, Effect, StatusReport, effect_color_slots};

/// Command overview printed by `mk61 help`.
pub const OVERVIEW: &str = "\
mk61 - Linux controller for the Gamakay MK61 / SK61L keyboard

Usage:
  mk61 [GLOBAL OPTIONS] <COMMAND>

Global options:
  --device <PATH>    Use a specific hidraw node instead of auto-detection
  --state-dir <DIR>  Directory for backups and flash guard state
  --verbose          Print diagnostic details to stderr

Commands:
  help               Show this overview
  list               List detected keyboards
  status             Show device and capability status
  led effects        List built-in and custom LED effects
  led effect <name|index> [--save]
                     Select an LED effect (RAM only unless --save)
  led save           Commit the active profile to flash with verification
  led color <effect> <RRGGBB>... [--save]
                     Set palette colours for a built-in effect
  led brightness <0..10> [--save]
                     Set the LED brightness
  led speed <2..102> [--save]
                     Set the LED speed (lower = faster)
  led direction <0..3> [--save]
                     Set the LED animation direction
  led custom [--base RRGGBB] <keys>=<RRGGBB>...
                     Define a per-key layout (writes flash)
  led custom clear   Remove the stored layout (writes flash)
  keys               List key names and matrix positions

Flash writes:
  Only `--save`, `led save`, `led custom` and `led custom clear` write the
  keyboard's flash memory. The custom commands write flash without an extra
  flag because the custom slots are write-only; every write prints `backup:`
  and `saved:` only after verification.

Exit status:
  0  success
  1  runtime error
  2  usage error

Current milestone: `help`, `list`, `status`, and `led` subcommands are implemented.
`list` and `status` are read-only; LED commands talk to the keyboard. Run
`mk61 --help` for the generated CLI help.
";

/// Compact command list appended to the generated `--help` output.
///
/// Clap reserves the name `help` for its generated subcommand and hides an
/// explicit subcommand with that name from the `Commands:` section. This footer
/// keeps the generated help complete and points to `mk61 help` for details.
pub const HELP_FOOTER: &str = "\
Commands:
  help               Show the full command overview
  list               List detected keyboards
  status             Show active profile and LED settings
  led effects        List built-in and custom LED effects
  led effect         Select an LED effect (RAM; --save commits)
  led save           Commit the active profile to flash
  led color          Set palette colours for a built-in effect
  led custom         Define or clear a per-key layout (writes flash)
  keys               List key names and matrix positions

Implemented in this milestone: `help`, `list`, `status`, and `led` subcommands.";

/// Writes the command overview to `writer`.
///
/// # Errors
///
/// Returns any I/O error produced while writing or flushing the stream.
pub fn print_overview(writer: &mut impl Write) -> io::Result<()> {
    writer.write_all(OVERVIEW.as_bytes())?;
    writer.flush()
}

/// Writes discovered devices to `writer`.
///
/// A single header row is followed by one row per device. When `devices` is
/// empty, a clear message is printed instead and the caller still treats the
/// command as successful. Product strings are already sanitised by the device
/// layer.
///
/// # Errors
///
/// Returns any I/O error produced while writing or flushing the stream.
pub fn print_devices(writer: &mut impl Write, devices: &[DeviceInfo]) -> io::Result<()> {
    if devices.is_empty() {
        writeln!(writer, "no supported MK61 devices found")?;
        return writer.flush();
    }

    writeln!(
        writer,
        "{:<16} {:<9} {:<6} PRODUCT",
        "DEVICE", "VID:PID", "CONFIG"
    )?;
    for device in devices {
        writeln!(
            writer,
            "{:<16} {:04X}:{:04X} {:<6} {}",
            device.path.display(),
            device.vendor_id,
            device.product_id,
            if device.is_config_interface {
                "yes"
            } else {
                "no"
            },
            device.product.as_deref().unwrap_or("(unknown)")
        )?;
    }

    writer.flush()
}

/// Writes the LED effect catalogue to `writer`.
///
/// The built-in effects come first and the custom per-key effect last. Effects
/// without colour slots are listed with `none`.
///
/// # Errors
///
/// Returns any I/O error produced while writing or flushing the stream.
pub fn print_effects(writer: &mut impl Write, effects: &[Effect]) -> io::Result<()> {
    writeln!(writer, "index  effect         colors  description")?;
    for effect in effects {
        let colors = match effect_color_slots(effect) {
            Ok(slots) => format!("1..{}", slots.count),
            Err(_) => "none".to_owned(),
        };
        writeln!(
            writer,
            "  {:>2}  {:<12} {:<7} {}",
            effect.index, effect.name, colors, effect.description
        )?;
    }
    writer.flush()
}

/// Writes the current status to `writer`.
///
/// The report comes from the service layer and only carries read-only
/// information: whether a daemon is supervising, which devices were found, and
/// which capability switches the host granted. Hardware state such as the
/// active profile or the selected effect is filled in by later milestones.
///
/// # Errors
///
/// Returns any I/O error produced while writing or flushing the stream.
pub fn print_status(writer: &mut impl Write, report: &StatusReport) -> io::Result<()> {
    writeln!(
        writer,
        "daemon:            {}",
        if report.daemon_running {
            "running"
        } else {
            "not running (standalone)"
        }
    )?;
    writeln!(writer, "devices found:   {}", report.device_count())?;
    for device in &report.devices {
        writeln!(
            writer,
            "  {}   {:04X}:{:04X}   {}",
            device.path.display(),
            device.vendor_id,
            device.product_id,
            if device.is_config_interface {
                "config: yes"
            } else {
                "config: no"
            }
        )?;
        writeln!(
            writer,
            "  {}",
            device.product.as_deref().unwrap_or("(unknown)")
        )?;
    }

    writeln!(writer, "capabilities:")?;
    writeln!(
        writer,
        "  led control:        {}",
        if report.capabilities.led_control {
            "on"
        } else {
            "off"
        }
    )?;
    writeln!(
        writer,
        "  profile writes:     {}",
        if report.capabilities.profile_writes {
            "on"
        } else {
            "off"
        }
    )?;

    writer.flush()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use mk61_service::CapabilityStatus;
    use mk61_service::ColorSlots;

    const PLANNED_COMMANDS: [&str; 8] = [
        "list",
        "status",
        "led effects",
        "led effect",
        "led save",
        "led color",
        "led custom",
        "keys",
    ];

    fn device(path: &str, is_config_interface: bool) -> DeviceInfo {
        DeviceInfo {
            path: PathBuf::from(path),
            vendor_id: 0x1EA7,
            product_id: 0x9018,
            product: Some("SK61L SK61L".to_owned()),
            is_config_interface,
            report_id: is_config_interface.then_some(0x07),
        }
    }

    #[test]
    fn overview_documents_every_planned_command() {
        for command in PLANNED_COMMANDS {
            assert!(OVERVIEW.contains(command), "missing {command}");
        }
    }

    #[test]
    fn help_footer_documents_every_planned_command() {
        for command in PLANNED_COMMANDS {
            assert!(HELP_FOOTER.contains(command), "missing {command}");
        }
    }

    #[test]
    fn overview_discloses_implicit_flash_writes() {
        assert!(OVERVIEW.contains("Flash writes:"));
        assert!(OVERVIEW.contains("Define a per-key layout (writes flash)"));
        assert!(OVERVIEW.contains("Remove the stored layout (writes flash)"));
        assert!(OVERVIEW.contains("`led custom` and `led custom clear`"));
    }

    #[test]
    fn overview_is_written_to_the_stream() {
        let mut buffer = Vec::new();
        print_overview(&mut buffer).unwrap();
        assert_eq!(buffer, OVERVIEW.as_bytes());
    }

    #[test]
    fn overview_propagates_write_errors() {
        struct FailingWriter;

        impl Write for FailingWriter {
            fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("simulated write failure"))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        assert!(print_overview(&mut FailingWriter).is_err());
    }

    #[test]
    fn device_list_is_a_header_and_one_row_per_device() {
        let mut buffer = Vec::new();
        print_devices(
            &mut buffer,
            &[device("/dev/hidraw1", false), device("/dev/hidraw2", true)],
        )
        .unwrap();

        let text = String::from_utf8(buffer).unwrap();
        assert!(text.contains("DEVICE"));
        assert!(text.contains("VID:PID"));
        assert!(text.contains("CONFIG"));
        assert!(text.contains("/dev/hidraw1"));
        assert!(text.contains("/dev/hidraw2"));
        assert!(text.contains("1EA7:9018"));
        assert!(text.contains("SK61L SK61L"));
        assert!(text.contains("yes"));
        assert!(text.contains("no"));
        assert_eq!(text.lines().count(), 3);
    }

    #[test]
    fn empty_device_list_prints_a_clear_message() {
        let mut buffer = Vec::new();
        print_devices(&mut buffer, &[]).unwrap();

        assert_eq!(
            String::from_utf8(buffer).unwrap(),
            "no supported MK61 devices found\n"
        );
    }

    #[test]
    fn missing_product_has_a_placeholder() {
        let mut info = device("/dev/hidraw1", false);
        info.product = None;
        let mut buffer = Vec::new();

        print_devices(&mut buffer, &[info]).unwrap();

        assert!(String::from_utf8(buffer).unwrap().contains("(unknown)"));
    }

    #[test]
    fn print_effects_lists_builtins_and_reports_slot_counts() {
        let effects = vec![
            Effect {
                index: 0,
                name: "wave",
                description: "wave across the matrix",
                colors: Some(ColorSlots {
                    offset: 84,
                    count: 14,
                }),
            },
            Effect {
                index: 2,
                name: "cycle",
                description: "hue rotation",
                colors: None,
            },
        ];
        let mut buffer = Vec::new();

        print_effects(&mut buffer, &effects).unwrap();

        let text = String::from_utf8(buffer).unwrap();
        assert!(text.contains("index  effect"));
        assert!(text.contains("wave"));
        assert!(text.contains("1..14"));
        assert!(text.contains("cycle"));
        assert!(text.contains("none"));
    }

    fn status_report(devices: &[DeviceInfo], capabilities: CapabilityStatus) -> StatusReport {
        StatusReport {
            daemon_running: false,
            devices: devices.to_vec(),
            capabilities,
        }
    }

    #[test]
    fn status_prints_daemon_devices_and_capabilities() {
        let report = status_report(
            &[device("/dev/hidraw1", false), device("/dev/hidraw2", true)],
            CapabilityStatus {
                led_control: true,
                profile_writes: false,
            },
        );
        let mut buffer = Vec::new();

        print_status(&mut buffer, &report).unwrap();

        let text = String::from_utf8(buffer).unwrap();
        assert!(text.contains("daemon:            not running (standalone)"));
        assert!(text.contains("devices found:   2"));
        assert!(text.contains("/dev/hidraw1"));
        assert!(text.contains("/dev/hidraw2"));
        assert!(text.contains("capabilities:"));
        assert!(text.contains("led control:        on"));
        assert!(text.contains("profile writes:     off"));
    }

    #[test]
    fn status_prints_an_empty_system() {
        let report = status_report(&[], CapabilityStatus::default());
        let mut buffer = Vec::new();

        print_status(&mut buffer, &report).unwrap();

        let text = String::from_utf8(buffer).unwrap();
        assert!(text.contains("devices found:   0"));
        assert!(text.contains("led control:        off"));
    }
}
