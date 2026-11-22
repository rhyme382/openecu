//! Contracts for reviewable, controlled calibration modifications.
use serde::{Deserialize, Serialize};
use thiserror::Error;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChangeValue {
    Number(f64),
    Text(String),
}
#[derive(Debug, Error, PartialEq)]
pub enum SolutionError {
    #[error("target must not be empty")]
    EmptyTarget,
    #[error("rationale must not be empty")]
    EmptyRationale,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlledChange {
    pub target: String,
    pub rationale: String,
    pub value: ChangeValue,
}
impl ControlledChange {
    pub fn new(
        target: impl Into<String>,
        rationale: impl Into<String>,
        value: ChangeValue,
    ) -> Result<Self, SolutionError> {
        let x = Self {
            target: target.into(),
            rationale: rationale.into(),
            value,
        };
        if x.target.trim().is_empty() {
            return Err(SolutionError::EmptyTarget);
        }
        if x.rationale.trim().is_empty() {
            return Err(SolutionError::EmptyRationale);
        }
        Ok(x)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_a_change_without_a_rationale() {
        assert!(ControlledChange::new("torque", "", ChangeValue::Number(1.)).is_err())
    }
    #[test]
    fn accepts_a_reviewable_change() {
        assert!(ControlledChange::new("torque", "validated", ChangeValue::Number(1.)).is_ok())
    }
}
