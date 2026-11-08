//! End-to-end tests for the `mk61` binary.
//!
//! These tests exercise argument parsing, exit codes, discovery and streaming
//! without any hardware access. Discovery tests point `MK61_SYSFS_ROOT` at a
//! temporary fake sysfs tree.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const MK61_UEVENT: &str =
    "DRIVER=hid-generic\nHID_ID=0003:00001EA7:00009018\nHID_NAME=SK61L SK61L\n";
const OTHER_UEVENT: &str =
    "DRIVER=hid-generic\nHID_ID=0003:00001BCF:000008A0\nHID_NAME=Other keyboard\n";
const CONFIG_DESCRIPTOR: &[u8] = &[
    0x06, 0x01, 0xFF, // Usage Page (0xFF01)
    0x09, 0x01, // Usage (1)
    0xA1, 0x01, // Collection (Application)
    0x85, 0x07, // Report ID (7)
    0x75, 0x08, // Report Size (8)
    0x96, 0x07, 0x01, // Report Count (263)
    0xB1, 0x02, // Feature
    0xC0, // End Collection
];

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mk61"))
        .args(args)
        .output()
        .expect("failed to run mk61")
}

fn run_with_env(args: &[&str], environment: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mk61"));
    command.args(args);
    for (key, value) in environment {
        command.env(key, value);
    }
    command.output().expect("failed to run mk61")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is not UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is not UTF-8")
}

struct FakeSysfs {
    root: PathBuf,
}

