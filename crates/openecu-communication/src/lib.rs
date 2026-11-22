//! Transport and protocol contracts only; implementations may be future adapters.
use thiserror::Error;
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CommunicationError {
    #[error("transport frame is empty")]
    EmptyFrame,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportFrame {
    pub bytes: Vec<u8>,
}
impl TransportFrame {
    pub fn new(bytes: Vec<u8>) -> Result<Self, CommunicationError> {
        if bytes.is_empty() {
            Err(CommunicationError::EmptyFrame)
        } else {
            Ok(Self { bytes })
        }
    }
}
pub trait Transport {
    type Error;
    fn send(&mut self, frame: TransportFrame) -> Result<(), Self::Error>;
    fn receive(&mut self) -> Result<TransportFrame, Self::Error>;
}
pub trait Protocol {
    type Error;
    fn encode(&self, payload: &[u8]) -> Result<TransportFrame, Self::Error>;
    fn decode(&self, frame: &TransportFrame) -> Result<Vec<u8>, Self::Error>;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_an_empty_transport_frame() {
        assert!(TransportFrame::new(Vec::new()).is_err())
    }
    #[test]
    fn preserves_frame_payload() {
        assert_eq!(TransportFrame::new(vec![1, 2]).unwrap().bytes, vec![1, 2])
    }
}
