//! Conservative reconstruction of `STD_AXIS` and `COM_AXIS` point records.
//!
//! This module only reads axis-point storage described by a validated
//! [`RecordPlan`]. It deliberately does not decode `CURVE`, `MAP`, or any
//! other characteristic payload.

use crate::{CalibrationProvenance, CalibrationValue, PhysicalValue, RawValue};
use openecu_definitions::a2l::{
    AxisAttribute, AxisDescription, AxisPoints, Characteristic, Module,
};
use openecu_definitions::record_layout::{
    AxisError, ElementKind, PointCounts, RecordAxis, RecordDataType, RecordPlan,
};
use openecu_firmware::{ByteOrder, EcuAddressMap, FirmwareError, FirmwareImage, PrimitiveType};
use thiserror::Error;

const MAX_AXIS_POINTS: u64 = 1 << 20;

#[derive(Debug, Clone, PartialEq)]
pub struct ReconstructedAxis {
    pub name: String,
    /// `STD_AXIS` or the resolved `AXIS_PTS` name for `COM_AXIS`.
    pub source: String,
    pub values: Vec<CalibrationValue>,
    pub capacity: u32,
}

#[derive(Debug, Error, PartialEq)]
pub enum AxisReconstructionError {
    #[error("axis index {index} is out of range")]
    AxisIndexOutOfRange { index: usize },
    #[error("axis `{axis}` is missing {field}")]
    MissingMetadata { axis: String, field: &'static str },
    #[error("axis `{axis}` has unsupported attribute `{attribute}`")]
    UnsupportedAxisAttribute { axis: String, attribute: String },
    #[error("axis `{axis}` uses ambiguous AXIS_PTS reference `{reference}`")]
    AmbiguousReference { axis: String, reference: String },
    #[error("axis `{axis}` references missing AXIS_PTS `{reference}`")]
    MissingReference { axis: String, reference: String },
    #[error("axis `{axis}` uses unsupported conversion `{conversion}`")]
    UnsupportedConversion { axis: String, conversion: String },
    #[error("axis `{axis}` uses unsupported data type `{data_type:?}`")]
    UnsupportedDataType {
        axis: String,
        data_type: RecordDataType,
    },
    #[error("axis `{axis}` uses unsupported byte order `{byte_order}`")]
    UnsupportedByteOrder { axis: String, byte_order: String },
    #[error("axis `{axis}` has invalid point count {count}")]
    InvalidPointCount { axis: String, count: u64 },
    #[error("axis `{axis}` point count {count} exceeds capacity {capacity}")]
    CapacityExceeded {
        axis: String,
        count: u64,
        capacity: u32,
    },
    #[error("axis `{axis}` point count {count} exceeds limit {maximum}")]
    TooManyPoints {
        axis: String,
        count: u64,
        maximum: u64,
    },
    #[error("axis `{axis}` has ambiguous metadata: {reason}")]
    AmbiguousMetadata { axis: String, reason: String },
    #[error(transparent)]
    Layout(#[from] AxisError),
    #[error(transparent)]
    Firmware(#[from] FirmwareError),
}

pub fn reconstruct_std_axis(
    module: &Module,
    characteristic: &Characteristic,
    axis_index: usize,
    image: &FirmwareImage,
    mapping: &EcuAddressMap,
) -> Result<ReconstructedAxis, AxisReconstructionError> {
    let axis = characteristic
        .axis_descriptions
        .get(axis_index)
        .ok_or(AxisReconstructionError::AxisIndexOutOfRange { index: axis_index })?;
    let name = characteristic.name.clone();
    if axis.attribute != Some(AxisAttribute::Std) {
        return Err(AxisReconstructionError::UnsupportedAxisAttribute {
            axis: name,
            attribute: attribute_name(axis.attribute.as_ref()),
        });
    }
    let address = characteristic
        .address
        .ok_or_else(|| missing(&name, "ECU address"))?;
    let layout_name = characteristic
        .layout_reference
        .as_deref()
        .filter(|name| *name != "NO_RECORD_LAYOUT")
        .ok_or_else(|| missing(&name, "record layout"))?;
    let layout = find_layout(module, layout_name, &name)?;
    let conversion = conversion(module, axis.conversion_reference.as_deref(), &name)?;
    let byte_order = byte_order(module, &axis.extra, &name)?;
    read_axis(
        module,
        layout,
        axis,
        &name,
        "STD_AXIS".into(),
        address,
        conversion,
        byte_order,
        axis_index,
        image,
        mapping,
    )
}

pub fn reconstruct_com_axis(
    module: &Module,
    characteristic: &Characteristic,
    axis_index: usize,
    image: &FirmwareImage,
    mapping: &EcuAddressMap,
) -> Result<ReconstructedAxis, AxisReconstructionError> {
    let descriptor = characteristic
        .axis_descriptions
        .get(axis_index)
        .ok_or(AxisReconstructionError::AxisIndexOutOfRange { index: axis_index })?;
    let name = characteristic.name.clone();
    if descriptor.attribute != Some(AxisAttribute::Com) {
        return Err(AxisReconstructionError::UnsupportedAxisAttribute {
            axis: name,
            attribute: attribute_name(descriptor.attribute.as_ref()),
        });
    }
    let reference = descriptor
        .axis_pts_ref
        .as_deref()
        .ok_or_else(|| missing(&name, "AXIS_PTS_REF"))?;
    let matches: Vec<&AxisPoints> = module
        .axis_points
        .iter()
        .filter(|a| a.name == reference)
        .collect();
    let shared = match matches.as_slice() {
        [axis] => *axis,
        [] => {
            return Err(AxisReconstructionError::MissingReference {
                axis: name,
                reference: reference.into(),
            });
        }
        _ => {
            return Err(AxisReconstructionError::AmbiguousReference {
                axis: name,
                reference: reference.into(),
            });
        }
    };
    let address = shared
        .address
        .ok_or_else(|| missing(reference, "ECU address"))?;
    let layout_name = shared
        .layout_reference
        .as_deref()
        .filter(|name| *name != "NO_RECORD_LAYOUT")
        .ok_or_else(|| missing(reference, "record layout"))?;
    let layout = find_layout(module, layout_name, reference)?;
    let conversion = conversion(module, shared.conversion_reference.as_deref(), reference)?;
    if descriptor.conversion_reference.as_deref() != shared.conversion_reference.as_deref() {
        return Err(AxisReconstructionError::AmbiguousMetadata {
            axis: name,
            reason: "AXIS_DESCR and AXIS_PTS use different conversions".into(),
        });
    }
    let byte_order = byte_order(module, &shared.extra, reference)?;
    read_axis(
        module,
        layout,
        descriptor,
        reference,
        reference.into(),
        address,
        conversion,
        byte_order,
        axis_index,
        image,
        mapping,
    )
}

#[allow(clippy::too_many_arguments)]
fn read_axis(
    module: &Module,
    layout: &openecu_definitions::a2l::RecordLayout,
    descriptor: &AxisDescription,
    name: &str,
    source: String,
    address: u64,
    conversion_unit: Option<String>,
    byte_order: ByteOrder,
    axis_index: usize,
    image: &FirmwareImage,
    mapping: &EcuAddressMap,
) -> Result<ReconstructedAxis, AxisReconstructionError> {
    let record_axis = record_axis(axis_index)?;
    let plan = RecordPlan::from_layout_with_mod_common(layout, module.mod_common.as_ref())?;
    plan.check_base_address(address)?;
    let count = point_count(
        &plan,
        record_axis,
        address,
        name,
        byte_order,
        image,
        mapping,
    )?;
    let capacity = descriptor
        .max_axis_points
        .ok_or_else(|| missing(name, "maximum axis points"))?;
    if count > u64::from(capacity) {
        return Err(AxisReconstructionError::CapacityExceeded {
            axis: name.into(),
            count,
            capacity,
        });
    }
    if count == 0 {
        return Err(AxisReconstructionError::InvalidPointCount {
            axis: name.into(),
            count,
        });
    }
    if count > MAX_AXIS_POINTS {
        return Err(AxisReconstructionError::TooManyPoints {
            axis: name.into(),
            count,
            maximum: MAX_AXIS_POINTS,
        });
    }
    let resolved = plan.resolve(&PointCounts::new().with_axis(record_axis, count))?;
    let placement = resolved
        .placement(ElementKind::AxisPts(record_axis))
        .ok_or_else(|| missing(name, "AXIS_PTS record element"))?;
    let element = plan
        .element(ElementKind::AxisPts(record_axis))
        .ok_or_else(|| missing(name, "AXIS_PTS data type"))?;
    let primitive = primitive_type(name, element.data_type)?;
    let total =
        usize::try_from(placement.size).map_err(|_| AxisReconstructionError::TooManyPoints {
            axis: name.into(),
            count,
            maximum: MAX_AXIS_POINTS,
        })?;
    let start = address
        .checked_add(placement.offset)
        .ok_or(AxisError::Overflow {
            operation: "axis address",
        })?;
    mapping.range_for(start, total)?;
    let mut values = Vec::with_capacity(count as usize);
    let unit = descriptor.phys_unit.clone().or(conversion_unit);
    for index in 0..count {
        let offset = index
            .checked_mul(u64::from(primitive.byte_len() as u32))
            .ok_or(AxisError::Overflow {
                operation: "axis element offset",
            })?;
        let ecu_address = start.checked_add(offset).ok_or(AxisError::Overflow {
            operation: "axis address",
        })?;
        let raw: RawValue = image
            .read_primitive(mapping, ecu_address, primitive, byte_order)?
            .into();
        let range = mapping.range_for(ecu_address, primitive.byte_len())?;
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
    Ok(ReconstructedAxis {
        name: name.into(),
        source,
        values,
        capacity,
    })
}

fn point_count(
    plan: &RecordPlan,
    axis: RecordAxis,
    address: u64,
    name: &str,
    byte_order: ByteOrder,
    image: &FirmwareImage,
    mapping: &EcuAddressMap,
) -> Result<u64, AxisReconstructionError> {
    if let Some(fixed) = plan.fixed_axis_points(axis) {
        return Ok(u64::from(fixed));
    }
    let element = plan
        .element(ElementKind::NoAxisPts(axis))
        .ok_or_else(|| missing(name, "NO_AXIS_PTS"))?;
    let offset =
        element
            .static_offset
            .ok_or_else(|| AxisReconstructionError::AmbiguousMetadata {
                axis: name.into(),
                reason: "NO_AXIS_PTS is after a variable-size element".into(),
            })?;
    let primitive = primitive_type(name, element.data_type)?;
    let ecu_address = address.checked_add(offset).ok_or(AxisError::Overflow {
        operation: "axis count address",
    })?;
    let value = image.read_primitive(mapping, ecu_address, primitive, byte_order)?;
    let count = match value {
        openecu_firmware::PrimitiveValue::Unsigned(value) => u64::from(value),
        openecu_firmware::PrimitiveValue::Signed(value) if value >= 0 => value as u64,
        openecu_firmware::PrimitiveValue::Signed(_) => {
            return Err(AxisReconstructionError::InvalidPointCount {
                axis: name.into(),
                count: 0,
            });
        }
    };
    Ok(count)
}

fn find_layout<'a>(
    module: &'a Module,
    name: &str,
    axis: &str,
) -> Result<&'a openecu_definitions::a2l::RecordLayout, AxisReconstructionError> {
    let layouts: Vec<_> = module
        .record_layouts
        .iter()
        .filter(|layout| layout.name == name)
        .collect();
    match layouts.as_slice() {
        [layout] => Ok(layout),
        [] => Err(missing(axis, "record layout")),
        _ => Err(AxisReconstructionError::AmbiguousMetadata {
            axis: axis.into(),
            reason: format!("duplicate record layout `{name}`"),
        }),
    }
}

fn conversion(
    module: &Module,
    reference: Option<&str>,
    axis: &str,
) -> Result<Option<String>, AxisReconstructionError> {
    let reference = reference
        .filter(|value| *value != "NO_COMPU_METHOD")
        .ok_or_else(|| missing(axis, "conversion"))?;
    let methods: Vec<_> = module
        .compu_methods
        .iter()
        .filter(|method| method.name == reference)
        .collect();
    let method = match methods.as_slice() {
        [method] => *method,
        [] => return Err(missing(axis, "conversion")),
        _ => {
            return Err(AxisReconstructionError::AmbiguousMetadata {
                axis: axis.into(),
                reason: format!("duplicate conversion `{reference}`"),
            });
        }
    };
    if method.conversion_type.as_deref() != Some("IDENTICAL") {
        return Err(AxisReconstructionError::UnsupportedConversion {
            axis: axis.into(),
            conversion: reference.into(),
        });
    }
    Ok(method.unit.clone())
}

fn byte_order(
    module: &Module,
    extra: &[openecu_definitions::a2l::RawItem],
    axis: &str,
) -> Result<ByteOrder, AxisReconstructionError> {
    let items: Vec<_> = extra
        .iter()
        .filter(|item| item.keyword == "BYTE_ORDER")
        .collect();
    let value = match items.as_slice() {
        [] => module
            .mod_common
            .as_ref()
            .and_then(|common| common.byte_order.as_deref()),
        [item] => item.values.first().map(String::as_str),
        _ => {
            return Err(AxisReconstructionError::AmbiguousMetadata {
                axis: axis.into(),
                reason: "duplicate BYTE_ORDER".into(),
            });
        }
    }
    .ok_or_else(|| missing(axis, "byte order"))?;
    match value {
        "MSB_FIRST" => Ok(ByteOrder::BigEndian),
        "MSB_LAST" => Ok(ByteOrder::LittleEndian),
        other => Err(AxisReconstructionError::UnsupportedByteOrder {
            axis: axis.into(),
            byte_order: other.into(),
        }),
    }
}

fn primitive_type(
    axis: &str,
    data_type: RecordDataType,
) -> Result<PrimitiveType, AxisReconstructionError> {
    match data_type {
        RecordDataType::UByte => Ok(PrimitiveType::U8),
        RecordDataType::SByte => Ok(PrimitiveType::S8),
        RecordDataType::UWord => Ok(PrimitiveType::U16),
        RecordDataType::SWord => Ok(PrimitiveType::S16),
        RecordDataType::ULong => Ok(PrimitiveType::U32),
        RecordDataType::SLong => Ok(PrimitiveType::S32),
        other => Err(AxisReconstructionError::UnsupportedDataType {
            axis: axis.into(),
            data_type: other,
        }),
    }
}

fn record_axis(index: usize) -> Result<RecordAxis, AxisReconstructionError> {
    match index {
        0 => Ok(RecordAxis::X),
        1 => Ok(RecordAxis::Y),
        2 => Ok(RecordAxis::Z),
        3 => Ok(RecordAxis::Fourth),
        4 => Ok(RecordAxis::Fifth),
        _ => Err(AxisReconstructionError::AxisIndexOutOfRange { index }),
    }
}

fn attribute_name(attribute: Option<&AxisAttribute>) -> String {
    match attribute {
        Some(AxisAttribute::Std) => "STD_AXIS",
        Some(AxisAttribute::Fix) => "FIX_AXIS",
        Some(AxisAttribute::Com) => "COM_AXIS",
        Some(AxisAttribute::Res) => "RES_AXIS",
        Some(AxisAttribute::Curve) => "CURVE_AXIS",
        Some(AxisAttribute::Unknown(value)) => value,
        None => "missing",
    }
    .into()
}
fn missing(axis: &str, field: &'static str) -> AxisReconstructionError {
    AxisReconstructionError::MissingMetadata {
        axis: axis.into(),
        field,
    }
}
