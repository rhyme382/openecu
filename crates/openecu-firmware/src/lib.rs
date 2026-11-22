//! In-memory firmware image abstractions; no file-system or ECU I/O.
use thiserror::Error;
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FirmwareError {
    #[error("range length must be non-zero")]
    EmptyRange,
    #[error("range exceeds firmware image")]
    OutOfBounds,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareRange {
    pub offset: usize,
    pub length: usize,
}
impl FirmwareRange {
    pub fn new(offset: usize, length: usize) -> Result<Self, FirmwareError> {
        if length == 0 {
            Err(FirmwareError::EmptyRange)
        } else {
            Ok(Self { offset, length })
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareImage {
    pub label: String,
    bytes: Vec<u8>,
}
impl FirmwareImage {
    pub fn new(label: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            label: label.into(),
            bytes,
        }
    }
    pub fn len(&self) -> usize {
        self.bytes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    pub fn read_range(&self, range: FirmwareRange) -> Result<&[u8], FirmwareError> {
        let end = range
            .offset
            .checked_add(range.length)
            .ok_or(FirmwareError::OutOfBounds)?;
        self.bytes
            .get(range.offset..end)
            .ok_or(FirmwareError::OutOfBounds)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_a_read_outside_the_firmware_image() {
        let x = FirmwareImage::new("fixture", vec![0, 1, 2]);
        assert!(x.read_range(FirmwareRange::new(2, 2).unwrap()).is_err())
    }
    #[test]
    fn returns_requested_bytes() {
        let x = FirmwareImage::new("fixture", vec![0, 1, 2]);
        assert_eq!(
            x.read_range(FirmwareRange::new(1, 2).unwrap()).unwrap(),
            [1, 2]
        )
    }
}
