//! In-memory transport used by tests and by future GUI demos.

use std::collections::VecDeque;

use crate::{DeviceError, Transport};

/// A deterministic [`Transport`] that records writes and replays queued reads.
///
/// Writes are appended to an internal log. Each read consumes one queued
/// response, which must be exactly as long as the caller's buffer. Queue a
/// response with [`MockTransport::push_response`] before calling
/// [`Transport::read_feature`].
#[derive(Debug, Default)]
pub struct MockTransport {
    responses: VecDeque<Vec<u8>>,
    writes: Vec<Vec<u8>>,
}

impl MockTransport {
    /// Creates an empty mock transport.
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues the response for the next read.
    pub fn push_response(&mut self, response: impl Into<Vec<u8>>) {
        self.responses.push_back(response.into());
    }

    /// Returns every feature report written so far, in order.
    pub fn writes(&self) -> &[Vec<u8>] {
        &self.writes
    }

    /// Returns the most recently written feature report, if any.
    pub fn last_write(&self) -> Option<&[u8]> {
        self.writes.last().map(Vec::as_slice)
    }
}

impl Transport for MockTransport {
    fn write_feature(&mut self, report: &[u8]) -> Result<(), DeviceError> {
        self.writes.push(report.to_vec());
        Ok(())
    }

    fn read_feature(&mut self, report: &mut [u8]) -> Result<usize, DeviceError> {
        let Some(response) = self.responses.pop_front() else {
            return Err(DeviceError::MalformedReport(
                "mock transport has no queued response",
            ));
        };
        if response.len() != report.len() {
            return Err(DeviceError::UnexpectedReportSize {
                expected: report.len(),
                actual: response.len(),
            });
        }
        report.copy_from_slice(&response);
        Ok(response.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_writes_in_order() {
        let mut transport = MockTransport::new();
        transport.write_feature(&[1, 2, 3]).unwrap();
        transport.write_feature(&[4, 5]).unwrap();

        assert_eq!(transport.writes(), &[vec![1, 2, 3], vec![4, 5]]);
        assert_eq!(transport.last_write(), Some([4, 5].as_slice()));
    }

    #[test]
    fn read_replays_the_next_queued_response() {
        let mut transport = MockTransport::new();
        transport.push_response([9_u8, 8, 7]);
        let mut buffer = [0_u8; 3];

        assert_eq!(transport.read_feature(&mut buffer).unwrap(), 3);
        assert_eq!(buffer, [9, 8, 7]);
    }

    #[test]
    fn read_rejects_size_mismatch() {
        let mut transport = MockTransport::new();
        transport.push_response([1_u8, 2]);
        let mut buffer = [0_u8; 3];

        assert!(matches!(
            transport.read_feature(&mut buffer),
            Err(DeviceError::UnexpectedReportSize {
                expected: 3,
                actual: 2
            })
        ));
    }

    #[test]
    fn read_without_a_queued_response_fails() {
        let mut transport = MockTransport::new();
        assert!(transport.read_feature(&mut [0_u8; 1]).is_err());
    }
}
