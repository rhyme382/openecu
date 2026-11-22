//! Calibration models have no ECU transport dependency.
use serde::{Deserialize, Serialize};
use thiserror::Error;
#[derive(Debug, Error, PartialEq)]
pub enum CalibrationError {
    #[error("axis `{axis}` is empty")]
    EmptyAxis { axis: String },
    #[error("axis `{axis}` is not strictly increasing")]
    UnorderedAxis { axis: String },
    #[error("invalid matrix shape")]
    InvalidShape,
    #[error("unit is empty")]
    EmptyUnit,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Axis {
    pub name: String,
    pub unit: String,
    pub values: Vec<f64>,
}
impl Axis {
    pub fn new(
        name: impl Into<String>,
        unit: impl Into<String>,
        values: Vec<f64>,
    ) -> Result<Self, CalibrationError> {
        let x = Self {
            name: name.into(),
            unit: unit.into(),
            values,
        };
        if x.unit.trim().is_empty() {
            return Err(CalibrationError::EmptyUnit);
        }
        if x.values.is_empty() {
            return Err(CalibrationError::EmptyAxis { axis: x.name });
        }
        if x.values.windows(2).any(|p| p[0] >= p[1]) {
            return Err(CalibrationError::UnorderedAxis { axis: x.name });
        }
        Ok(x)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationMap {
    pub x_axis: Axis,
    pub y_axis: Axis,
    pub values: Vec<Vec<f64>>,
    pub unit: String,
}
impl CalibrationMap {
    pub fn new(
        x_axis: Axis,
        y_axis: Axis,
        values: Vec<Vec<f64>>,
        unit: impl Into<String>,
    ) -> Result<Self, CalibrationError> {
        let x = Self {
            x_axis,
            y_axis,
            values,
            unit: unit.into(),
        };
        if x.unit.trim().is_empty() {
            return Err(CalibrationError::EmptyUnit);
        }
        if x.values.len() != x.x_axis.values.len()
            || x.values.iter().any(|r| r.len() != x.y_axis.values.len())
        {
            return Err(CalibrationError::InvalidShape);
        }
        Ok(x)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_a_ragged_map() {
        let x = CalibrationMap::new(
            Axis::new("rpm", "rpm", vec![1., 2.]).unwrap(),
            Axis::new("load", "%", vec![1., 2.]).unwrap(),
            vec![vec![1., 2.], vec![3.]],
            "mg",
        );
        assert!(x.is_err())
    }
    #[test]
    fn rejects_an_unordered_axis() {
        assert!(Axis::new("rpm", "rpm", vec![2., 2.]).is_err())
    }
}
