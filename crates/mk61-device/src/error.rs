//! Device-layer errors.

use std::io;
use std::path::PathBuf;

/// Errors raised while discovering or communicating with the keyboard.
#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    /// An operating-system operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// The device returned a report with a different size than requested.
    #[error("device returned {actual} bytes, expected {expected}")]
    UnexpectedReportSize {
        /// Size the caller asked for.
        expected: usize,
        /// Size the device actually produced.
        actual: usize,
    },

    /// No supported keyboard was found.
    #[error("no supported MK61 keyboard found")]
    NotFound,

    /// A report did not follow the expected protocol layout.
    #[error("malformed device report: {0}")]
    MalformedReport(&'static str),

    /// The operation is reserved for a later milestone.
    #[error("not implemented yet: {0}")]
    Unsupported(&'static str),

    /// The sysfs root used for discovery is not an absolute path.
    #[error("sysfs root must be an absolute path: {0}")]
    InvalidSysfsRoot(PathBuf),

    /// An explicit device path is not absolute.
    #[error("device path must be absolute: {0}")]
    DevicePathNotAbsolute(PathBuf),

    /// An explicit device path is a symbolic link.
    #[error("device path must not be a symbolic link: {0}")]
    DevicePathIsSymlink(PathBuf),

    /// An explicit device path is not a character device.
    #[error("device path is not a character device: {0}")]
    NotCharacterDevice(PathBuf),

    /// An explicit device path has no usable file name for sysfs lookup.
    #[error("device path has no usable file name: {0}")]
    InvalidDeviceName(PathBuf),

    /// An explicit device path could not be inspected.
    #[error("cannot access device path {path}: {source}")]
    DevicePathIo {
        /// Path supplied by the caller.
        path: PathBuf,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },

    /// Sysfs metadata for an explicit device could not be read.
    #[error("cannot read sysfs metadata for {path}: {source}")]
    SysfsIo {
        /// Sysfs path that could not be read.
        path: PathBuf,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },

    /// Sysfs metadata for an explicit device was malformed.
    #[error("malformed sysfs data in {path}: {reason}")]
    MalformedSysfs {
        /// Sysfs path containing the malformed data.
        path: PathBuf,
        /// Human-readable reason.
        reason: String,
    },

    /// The explicit device does not have the supported vendor and product IDs.
    #[error("not a supported MK61 device: {0}")]
    UnsupportedDevice(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_errors_name_the_offending_path() {
        let path = PathBuf::from("relative/hidraw0");
        let error = DeviceError::DevicePathNotAbsolute(path);

        assert_eq!(
            error.to_string(),
            "device path must be absolute: relative/hidraw0"
        );
    }
}