impl FakeSysfs {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let root = std::env::temp_dir().join(format!(
            "mk61-cli-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("class/hidraw")).unwrap();
        Self { root }
    }

    fn add(&self, name: &str, uevent: &str, descriptor: Option<&[u8]>) {
        let node = self.root.join("class/hidraw").join(name).join("device");
        fs::create_dir_all(&node).unwrap();
        fs::write(node.join("uevent"), uevent).unwrap();
        if let Some(descriptor) = descriptor {
            fs::write(node.join("report_descriptor"), descriptor).unwrap();
        }
    }

    fn root_string(&self) -> &str {
        self.root.to_str().expect("temporary path is UTF-8")
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for FakeSysfs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn run_list(sysfs: &FakeSysfs) -> Output {
    run_with_env(&["list"], &[("MK61_SYSFS_ROOT", sysfs.root_string())])
}

#[test]
fn help_subcommand_prints_the_overview_and_succeeds() {
    let output = run(&["help"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());

    let text = stdout(&output);
    for command in [
        "list",
        "status",
        "led effects",
        "led effect",
        "led save",
        "led color",
        "led custom",
        "keys",
    ] {
        assert!(text.contains(command), "overview is missing {command}");
    }
    for global_option in ["--device <PATH>", "--state-dir <DIR>", "--verbose"] {
        assert!(
            text.contains(global_option),
            "overview is missing {global_option}"
        );
    }
    for exit_status in ["0  success", "1  runtime error", "2  usage error"] {
        assert!(
            text.contains(exit_status),
            "overview is missing exit status {exit_status}"
        );
    }
    assert!(text.contains("`help`, `list`, and `status` are implemented"));
}

#[test]
fn help_subcommand_has_long_help_with_an_example() {
    let output = run(&["help", "--help"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).is_empty());

    let text = stdout(&output);
    assert!(text.contains("Examples"), "long help is missing examples");
    assert!(
        text.contains("mk61 help"),
        "long help is missing the example command"
    );
}

#[test]
fn help_flag_prints_generated_help_and_succeeds() {
    let output = run(&["--help"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).is_empty());

    let text = stdout(&output);
    assert!(text.contains("Usage:"));
    assert!(text.contains("GLOBAL OPTIONS"));
    assert!(text.contains("led effects"));
    assert!(text.contains("Commands:"));
    assert!(text.contains("Implemented in this milestone: `help`, `list`, and `status`."));
}

#[test]
fn list_has_long_help_with_examples() {
    let output = run(&["list", "--help"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).is_empty());

    let text = stdout(&output);
    assert!(text.contains("Examples"));
    assert!(text.contains("mk61 list"));
    assert!(text.contains("mk61 --device /dev/hidraw3 list"));
    assert!(text.contains("never opens"));
}

#[test]
fn version_flag_reports_the_crate_version() {
    let output = run(&["--version"]);

    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert!(text.contains("mk61"));
    assert!(text.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn global_options_are_accepted_without_hardware_access() {
    let state_dir = std::env::temp_dir().join(format!(
        "mk61-cli-test-state-{}-{}",
        std::process::id(),
        line!()
    ));
    let state_dir = state_dir
        .to_str()
        .expect("temporary directory path is valid UTF-8");

    let output = run(&[
        "--device",
        "/dev/mk61-does-not-exist",
        "--state-dir",
        state_dir,
        "--verbose",
        "help",
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).is_empty());
    assert!(stdout(&output).contains("Usage:"));
    assert!(
        !std::path::Path::new(state_dir).exists(),
        "help must not create the state directory"
    );
}

#[test]
fn global_options_are_accepted_after_the_subcommand() {
    let output = run(&["help", "--device", "/dev/mk61-does-not-exist", "--verbose"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).is_empty());
    assert!(stdout(&output).contains("Usage:"));
}

#[test]
fn list_reports_an_empty_system_successfully() {
    let sysfs = FakeSysfs::new("empty");

    let output = run_list(&sysfs);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(stdout(&output), "no supported MK61 devices found\n");
}

#[test]
fn list_prints_one_row_per_mk61_device_and_marks_the_config_interface() {
    let sysfs = FakeSysfs::new("devices");
    sysfs.add("hidraw0", OTHER_UEVENT, None);
    sysfs.add("hidraw1", MK61_UEVENT, None);
    sysfs.add("hidraw2", MK61_UEVENT, Some(CONFIG_DESCRIPTOR));
    sysfs.add("hidraw3", "HID_NAME=broken uevent\n", None);

    let output = run_list(&sysfs);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());

    let text = stdout(&output);
    assert!(text.contains("DEVICE"));
    assert!(text.contains("VID:PID"));
    assert!(text.contains("CONFIG"));
    assert!(text.contains("/dev/hidraw1"));
    assert!(text.contains("/dev/hidraw2"));
    assert!(text.contains("1EA7:9018"));
    assert!(text.contains("SK61L SK61L"));
    assert!(!text.contains("/dev/hidraw0"), "non-MK61 must be hidden");
    assert!(
        !text.contains("/dev/hidraw3"),
        "broken uevent must be skipped"
    );
    assert_eq!(
        text.lines().count(),
        3,
        "expected a header and two device rows: {text}"
    );

    let config_row = text
        .lines()
        .find(|line| line.contains("/dev/hidraw2"))
        .unwrap();
    assert!(config_row.contains("yes"));
    let plain_row = text
        .lines()
        .find(|line| line.contains("/dev/hidraw1"))
        .unwrap();
    assert!(plain_row.contains("no"));
}

#[test]
fn list_sanitises_product_strings() {
    let sysfs = FakeSysfs::new("sanitise");
    sysfs.add(
        "hidraw0",
        "HID_ID=0003:00001EA7:00009018\nHID_NAME=SK61L\x1b[31m EVIL\x07\n",
        None,
    );

    let output = run_list(&sysfs);

    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert!(text.contains("SK61L EVIL"));
    assert!(!text.contains('\u{001B}'), "escape character leaked");
    assert!(!text.contains("[31m"), "ANSI sequence leaked");
}

#[test]
fn list_accepts_one_explicit_device() {
    let sysfs = FakeSysfs::new("explicit-ok");
    sysfs.add("null", MK61_UEVENT, Some(CONFIG_DESCRIPTOR));

    let output = run_with_env(
        &["--device", "/dev/null", "list"],
        &[("MK61_SYSFS_ROOT", sysfs.root_string())],
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());

    let text = stdout(&output);
    assert!(text.contains("/dev/null"));
    assert!(text.contains("1EA7:9018"));
    assert!(text.contains("yes"));
}

#[test]
fn explicit_device_rejects_a_relative_path() {
    let sysfs = FakeSysfs::new("explicit-relative");

    let output = run_with_env(
        &["--device", "hidraw0", "list"],
        &[("MK61_SYSFS_ROOT", sysfs.root_string())],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("device path must be absolute"));
}

#[test]
fn explicit_device_rejects_a_missing_path() {
    let sysfs = FakeSysfs::new("explicit-missing");

    let output = run_with_env(
        &["--device", "/dev/mk61-does-not-exist", "list"],
        &[("MK61_SYSFS_ROOT", sysfs.root_string())],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("cannot access device path"));
}

#[test]
fn explicit_device_rejects_a_directory() {
    let sysfs = FakeSysfs::new("explicit-directory");
    let directory = sysfs.path().to_str().unwrap();

    let output = run_with_env(
        &["--device", directory, "list"],
        &[("MK61_SYSFS_ROOT", sysfs.root_string())],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("not a character device"));
}

#[test]
fn explicit_device_rejects_a_regular_file() {
    let sysfs = FakeSysfs::new("explicit-file");
    let file = sysfs.path().join("regular");
    fs::write(&file, b"not a device").unwrap();

    let output = run_with_env(
        &["--device", file.to_str().unwrap(), "list"],
        &[("MK61_SYSFS_ROOT", sysfs.root_string())],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("not a character device"));
}

#[test]
fn explicit_device_rejects_a_symlink() {
    let sysfs = FakeSysfs::new("explicit-symlink");
    let link = sysfs.path().join("device-link");
    std::os::unix::fs::symlink("/dev/null", &link).unwrap();

    let output = run_with_env(
        &["--device", link.to_str().unwrap(), "list"],
        &[("MK61_SYSFS_ROOT", sysfs.root_string())],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("symbolic link"));
}

#[test]
fn explicit_device_rejects_a_non_mk61_node() {
    let sysfs = FakeSysfs::new("explicit-other");
    sysfs.add("null", OTHER_UEVENT, None);

    let output = run_with_env(
        &["--device", "/dev/null", "list"],
        &[("MK61_SYSFS_ROOT", sysfs.root_string())],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("not a supported MK61 device"));
}

#[test]
fn explicit_device_requires_sysfs_metadata() {
    let sysfs = FakeSysfs::new("explicit-no-metadata");

    let output = run_with_env(
        &["--device", "/dev/null", "list"],
        &[("MK61_SYSFS_ROOT", sysfs.root_string())],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("cannot read sysfs metadata"));
}

#[test]
fn relative_sysfs_override_is_a_runtime_error() {
    let output = run_with_env(&["list"], &[("MK61_SYSFS_ROOT", "relative/fake-sysfs")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("MK61_SYSFS_ROOT"));
    assert!(stderr(&output).contains("absolute"));
}

#[test]
fn status_prints_the_device_and_capability_report() {
    let sysfs = FakeSysfs::new("status-empty");

    let output = run_with_env(&["status"], &[("MK61_SYSFS_ROOT", sysfs.root_string())]);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());

    let text = stdout(&output);
    assert!(text.contains("devices found"));
    assert!(text.contains("capabilities:"));
    assert!(text.contains("daemon:"));
}

#[test]
fn status_accepts_one_explicit_device() {
    let sysfs = FakeSysfs::new("status-explicit");
    sysfs.add("null", MK61_UEVENT, Some(CONFIG_DESCRIPTOR));

    let output = run_with_env(
        &["--device", "/dev/null", "status"],
        &[("MK61_SYSFS_ROOT", sysfs.root_string())],
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert!(stdout(&output).contains("1EA7:9018"));
}

#[test]
fn status_rejects_a_missing_device() {
    let sysfs = FakeSysfs::new("status-missing");

    let output = run_with_env(
        &["--device", "/dev/mk61-does-not-exist", "status"],
        &[("MK61_SYSFS_ROOT", sysfs.root_string())],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn planned_commands_are_still_usage_errors() {
    for args in [
        &["led", "effects"][..],
        &["led", "effect", "wave"][..],
        &["led", "save"][..],
        &["led", "color", "wave", "FF0000"][..],
        &["led", "custom", "ESC=FF0000"][..],
        &["keys"][..],
    ] {
        let output = run(args);

        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?} should not be implemented in this milestone"
        );
        assert!(output.stdout.is_empty());
        assert!(!stderr(&output).is_empty());
    }
}

#[test]
fn missing_command_is_a_usage_error() {
    let output = run(&[]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Usage:"));
}

#[test]
fn unknown_command_is_a_usage_error() {
    let output = run(&["definitely-not-a-command"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unrecognized subcommand"));
}
