//! Safe calculation of typed `FIX_AXIS` descriptions.
//!
//! This module only evaluates the parameters already parsed from A2L. It does
//! not read binary data and does not decode CURVE/MAP characteristics.

use crate::a2l::{AxisAttribute, AxisDescription, CompuMethod, FixAxisPar, FixAxisParDist};
use thiserror::Error;

const MAX_AXIS_POINTS: u32 = 1_048_576;
const MIN_SHIFT: i64 = -1074;
const MAX_SHIFT: i64 = 1023;

#[derive(Debug, Clone, Copy)]
pub struct FixedAxisRequest<'a> {
    pub axis: &'a AxisDescription,
    pub capacity: Option<u32>,
    pub dimensions: Option<&'a [u32]>,
    pub conversion: FixedAxisConversion<'a>,
}

#[derive(Debug, Clone, Copy)]
pub enum FixedAxisConversion<'a> {
    /// Keep the generated values in their source/raw domain.
    Raw,
    /// A conversion method was found and independently resolved as IDENTICAL.
    Identical(&'a CompuMethod),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FixedAxisMethod {
    Par(FixAxisPar),
    ParDist(FixAxisParDist),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FixedAxisValue {
    pub index: u32,
    pub raw: f64,
    pub physical: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FixedAxisProvenance {
    method: FixedAxisMethod,
    indices: Vec<u32>,
}

impl FixedAxisProvenance {
    pub fn method(&self) -> &FixedAxisMethod {
        &self.method
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FixedAxisResult {
    values: Vec<FixedAxisValue>,
    provenance: FixedAxisProvenance,
}

impl FixedAxisResult {
    pub fn values(&self) -> &[FixedAxisValue] {
        &self.values
    }

    pub fn raw_values(&self) -> Vec<f64> {
        self.values.iter().map(|value| value.raw).collect()
    }

    pub fn physical_values(&self) -> Option<Vec<f64>> {
        self.values
            .first()
            .and_then(|value| value.physical.map(|_| ()))
            .map(|_| {
                self.values
                    .iter()
                    .map(|value| value.physical.unwrap())
                    .collect()
            })
    }

    pub fn method(&self) -> &FixedAxisMethod {
        self.provenance.method()
    }

    pub fn provenance(&self) -> &FixedAxisProvenance {
        &self.provenance
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum FixedAxisError {
    #[error("AXIS_DESCR is not FIX_AXIS")]
    NotFixedAxis,
    #[error("FIX_AXIS requires exactly one supported parameter declaration")]
    ContradictoryParameters,
    #[error("FIX_AXIS has no FIX_AXIS_PAR or FIX_AXIS_PAR_DIST parameters")]
    MissingParameters,
    #[error("FIX_AXIS capacity is absent")]
    MissingCapacity,
    #[error("FIX_AXIS capacity must be non-zero")]
    ZeroCapacity,
    #[error("FIX_AXIS point count {count} exceeds capacity {capacity}")]
    CapacityExceeded { count: u32, capacity: u32 },
    #[error("FIX_AXIS point count {count} exceeds MAX_AXIS_PTS {maximum}")]
    MaxAxisPointsExceeded { count: u32, maximum: u32 },
    #[error("FIX_AXIS dimensions are inconsistent with point count {count}")]
    InconsistentDimensions { count: u32 },
    #[error("FIX_AXIS dimension product overflow")]
    DimensionOverflow,
    #[error("FIX_AXIS shift {shift} is outside the finite binary64 range")]
    InvalidShift { shift: i64 },
    #[error(
        "FIX_AXIS integer value {value} cannot be represented exactly as binary64 at index {index}"
    )]
    InexactInteger { index: u32, value: i128 },
    #[error("FIX_AXIS arithmetic overflow at index {index}")]
    ArithmeticOverflow { index: u32 },
    #[error("FIX_AXIS generated a non-finite value at index {index}")]
    NonFiniteValue { index: u32 },
    #[error("conversion `{name}` is unknown or was not resolved")]
    UnknownConversion { name: String },
    #[error("conversion `{name}` is not IDENTICAL")]
    UnsupportedConversion { name: String },
    #[error("FIX_AXIS limits are invalid")]
    InvalidLimits,
    #[error("FIX_AXIS value at index {index} is outside its declared limits")]
    OutsideLimits { index: u32 },
    #[error("IDENTICAL conversion method name does not match the axis reference")]
    ConversionNameMismatch,
}

pub fn calculate_fixed_axis(
    request: FixedAxisRequest<'_>,
) -> Result<FixedAxisResult, FixedAxisError> {
    let axis = request.axis;
    if axis.attribute != Some(AxisAttribute::Fix) {
        return Err(FixedAxisError::NotFixedAxis);
    }
    if axis.fix_axis_par.is_some() && axis.fix_axis_par_dist.is_some()
        || axis.fix_axis_par_list.is_some()
    {
        return Err(FixedAxisError::ContradictoryParameters);
    }
    let method = match (&axis.fix_axis_par, &axis.fix_axis_par_dist) {
        (Some(parameter), None) => FixedAxisMethod::Par(parameter.clone()),
        (None, Some(parameter)) => FixedAxisMethod::ParDist(parameter.clone()),
        (None, None) => return Err(FixedAxisError::MissingParameters),
        (Some(_), Some(_)) => return Err(FixedAxisError::ContradictoryParameters),
    };
    let count = match &method {
        FixedAxisMethod::Par(parameter) => parameter.number_of_points,
        FixedAxisMethod::ParDist(parameter) => parameter.number_of_points,
    };
    if count > MAX_AXIS_POINTS {
        return Err(FixedAxisError::MaxAxisPointsExceeded {
            count,
            maximum: MAX_AXIS_POINTS,
        });
    }
    let capacity = request.capacity.ok_or(FixedAxisError::MissingCapacity)?;
    if capacity == 0 {
        return Err(FixedAxisError::ZeroCapacity);
    }
    if count > capacity {
        return Err(FixedAxisError::CapacityExceeded { count, capacity });
    }
    if let Some(maximum) = axis.max_axis_points
        && count > maximum
    {
        return Err(FixedAxisError::MaxAxisPointsExceeded { count, maximum });
    }
    if let Some(dimensions) = request.dimensions {
        if dimensions.is_empty() {
            return Err(FixedAxisError::InconsistentDimensions { count });
        }
        let product = dimensions.iter().try_fold(1u32, |product, dimension| {
            product
                .checked_mul(*dimension)
                .ok_or(FixedAxisError::DimensionOverflow)
        })?;
        if product != count {
            return Err(FixedAxisError::InconsistentDimensions { count });
        }
    }

    let physical = resolve_conversion(axis, request.conversion)?;
    let mut values = Vec::with_capacity(count as usize);
    let mut raw_values = Vec::with_capacity(count as usize);
    for index in 0..count {
        let raw = value_at(&method, index)?;
        if !raw.is_finite() {
            return Err(FixedAxisError::NonFiniteValue { index });
        }
        raw_values.push(raw);
        values.push(FixedAxisValue {
            index,
            raw,
            physical: physical.then_some(raw),
        });
    }
    validate_limits(axis, &raw_values)?;
    Ok(FixedAxisResult {
        values,
        provenance: FixedAxisProvenance {
            method,
            indices: (0..count).collect(),
        },
    })
}

fn resolve_conversion(
    axis: &AxisDescription,
    conversion: FixedAxisConversion<'_>,
) -> Result<bool, FixedAxisError> {
    let Some(reference) = axis.conversion_reference.as_deref() else {
        return Ok(false);
    };
    if reference == "NO_COMPU_METHOD" {
        return Ok(false);
    }
    match conversion {
        FixedAxisConversion::Raw => Err(FixedAxisError::UnknownConversion {
            name: reference.to_owned(),
        }),
        FixedAxisConversion::Identical(method) => {
            if method.name != reference {
                return Err(FixedAxisError::ConversionNameMismatch);
            }
            if method.conversion_type.as_deref() != Some("IDENTICAL") {
                return Err(FixedAxisError::UnsupportedConversion {
                    name: reference.to_owned(),
                });
            }
            Ok(true)
        }
    }
}

fn value_at(method: &FixedAxisMethod, index: u32) -> Result<f64, FixedAxisError> {
    match method {
        FixedAxisMethod::Par(parameter) => {
            if !(MIN_SHIFT..=MAX_SHIFT).contains(&parameter.shift) {
                return Err(FixedAxisError::InvalidShift {
                    shift: parameter.shift,
                });
            }
            if parameter.shift >= 0 {
                let offset = i128::from(parameter.offset);
                if parameter.shift <= 126 {
                    let step = 1i128 << parameter.shift;
                    let value = offset
                        .checked_add(
                            i128::from(index)
                                .checked_mul(step)
                                .ok_or(FixedAxisError::ArithmeticOverflow { index })?,
                        )
                        .ok_or(FixedAxisError::ArithmeticOverflow { index })?;
                    return exact_integer_to_f64(value, index);
                }
                // i128 cannot represent these powers. The zero offset is the
                // only case this narrow API can prove exact without a larger
                // integer representation; the f64 operation still rejects inf.
                if parameter.offset != 0 {
                    return Err(FixedAxisError::InexactInteger {
                        index,
                        value: offset,
                    });
                }
                let step = power_of_two(parameter.shift);
                let value = f64::from(index) * step;
                if !value.is_finite() {
                    return Err(FixedAxisError::NonFiniteValue { index });
                }
                Ok(value)
            } else {
                let offset = i128::from(parameter.offset);
                ensure_exact_integer(offset, index)?;
                let step = power_of_two(parameter.shift);
                let fraction = f64::from(index) * step;
                // At this magnitude a non-zero fractional part may be lost by
                // addition. Reject it rather than report a rounded raw value.
                if index != 0 && parameter.offset.unsigned_abs() >= 1u64 << 53 {
                    return Err(FixedAxisError::InexactInteger {
                        index,
                        value: offset,
                    });
                }
                let value = parameter.offset as f64 + fraction;
                if !value.is_finite() {
                    return Err(FixedAxisError::NonFiniteValue { index });
                }
                Ok(value)
            }
        }
        FixedAxisMethod::ParDist(parameter) => {
            let value = i128::from(parameter.offset)
                .checked_add(
                    i128::from(index)
                        .checked_mul(i128::from(parameter.distance))
                        .ok_or(FixedAxisError::ArithmeticOverflow { index })?,
                )
                .ok_or(FixedAxisError::ArithmeticOverflow { index })?;
            if value < i128::from(i64::MIN) || value > i128::from(i64::MAX) {
                return Err(FixedAxisError::ArithmeticOverflow { index });
            }
            exact_integer_to_f64(value, index)
        }
    }
}

fn power_of_two(shift: i64) -> f64 {
    debug_assert!((-1074..=1023).contains(&shift));
    if shift >= -1022 {
        f64::from_bits(((shift + 1023) as u64) << 52)
    } else {
        f64::from_bits(1u64 << (shift + 1074))
    }
}

fn ensure_exact_integer(value: i128, index: u32) -> Result<(), FixedAxisError> {
    if is_exact_binary64_integer(value) {
        Ok(())
    } else {
        Err(FixedAxisError::InexactInteger { index, value })
    }
}

fn exact_integer_to_f64(value: i128, index: u32) -> Result<f64, FixedAxisError> {
    ensure_exact_integer(value, index)?;
    let result = value as f64;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(FixedAxisError::NonFiniteValue { index })
    }
}

fn is_exact_binary64_integer(value: i128) -> bool {
    let magnitude = value.unsigned_abs();
    let bits = 128 - magnitude.leading_zeros();
    if bits <= 53 {
        return true;
    }
    let discarded = bits - 53;
    (magnitude & ((1u128 << discarded) - 1)) == 0
}

fn validate_limits(axis: &AxisDescription, values: &[f64]) -> Result<(), FixedAxisError> {
    let (Some(lower), Some(upper)) = (&axis.lower_limit, &axis.upper_limit) else {
        return Ok(());
    };
    if !lower.value().is_finite() || !upper.value().is_finite() || lower.value() > upper.value() {
        return Err(FixedAxisError::InvalidLimits);
    }
    for (index, value) in values.iter().enumerate() {
        if *value < lower.value() || *value > upper.value() {
            return Err(FixedAxisError::OutsideLimits {
                index: index as u32,
            });
        }
    }
    Ok(())
}
