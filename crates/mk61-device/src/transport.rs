//! Byte-level feature-report transport.

use std::fs::OpenOptions;
use std::path::Path;

use rustix::ioctl::Direction;
use rustix::ioctl::Ioctl;
use rustix::ioctl::ioctl as rustix_ioctl;
use rustix::ioctl::opcode;

use crate::DeviceError;

/// Size of the MK61 / SK61L vendor configuration feature report, in bytes.
pub const FEATURE_REPORT_SIZE: usize = 264;

/// Report ID of the vendor configuration feature report.
pub const FEATURE_REPORT_ID: u8 = 0x07;

/// A bidirectional channel for HID feature reports.
///
/// Implementations deal only in bytes; they must not interpret commands or
/// payloads. The real Linux implementation uses hidraw ioctls, while
/// [`crate::MockTransport`] serves tests and simulations.
pub trait Transport: Send {
    /// Writes one feature report.
    ///
    /// Write-only reports do not require a following read.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Io`] when the underlying device operation fails.
    fn write_feature(&mut self, report: &[u8]) -> Result<(), DeviceError>;

    /// Reads one feature report into the start of `report`.
    ///
    /// Returns the number of bytes written into the buffer. Implementations
    /// must return [`DeviceError::UnexpectedReportSize`] when the device
    /// produces a different number of bytes than the buffer can hold.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Io`] when the underlying device operation fails
    /// and [`DeviceError::UnexpectedReportSize`] on a size mismatch.
    fn read_feature(&mut self, report: &mut [u8]) -> Result<usize, DeviceError>;
}

/// A transport backed by a Linux hidraw node.
///
/// The type owns the open file descriptor and uses `HIDIOCSFEATURE` /
/// `HIDIOCGFEATURE` ioctls to move 264-byte feature reports. Only this type
/// (and the discovery helpers) performs the `unsafe` hidraw protocol work.
#[derive(Debug)]
pub struct HidrawTransport {
    file: std::fs::File,
}

impl HidrawTransport {
    /// Opens `path` for read/write access.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Io`] when the node cannot be opened.
    pub fn open(path: &Path) -> Result<Self, DeviceError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(DeviceError::Io)?;
        Ok(Self { file })
    }

    /// Returns the underlying file descriptor's path-less handle.
    #[must_use]
    pub fn file(&self) -> &std::fs::File {
        &self.file
    }
}

/// Wrapper around a mutable byte buffer that carries the hidraw ioctl opcode.
struct HidrawFeatureIoctl<'a> {
    buffer: &'a mut [u8],
    opcode: rustix::ioctl::Opcode,
}

// SAFETY: The opcode and buffer layout match the hidraw kernel contract:
// `HIDIOCSFEATURE(len)` / `HIDIOCGFEATURE(len)` with `len` equal to the
// buffer length. The kernel reads and writes at most `len` bytes.
unsafe impl<'a> Ioctl for HidrawFeatureIoctl<'a> {
    type Output = usize;

    const IS_MUTATING: bool = true;

    fn opcode(&self) -> rustix::ioctl::Opcode {
        self.opcode
    }

    fn as_ptr(&mut self) -> *mut core::ffi::c_void {
        self.buffer.as_mut_ptr().cast()
    }

    unsafe fn output_from_ptr(
        output: rustix::ioctl::IoctlOutput,
        _ptr: *mut core::ffi::c_void,
    ) -> rustix::io::Result<Self::Output> {
        Ok(output as usize)
    }
}

impl Transport for HidrawTransport {
    fn write_feature(&mut self, report: &[u8]) -> Result<(), DeviceError> {
        if report.len() != FEATURE_REPORT_SIZE {
            return Err(DeviceError::MalformedReport(
                "feature report must be 264 bytes",
            ));
        }
        // HIDIOCSFEATURE: the first byte is the report number.
        let mut buffer = [0_u8; FEATURE_REPORT_SIZE];
        buffer.copy_from_slice(report);

        let opcode = opcode::from_components(Direction::ReadWrite, b'H', 0x06, FEATURE_REPORT_SIZE);
        let ioctl = HidrawFeatureIoctl {
            buffer: &mut buffer,
            opcode,
        };
        // SAFETY: opcode and buffer match the hidraw kernel contract for HIDIOCSFEATURE.
        unsafe { rustix_ioctl(&self.file, ioctl) }
            .map_err(|e| DeviceError::Io(std::io::Error::from(e)))?;
        Ok(())
    }

    fn read_feature(&mut self, report: &mut [u8]) -> Result<usize, DeviceError> {
        if report.len() != FEATURE_REPORT_SIZE {
            return Err(DeviceError::MalformedReport(
                "feature report buffer must be 264 bytes",
            ));
        }
        // HIDIOCGFEATURE: the first byte is the report number requested.
        let mut buffer = [0_u8; FEATURE_REPORT_SIZE];
        buffer[0] = report[0]; // report number (0x07)
        let requested_report = report[0];

        let opcode = opcode::from_components(Direction::ReadWrite, b'H', 0x07, FEATURE_REPORT_SIZE);
        let ioctl = HidrawFeatureIoctl {
            buffer: &mut buffer,
            opcode,
        };
        // SAFETY: opcode and buffer match the hidraw kernel contract for HIDIOCGFEATURE.
        let transferred = unsafe { rustix_ioctl(&self.file, ioctl) }
            .map_err(|e| DeviceError::Io(std::io::Error::from(e)))?;

        if transferred != report.len() {
            return Err(DeviceError::UnexpectedReportSize {
                expected: report.len(),
                actual: transferred,
            });
        }
        if buffer[0] != requested_report {
            return Err(DeviceError::MalformedReport(
                "hidraw returned a different report number",
            ));
        }
        report.copy_from_slice(&buffer);
        Ok(transferred)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_report_size_is_264() {
        assert_eq!(FEATURE_REPORT_SIZE, 264);
    }

    #[test]
    fn hidio_cgfeature_opcode_matches_kernel_header() {
        let opcode = rustix::ioctl::opcode::from_components(
            rustix::ioctl::Direction::ReadWrite,
            b'H',
            0x07,
            FEATURE_REPORT_SIZE,
        );
        assert_eq!(opcode, 0xC1084807);
    }

    #[test]
    fn hidio_csfeature_opcode_matches_kernel_header() {
        let opcode = rustix::ioctl::opcode::from_components(
            rustix::ioctl::Direction::ReadWrite,
            b'H',
            0x06,
            FEATURE_REPORT_SIZE,
        );
        assert_eq!(opcode, 0xC1084806);
    }
}
