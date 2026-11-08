//! Byte-level feature-report transport.

use crate::DeviceError;

/// A bidirectional channel for HID feature reports.
///
/// Implementations deal only in bytes; they must not interpret commands or
/// payloads. The real Linux implementation will use hidraw ioctls, while
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
