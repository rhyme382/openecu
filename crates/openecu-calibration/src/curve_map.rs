//! Conservative CURVE/MAP decoding on top of the validated T002-C2 APIs.

use crate::{
    CalibrationError, CalibrationProvenance, CalibrationValue, Conversion, MatrixLayout,
    PhysicalValue, RawValue, reconstruct_com_axis, reconstruct_std_axis,
};
use openecu_definitions::a2l::{AxisAttribute, AxisDescription, Characteristic, Module};
use openecu_definitions::fixed_axis::{
    FixedAxisConversion, FixedAxisRequest, calculate_fixed_axis,
};
use openecu_definitions::record_layout::{
    ElementKind, PointCounts, RecordAxis, RecordDataType, RecordPlan,
};
use openecu_firmware::{ByteOrder, EcuAddressMap, FirmwareImage, PrimitiveType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CharacteristicKind {
    Curve,
    Map,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AxisValueSource {
    Binary,
    Generated,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AxisRawValue {
    Binary(RawValue),
    Generated(f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisValue {
    pub raw: AxisRawValue,
    pub physical: Option<f64>,
    pub unit: Option<String>,
    pub conversion_reference: Option<String>,
    pub provenance: Option<CalibrationProvenance>,
    pub source: AxisValueSource,
}

impl AxisValue {
    pub fn raw_f64(&self) -> f64 {
        match self.raw {
            AxisRawValue::Binary(RawValue::Unsigned(value)) => value as f64,
            AxisRawValue::Binary(RawValue::Signed(value)) => value as f64,
            AxisRawValue::Generated(value) => value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecodedAxis {
    pub name: String,
    pub source: AxisValueSource,
    pub values: Vec<AxisValue>,
}

impl DecodedAxis {
    pub fn raw_values(&self) -> Vec<f64> {
        self.values.iter().map(AxisValue::raw_f64).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecodedCharacteristic {
    pub name: String,
    pub kind: CharacteristicKind,
    pub axes: Vec<DecodedAxis>,
    pub values: Vec<CalibrationValue>,
    /// CURVE: `[x]`; MAP: `[y, x]`.
    pub dimensions: Vec<usize>,
    pub layout: MatrixLayout,
    pub conversion: Conversion,
    pub conversion_reference: String,
    pub unit: Option<String>,
    pub record_layout: String,
}

impl DecodedCharacteristic {
    pub fn pairs(&self) -> Vec<(f64, &CalibrationValue)> {
        self.axes[0]
            .values
            .iter()
            .zip(&self.values)
            .map(|(x, value)| (x.raw_f64(), value))
            .collect()
    }

    pub fn z_at(&self, y: usize, x: usize) -> Option<&CalibrationValue> {
        if self.kind != CharacteristicKind::Map || self.dimensions.len() != 2 {
            return None;
        }
        let width = self.dimensions[1];
        if y >= self.dimensions[0] || x >= width {
            return None;
        }
        self.values.get(y.checked_mul(width)?.checked_add(x)?)
    }
}

pub fn decode_characteristic(
    module: &Module,
    characteristic: &Characteristic,
    image: &FirmwareImage,
    mapping: &EcuAddressMap,
) -> Result<DecodedCharacteristic, CalibrationError> {
    let kind = match characteristic.characteristic_type.as_deref() {
        Some("CURVE") => CharacteristicKind::Curve,
        Some("MAP") => CharacteristicKind::Map,
        Some(other) => {
            return Err(CalibrationError::UnsupportedA2lCharacteristicType {
                characteristic: characteristic.name.clone(),
                characteristic_type: other.into(),
            });
        }
        None => return Err(missing(characteristic, "characteristic type")),
    };
    let expected_axes = if kind == CharacteristicKind::Curve {
        1
    } else {
        2
    };
    if characteristic.axis_descriptions.len() != expected_axes {
        return Err(CalibrationError::UnsupportedA2lLayout {
            characteristic: characteristic.name.clone(),
            reason: format!("expected {expected_axes} axis descriptions"),
        });
    }
    let address = characteristic
        .address
        .ok_or_else(|| missing(characteristic, "ECU address"))?;
    let (_, conversion_reference, unit) = characteristic_conversion(module, characteristic)?;
    let layout_name = characteristic
        .layout_reference
        .as_deref()
        .filter(|v| *v != "NO_RECORD_LAYOUT")
        .ok_or_else(|| missing(characteristic, "record layout"))?;
    let layout = module
        .record_layouts
        .iter()
        .find(|v| v.name == layout_name)
        .ok_or_else(|| missing(characteristic, "record layout"))?;
    let plan = RecordPlan::from_layout_with_mod_common(layout, module.mod_common.as_ref())?;
    plan.check_base_address(address)?;
    let fnc = plan.element(ElementKind::FncValues).ok_or_else(|| {
        CalibrationError::MissingA2lMetadata {
            characteristic: characteristic.name.clone(),
            field: "FNC_VALUES",
        }
    })?;
    if fnc.index_mode.as_deref() != Some("ROW_DIR") {
        return Err(CalibrationError::UnsupportedA2lLayout {
            characteristic: characteristic.name.clone(),
            reason: "FNC_VALUES index mode must be ROW_DIR to prove Z[y][x]".into(),
        });
    }
    for (index, descriptor) in characteristic.axis_descriptions.iter().enumerate() {
        if matches!(
            descriptor.attribute,
            Some(AxisAttribute::Std | AxisAttribute::Com)
        ) {
            let axis_kind = if index == 0 {
                RecordAxis::X
            } else {
                RecordAxis::Y
            };
            let axis = plan
                .element(ElementKind::AxisPts(axis_kind))
                .ok_or_else(|| CalibrationError::UnsupportedA2lLayout {
                    characteristic: characteristic.name.clone(),
                    reason: format!("missing AXIS_PTS_{}", if index == 0 { "X" } else { "Y" }),
                })?;
            if fnc.position < axis.position {
                return Err(CalibrationError::UnsupportedA2lLayout {
                    characteristic: characteristic.name.clone(),
                    reason: "FNC_VALUES precedes a variable axis; its offset depends on an unproven cardinality".into(),
                });
            }
        }
    }
    let axes = decode_axes(module, characteristic, image, mapping)?;
    let axis_counts = axes
        .iter()
        .map(|axis| {
            u64::try_from(axis.values.len()).map_err(|_| CalibrationError::MatrixSizeOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !characteristic.dimensions.is_empty()
        && (characteristic.dimensions.len() != expected_axes
            || characteristic
                .dimensions
                .iter()
                .zip(&axis_counts)
                .any(|(&declared, &actual)| u64::from(declared) != actual))
    {
        return Err(CalibrationError::UnsupportedA2lLayout {
            characteristic: characteristic.name.clone(),
            reason: "declared axis dimensions do not match reconstructed cardinality".into(),
        });
    }
    let dimensions = if kind == CharacteristicKind::Curve {
        vec![usize::try_from(axis_counts[0]).map_err(|_| CalibrationError::MatrixSizeOverflow)?]
    } else {
        vec![
            usize::try_from(axis_counts[1]).map_err(|_| CalibrationError::MatrixSizeOverflow)?,
            usize::try_from(axis_counts[0]).map_err(|_| CalibrationError::MatrixSizeOverflow)?,
        ]
    };
    let count = dimensions.iter().try_fold(1usize, |acc, &n| {
        acc.checked_mul(n)
            .ok_or(CalibrationError::MatrixSizeOverflow)
    })?;
    if count == 0 {
        return Err(CalibrationError::InvalidMatrixDimensions);
    }
    if count > crate::MAX_MATRIX_ELEMENTS {
        return Err(CalibrationError::MatrixTooLarge {
            count,
            max: crate::MAX_MATRIX_ELEMENTS,
        });
    }
    let counts = PointCounts::new()
        .with_fnc_values(u64::try_from(count).map_err(|_| CalibrationError::MatrixSizeOverflow)?)
        .with_axis(RecordAxis::X, axis_counts[0])
        .with_axis(RecordAxis::Y, *axis_counts.get(1).unwrap_or(&0));
    let resolved = plan.resolve(&counts)?;
    let placement = resolved.placement(ElementKind::FncValues).ok_or_else(|| {
        CalibrationError::UnsupportedA2lLayout {
            characteristic: characteristic.name.clone(),
            reason: "FNC_VALUES placement is not resolvable".into(),
        }
    })?;
    let data_type = primitive_type(characteristic, fnc.data_type)?;
    let total =
        usize::try_from(placement.size).map_err(|_| CalibrationError::MatrixSizeOverflow)?;
    let start = address
        .checked_add(placement.offset)
        .ok_or(CalibrationError::MatrixSizeOverflow)?;
    mapping.range_for(start, total)?;
    image.read_ecu_range(mapping, start, total)?;
    let byte_order = characteristic_byte_order(module, characteristic)?;
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        let offset = index
            .checked_mul(data_type.byte_len())
            .ok_or(CalibrationError::MatrixSizeOverflow)?;
        let ecu_address = start
            .checked_add(u64::try_from(offset).map_err(|_| CalibrationError::MatrixSizeOverflow)?)
            .ok_or(CalibrationError::MatrixSizeOverflow)?;
        let raw: RawValue = image
            .read_primitive(mapping, ecu_address, data_type, byte_order)?
            .into();
        let range = mapping.range_for(ecu_address, data_type.byte_len())?;
        values.push(CalibrationValue {
            raw,
            physical: PhysicalValue {
                value: raw.as_f64(),
                unit: unit.clone(),
            },
            provenance: CalibrationProvenance {
                ecu_address,
                bin_offset: range.offset,
                byte_length: range.length,
            },
        });
    }
    Ok(DecodedCharacteristic {
        name: characteristic.name.clone(),
        kind,
        axes,
        values,
        dimensions,
        layout: MatrixLayout::RowMajor,
        conversion: Conversion::Identical,
        conversion_reference,
        unit,
        record_layout: layout.name.clone(),
    })
}

fn decode_axes(
    module: &Module,
    characteristic: &Characteristic,
    image: &FirmwareImage,
    mapping: &EcuAddressMap,
) -> Result<Vec<DecodedAxis>, CalibrationError> {
    characteristic
        .axis_descriptions
        .iter()
        .enumerate()
        .map(|(index, descriptor)| match descriptor.attribute {
            Some(AxisAttribute::Std) => Ok(axis_from_binary(
                reconstruct_std_axis(module, characteristic, index, image, mapping)?,
                characteristic.axis_descriptions[index]
                    .conversion_reference
                    .as_deref(),
            )),
            Some(AxisAttribute::Com) => Ok(axis_from_binary(
                reconstruct_com_axis(module, characteristic, index, image, mapping)?,
                characteristic.axis_descriptions[index]
                    .conversion_reference
                    .as_deref(),
            )),
            Some(AxisAttribute::Fix) => axis_from_fixed(module, characteristic, descriptor),
            _ => Err(CalibrationError::UnsupportedA2lLayout {
                characteristic: characteristic.name.clone(),
                reason: "axis attribute is not STD_AXIS, COM_AXIS, or FIX_AXIS".into(),
            }),
        })
        .collect()
}

fn axis_from_binary(
    axis: crate::ReconstructedAxis,
    conversion_reference: Option<&str>,
) -> DecodedAxis {
    let values = axis
        .values
        .into_iter()
        .map(|value| AxisValue {
            raw: AxisRawValue::Binary(value.raw),
            physical: Some(value.physical.value),
            unit: value.physical.unit,
            conversion_reference: conversion_reference.map(str::to_owned),
            provenance: Some(value.provenance),
            source: AxisValueSource::Binary,
        })
        .collect();
    DecodedAxis {
        name: axis.name,
        source: AxisValueSource::Binary,
        values,
    }
}

fn axis_from_fixed(
    module: &Module,
    characteristic: &Characteristic,
    descriptor: &AxisDescription,
) -> Result<DecodedAxis, CalibrationError> {
    let method = descriptor
        .conversion_reference
        .as_deref()
        .filter(|v| *v != "NO_COMPU_METHOD")
        .map(|name| {
            module
                .compu_methods
                .iter()
                .find(|method| method.name == name)
                .ok_or_else(|| CalibrationError::MissingA2lMetadata {
                    characteristic: characteristic.name.clone(),
                    field: "axis conversion",
                })
        })
        .transpose()?;
    let result = calculate_fixed_axis(FixedAxisRequest {
        axis: descriptor,
        capacity: descriptor.max_axis_points,
        dimensions: None,
        conversion: method.map_or(FixedAxisConversion::Raw, FixedAxisConversion::Identical),
    })?;
    let values = result
        .values()
        .iter()
        .map(|value| AxisValue {
            raw: AxisRawValue::Generated(value.raw),
            physical: value.physical,
            unit: method.and_then(|m| m.unit.clone()),
            conversion_reference: descriptor
                .conversion_reference
                .as_deref()
                .filter(|value| *value != "NO_COMPU_METHOD")
                .map(str::to_owned),
            provenance: None,
            source: AxisValueSource::Generated,
        })
        .collect();
    Ok(DecodedAxis {
        name: characteristic.name.clone(),
        source: AxisValueSource::Generated,
        values,
    })
}

fn characteristic_conversion(
    module: &Module,
    characteristic: &Characteristic,
) -> Result<(Conversion, String, Option<String>), CalibrationError> {
    let name = characteristic
        .conversion_reference
        .as_deref()
        .filter(|v| *v != "NO_COMPU_METHOD")
        .ok_or_else(|| missing(characteristic, "conversion"))?;
    let method = module
        .compu_methods
        .iter()
        .find(|method| method.name == name)
        .ok_or_else(|| missing(characteristic, "conversion"))?;
    if method.conversion_type.as_deref() != Some("IDENTICAL") {
        return Err(CalibrationError::UnsupportedA2lConversion {
            characteristic: characteristic.name.clone(),
            conversion: name.into(),
        });
    }
    Ok((Conversion::Identical, name.into(), method.unit.clone()))
}

fn characteristic_byte_order(
    module: &Module,
    characteristic: &Characteristic,
) -> Result<ByteOrder, CalibrationError> {
    let value = characteristic
        .extra
        .iter()
        .find(|item| item.keyword == "BYTE_ORDER")
        .and_then(|item| item.values.first().map(String::as_str))
        .or_else(|| {
            module
                .mod_common
                .as_ref()
                .and_then(|common| common.byte_order.as_deref())
        })
        .ok_or_else(|| missing(characteristic, "byte order"))?;
    match value {
        "MSB_FIRST" => Ok(ByteOrder::BigEndian),
        "MSB_LAST" => Ok(ByteOrder::LittleEndian),
        other => Err(CalibrationError::UnsupportedA2lByteOrder {
            characteristic: characteristic.name.clone(),
            byte_order: other.into(),
        }),
    }
}

fn primitive_type(
    characteristic: &Characteristic,
    data_type: RecordDataType,
) -> Result<PrimitiveType, CalibrationError> {
    match data_type {
        RecordDataType::UByte => Ok(PrimitiveType::U8),
        RecordDataType::SByte => Ok(PrimitiveType::S8),
        RecordDataType::UWord => Ok(PrimitiveType::U16),
        RecordDataType::SWord => Ok(PrimitiveType::S16),
        RecordDataType::ULong => Ok(PrimitiveType::U32),
        RecordDataType::SLong => Ok(PrimitiveType::S32),
        other => Err(CalibrationError::UnsupportedA2lDataType {
            characteristic: characteristic.name.clone(),
            data_type: format!("{other:?}"),
        }),
    }
}

fn missing(characteristic: &Characteristic, field: &'static str) -> CalibrationError {
    CalibrationError::MissingA2lMetadata {
        characteristic: characteristic.name.clone(),
        field,
    }
}
