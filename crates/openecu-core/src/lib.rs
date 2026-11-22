//! Shared ECU, HW/SW and firmware provenance identities.
use serde::{Deserialize, Serialize};
use thiserror::Error;
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    #[error("{field} must not be empty")]
    EmptyField { field: &'static str },
}
pub trait Validate {
    fn validate(&self) -> Result<(), ValidationError>;
}
fn text(value: &str, field: &'static str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        Err(ValidationError::EmptyField { field })
    } else {
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareIdentity {
    pub part_number: String,
}
impl HardwareIdentity {
    pub fn new(part_number: impl Into<String>) -> Self {
        Self {
            part_number: part_number.into(),
        }
    }
}
impl Validate for HardwareIdentity {
    fn validate(&self) -> Result<(), ValidationError> {
        text(&self.part_number, "hardware.part_number")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoftwareIdentity {
    pub version: String,
}
impl SoftwareIdentity {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
        }
    }
}
impl Validate for SoftwareIdentity {
    fn validate(&self) -> Result<(), ValidationError> {
        text(&self.version, "software.version")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcuIdentity {
    pub manufacturer: String,
    pub family: String,
    pub hardware: HardwareIdentity,
    pub software: SoftwareIdentity,
}
impl Validate for EcuIdentity {
    fn validate(&self) -> Result<(), ValidationError> {
        text(&self.manufacturer, "manufacturer")?;
        text(&self.family, "family")?;
        self.hardware.validate()?;
        self.software.validate()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareProvenance {
    pub source_label: String,
    pub sha256: String,
}
impl Validate for FirmwareProvenance {
    fn validate(&self) -> Result<(), ValidationError> {
        text(&self.source_label, "source_label")?;
        text(&self.sha256, "sha256")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_an_ecu_identity_without_a_manufacturer() {
        let x = EcuIdentity {
            manufacturer: "".into(),
            family: "EDC17".into(),
            hardware: HardwareIdentity::new("HW"),
            software: SoftwareIdentity::new("SW"),
        };
        assert!(x.validate().is_err());
    }
    #[test]
    fn accepts_a_complete_ecu_identity() {
        let x = EcuIdentity {
            manufacturer: "Bosch".into(),
            family: "EDC17".into(),
            hardware: HardwareIdentity::new("HW"),
            software: SoftwareIdentity::new("SW"),
        };
        assert!(x.validate().is_ok());
    }
}
