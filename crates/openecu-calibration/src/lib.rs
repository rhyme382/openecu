//! Calibration models have no ECU transport dependency.
use openecu_definitions::a2l::{Characteristic, Module, RecordLayout};
use openecu_firmware::{
    ByteOrder, EcuAddressMap, FirmwareError, FirmwareImage, PrimitiveType, PrimitiveValue,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
mod axis;
mod curve_map;
pub use axis::{
    AxisReconstructionError, ReconstructedAxis, reconstruct_com_axis, reconstruct_std_axis,
};
pub use curve_map::{
    AxisRawValue, AxisValue, AxisValueSource, CharacteristicKind, DecodedAxis,
    DecodedCharacteristic, decode_characteristic,
};
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
    #[error("matrix dimensions must be non-empty and non-zero")]
    InvalidMatrixDimensions,
    #[error("matrix value count overflows addressable memory")]
    MatrixSizeOverflow,
    #[error("A2L characteristic `{characteristic}` is missing {field}")]
    MissingA2lMetadata {
        characteristic: String,
        field: &'static str,
    },
    #[error("A2L characteristic `{characteristic}` uses unsupported conversion `{conversion}")]
    UnsupportedA2lConversion {
        characteristic: String,
        conversion: String,
    },
    #[error("A2L characteristic `{characteristic}` uses unsupported data type `{data_type}")]
    UnsupportedA2lDataType {
        characteristic: String,
        data_type: String,
    },
    #[error("A2L characteristic `{characteristic}` uses unsupported byte order `{byte_order}")]
    UnsupportedA2lByteOrder {
        characteristic: String,
        byte_order: String,
    },
    #[error("matrix has {count} elements, above the limit of {max}")]
    MatrixTooLarge { count: usize, max: usize },
    #[error(
        "A2L characteristic `{characteristic}` has unsupported type `{characteristic_type}` (axes cannot be reconstructed with certainty)"
    )]
    UnsupportedA2lCharacteristicType {
        characteristic: String,
        characteristic_type: String,
    },
    #[error("A2L characteristic `{characteristic}` has an unsupported layout: {reason}")]
    UnsupportedA2lLayout {
        characteristic: String,
        reason: String,
    },
    #[error(transparent)]
    Axis(#[from] AxisReconstructionError),
    #[error(transparent)]
    FixedAxis(#[from] openecu_definitions::fixed_axis::FixedAxisError),
    #[error(transparent)]
    RecordLayout(#[from] openecu_definitions::record_layout::AxisError),
    #[error(transparent)]
    Firmware(#[from] FirmwareError),
}
/// Upper bound on matrix elements, so a hostile definition cannot force a huge allocation.
pub const MAX_MATRIX_ELEMENTS: usize = 1 << 20;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conversion {
    Identical,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatrixLayout {
    RowMajor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RawValue {
    Unsigned(u32),
    Signed(i32),
}
impl From<PrimitiveValue> for RawValue {
    fn from(value: PrimitiveValue) -> Self {
        match value {
            PrimitiveValue::Unsigned(value) => Self::Unsigned(value),
            PrimitiveValue::Signed(value) => Self::Signed(value),
        }
    }
}
impl RawValue {
    const fn as_f64(self) -> f64 {
        match self {
            Self::Unsigned(value) => value as f64,
            Self::Signed(value) => value as f64,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicalValue {
    pub value: f64,
    pub unit: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrationProvenance {
    pub ecu_address: u64,
    pub bin_offset: usize,
    pub byte_length: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationValue {
    pub raw: RawValue,
    pub physical: PhysicalValue,
    pub provenance: CalibrationProvenance,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationMatrix {
    pub dimensions: Vec<usize>,
    pub layout: MatrixLayout,
    pub values: Vec<CalibrationValue>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrationDefinition {
    pub name: String,
    pub ecu_address: u64,
    pub data_type: PrimitiveType,
    pub byte_order: ByteOrder,
    pub conversion: Conversion,
    pub unit: Option<String>,
    pub dimensions: Vec<usize>,
    pub layout: Option<MatrixLayout>,
}
impl CalibrationDefinition {
    pub fn scalar(
        name: impl Into<String>,
        ecu_address: u64,
        data_type: PrimitiveType,
        byte_order: ByteOrder,
        conversion: Conversion,
        unit: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            ecu_address,
            data_type,
            byte_order,
            conversion,
            unit,
            dimensions: vec![],
            layout: None,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn matrix(
        name: impl Into<String>,
        ecu_address: u64,
        data_type: PrimitiveType,
        byte_order: ByteOrder,
        conversion: Conversion,
        unit: Option<String>,
        dimensions: Vec<usize>,
        layout: MatrixLayout,
    ) -> Result<Self, CalibrationError> {
        Self::element_count(&dimensions)?;
        Ok(Self {
            name: name.into(),
            ecu_address,
            data_type,
            byte_order,
            conversion,
            unit,
            dimensions,
            layout: Some(layout),
        })
    }
    pub fn read_scalar(
        &self,
        image: &FirmwareImage,
        mapping: &EcuAddressMap,
    ) -> Result<CalibrationValue, CalibrationError> {
        if !self.dimensions.is_empty() {
            return Err(CalibrationError::InvalidMatrixDimensions);
        }
        self.read_value(image, mapping, self.ecu_address)
    }
    pub fn read_matrix(
        &self,
        image: &FirmwareImage,
        mapping: &EcuAddressMap,
    ) -> Result<CalibrationMatrix, CalibrationError> {
        let layout = self
            .layout
            .ok_or(CalibrationError::InvalidMatrixDimensions)?;
        let count = Self::element_count(&self.dimensions)?;
        let element_len = self.data_type.byte_len();
        let stride =
            u64::try_from(element_len).map_err(|_| CalibrationError::MatrixSizeOverflow)?;
        // Validate the whole contiguous block (region + image bounds) before allocating.
        let total_len = count
            .checked_mul(element_len)
            .ok_or(CalibrationError::MatrixSizeOverflow)?;
        let range = mapping.range_for(self.ecu_address, total_len)?;
        image.read_range(range)?;
        let mut values = Vec::with_capacity(count);
        for index in 0..count {
            let index = u64::try_from(index).map_err(|_| CalibrationError::MatrixSizeOverflow)?;
            let offset = index
                .checked_mul(stride)
                .ok_or(CalibrationError::MatrixSizeOverflow)?;
            let address = self
                .ecu_address
                .checked_add(offset)
                .ok_or(CalibrationError::MatrixSizeOverflow)?;
            values.push(self.read_value(image, mapping, address)?);
        }
        Ok(CalibrationMatrix {
            dimensions: self.dimensions.clone(),
            layout,
            values,
        })
    }
    pub fn from_a2l(
        module: &Module,
        characteristic: &Characteristic,
    ) -> Result<Self, CalibrationError> {
        let name = &characteristic.name;
        let address = characteristic
            .address
            .ok_or_else(|| Self::missing(name, "ECU address"))?;
        let characteristic_type = characteristic
            .characteristic_type
            .as_deref()
            .ok_or_else(|| Self::missing(name, "characteristic type"))?;
        // Only VALUE and VAL_BLK carry no axes. CURVE/MAP/CUBOID/... need axis
        // layout positions that the importer does not keep, so axes are never guessed.
        if !matches!(characteristic_type, "VALUE" | "VAL_BLK")
            || !characteristic.axis_references.is_empty()
            || !characteristic.dimensions.is_empty()
        {
            return Err(CalibrationError::UnsupportedA2lCharacteristicType {
                characteristic: name.clone(),
                characteristic_type: characteristic_type.into(),
            });
        }
        let layout_name = characteristic
            .layout_reference
            .as_deref()
            .filter(|value| *value != "NO_RECORD_LAYOUT")
            .ok_or_else(|| Self::missing(name, "record layout"))?;
        let record_layout = module
            .record_layouts
            .iter()
            .find(|layout| layout.name == layout_name)
            .ok_or_else(|| Self::missing(name, "record layout"))?;
        // The record must hold nothing but FNC_VALUES, so the data starts at the
        // characteristic address and elements are packed.
        if let Some(other) = record_layout
            .extra
            .iter()
            .find(|item| item.keyword != "FNC_VALUES" && !item.keyword.starts_with("ALIGNMENT_"))
        {
            return Err(Self::unsupported_layout(
                name,
                format!("record layout item `{}` is not supported", other.keyword),
            ));
        }
        let mut fnc_values = record_layout
            .extra
            .iter()
            .filter(|item| item.keyword == "FNC_VALUES");
        let first = fnc_values
            .next()
            .ok_or_else(|| Self::missing(name, "FNC_VALUES data type"))?;
        if fnc_values.next().is_some() {
            return Err(Self::unsupported_layout(name, "duplicate FNC_VALUES"));
        }
        let fnc_values = first;
        // ASAP2: the first FNC_VALUES operand is the 1-based position of the item in
        // the record. With FNC_VALUES as the only data item, only position 1 proves
        // the data starts at offset 0 of the record (= the characteristic address).
        let position = fnc_values
            .values
            .first()
            .ok_or_else(|| Self::missing(name, "FNC_VALUES position"))?;
        if position.parse::<u64>().ok() != Some(1) {
            return Err(Self::unsupported_layout(
                name,
                format!(
                    "FNC_VALUES position `{position}` (only position 1, the first and only record item, proves data starts at the characteristic address)"
                ),
            ));
        }
        let index_mode = fnc_values
            .values
            .get(2)
            .ok_or_else(|| Self::missing(name, "FNC_VALUES index mode"))?;
        if index_mode != "ROW_DIR" {
            return Err(Self::unsupported_layout(
                name,
                format!("index mode `{index_mode}` (only ROW_DIR is supported)"),
            ));
        }
        let address_type = fnc_values
            .values
            .get(3)
            .ok_or_else(|| Self::missing(name, "FNC_VALUES address type"))?;
        if address_type != "DIRECT" {
            return Err(Self::unsupported_layout(
                name,
                format!("address type `{address_type}` (only DIRECT is supported)"),
            ));
        }
        let data_type = fnc_values
            .values
            .get(1)
            .ok_or_else(|| Self::missing(name, "FNC_VALUES data type"))?;
        let data_type = Self::a2l_data_type(name, data_type)?;
        Self::check_alignments(name, record_layout, data_type, address)?;
        let byte_order = characteristic
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
            .ok_or_else(|| Self::missing(name, "byte order"))?;
        let byte_order = Self::a2l_byte_order(name, byte_order)?;
        let conversion_name = characteristic
            .conversion_reference
            .as_deref()
            .filter(|value| *value != "NO_COMPU_METHOD")
            .ok_or_else(|| Self::missing(name, "conversion"))?;
        let conversion = module
            .compu_methods
            .iter()
            .find(|method| method.name == conversion_name)
            .ok_or_else(|| Self::missing(name, "conversion"))?;
        if conversion.conversion_type.as_deref() != Some("IDENTICAL") {
            return Err(CalibrationError::UnsupportedA2lConversion {
                characteristic: name.clone(),
                conversion: conversion_name.into(),
            });
        }
        let dimensions = Self::a2l_block_dimensions(characteristic)?;
        if characteristic_type == "VALUE" {
            if dimensions.is_some() {
                return Err(Self::unsupported_layout(
                    name,
                    "VALUE must not declare MATRIX_DIM or NUMBER",
                ));
            }
            Ok(Self::scalar(
                name,
                address,
                data_type,
                byte_order,
                Conversion::Identical,
                conversion.unit.clone(),
            ))
        } else {
            let dimensions =
                dimensions.ok_or_else(|| Self::missing(name, "MATRIX_DIM or NUMBER"))?;
            Self::matrix(
                name,
                address,
                data_type,
                byte_order,
                Conversion::Identical,
                conversion.unit.clone(),
                dimensions,
                // Justified by the explicit ROW_DIR index mode checked above.
                MatrixLayout::RowMajor,
            )
        }
    }
    /// `ALIGNMENT_*` only pad items inside a record. The record holds one item at
    /// offset 0 and array elements are packed, so alignment never moves an offset;
    /// it can only be violated when the record start is itself misaligned. Such
    /// records, malformed or duplicate directives are rejected, never ignored.
    fn check_alignments(
        characteristic: &str,
        layout: &RecordLayout,
        data_type: PrimitiveType,
        address: u64,
    ) -> Result<(), CalibrationError> {
        let mut seen: Vec<&str> = vec![];
        for item in layout
            .extra
            .iter()
            .filter(|item| item.keyword.starts_with("ALIGNMENT_"))
        {
            let applies_to_data = match item.keyword.as_str() {
                "ALIGNMENT_BYTE" => data_type.byte_len() == 1,
                "ALIGNMENT_WORD" => data_type.byte_len() == 2,
                "ALIGNMENT_LONG" => data_type.byte_len() == 4,
                "ALIGNMENT_INT64"
                | "ALIGNMENT_FLOAT16_IEEE"
                | "ALIGNMENT_FLOAT32_IEEE"
                | "ALIGNMENT_FLOAT64_IEEE" => false,
                other => {
                    return Err(Self::unsupported_layout(
                        characteristic,
                        format!("unknown alignment directive `{other}`"),
                    ));
                }
            };
            if seen.contains(&item.keyword.as_str()) {
                return Err(Self::unsupported_layout(
                    characteristic,
                    format!("duplicate `{}`", item.keyword),
                ));
            }
            seen.push(&item.keyword);
            let alignment = match item.values.as_slice() {
                [value] => value.parse::<u64>().ok().filter(|v| v.is_power_of_two()),
                _ => None,
            }
            .ok_or_else(|| {
                Self::unsupported_layout(
                    characteristic,
                    format!("`{}` needs one power-of-two byte count", item.keyword),
                )
            })?;
            if applies_to_data && !address.is_multiple_of(alignment) {
                return Err(Self::unsupported_layout(
                    characteristic,
                    format!(
                        "address {address:#x} violates `{} {alignment}`",
                        item.keyword
                    ),
                ));
            }
        }
        Ok(())
    }
    /// `VAL_BLK` size from `MATRIX_DIM` or `NUMBER`. Only one-dimensional blocks are
    /// accepted: the storage order of several dimensions is not certain.
    fn a2l_block_dimensions(
        characteristic: &Characteristic,
    ) -> Result<Option<Vec<usize>>, CalibrationError> {
        let name = &characteristic.name;
        let item = |keyword: &str| {
            characteristic
                .extra
                .iter()
                .find(|item| item.keyword == keyword)
        };
        let parse = |values: &[String]| -> Result<Vec<usize>, CalibrationError> {
            values
                .iter()
                .map(|value| {
                    value.parse::<usize>().map_err(|_| {
                        Self::unsupported_layout(name, format!("invalid dimension `{value}`"))
                    })
                })
                .collect()
        };
        let declared = match (item("MATRIX_DIM"), item("NUMBER")) {
            (None, None) => return Ok(None),
            (Some(_), Some(_)) => {
                return Err(Self::unsupported_layout(
                    name,
                    "both MATRIX_DIM and NUMBER are declared",
                ));
            }
            (Some(item), None) | (None, Some(item)) => parse(&item.values)?,
        };
        let mut non_unit = declared.iter().copied().filter(|dimension| *dimension != 1);
        match (non_unit.next(), non_unit.next(), declared.is_empty()) {
            (_, _, true) => Err(Self::missing(name, "MATRIX_DIM or NUMBER")),
            (None, _, _) => Ok(Some(vec![1])),
            (Some(count), None, _) => Ok(Some(vec![count])),
            _ => Err(Self::unsupported_layout(
                name,
                "multi-dimensional blocks (storage order is not certain)",
            )),
        }
    }
    fn read_value(
        &self,
        image: &FirmwareImage,
        mapping: &EcuAddressMap,
        ecu_address: u64,
    ) -> Result<CalibrationValue, CalibrationError> {
        let raw = image
            .read_primitive(mapping, ecu_address, self.data_type, self.byte_order)?
            .into();
        let range = mapping.range_for(ecu_address, self.data_type.byte_len())?;
        Ok(CalibrationValue {
            raw,
            physical: PhysicalValue {
                value: raw.as_f64(),
                unit: self.unit.clone(),
            },
            provenance: CalibrationProvenance {
                ecu_address,
                bin_offset: range.offset,
                byte_length: range.length,
            },
        })
    }
    fn element_count(dimensions: &[usize]) -> Result<usize, CalibrationError> {
        if dimensions.is_empty() || dimensions.contains(&0) {
            return Err(CalibrationError::InvalidMatrixDimensions);
        }
        let count = dimensions.iter().try_fold(1usize, |count, dimension| {
            count
                .checked_mul(*dimension)
                .ok_or(CalibrationError::MatrixSizeOverflow)
        })?;
        if count > MAX_MATRIX_ELEMENTS {
            return Err(CalibrationError::MatrixTooLarge {
                count,
                max: MAX_MATRIX_ELEMENTS,
            });
        }
        Ok(count)
    }
    fn unsupported_layout(characteristic: &str, reason: impl Into<String>) -> CalibrationError {
        CalibrationError::UnsupportedA2lLayout {
            characteristic: characteristic.into(),
            reason: reason.into(),
        }
    }
    fn missing(characteristic: &str, field: &'static str) -> CalibrationError {
        CalibrationError::MissingA2lMetadata {
            characteristic: characteristic.into(),
            field,
        }
    }
    fn a2l_data_type(
        characteristic: &str,
        data_type: &str,
    ) -> Result<PrimitiveType, CalibrationError> {
        match data_type {
            "UBYTE" => Ok(PrimitiveType::U8),
            "SBYTE" => Ok(PrimitiveType::S8),
            "UWORD" => Ok(PrimitiveType::U16),
            "SWORD" => Ok(PrimitiveType::S16),
            "ULONG" => Ok(PrimitiveType::U32),
            "SLONG" => Ok(PrimitiveType::S32),
            _ => Err(CalibrationError::UnsupportedA2lDataType {
                characteristic: characteristic.into(),
                data_type: data_type.into(),
            }),
        }
    }
    fn a2l_byte_order(
        characteristic: &str,
        byte_order: &str,
    ) -> Result<ByteOrder, CalibrationError> {
        match byte_order {
            "MSB_FIRST" => Ok(ByteOrder::BigEndian),
            "MSB_LAST" => Ok(ByteOrder::LittleEndian),
            _ => Err(CalibrationError::UnsupportedA2lByteOrder {
                characteristic: characteristic.into(),
                byte_order: byte_order.into(),
            }),
        }
    }
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
    use openecu_definitions::parse_a2l;
    use openecu_firmware::MemoryRegion;

    #[test]
    fn scalar_read_keeps_raw_physical_value_and_provenance_distinct() {
        let image = FirmwareImage::new("fixture", vec![0, 0x12, 0x34]);
        let mapping = EcuAddressMap::new(0x8000, 0, 3).unwrap();
        let definition = CalibrationDefinition::scalar(
            "torque_limit",
            0x8001,
            PrimitiveType::U16,
            ByteOrder::BigEndian,
            Conversion::Identical,
            Some("Nm".into()),
        );

        let value = definition.read_scalar(&image, &mapping).unwrap();

        assert_eq!(value.raw, RawValue::Unsigned(4_660));
        assert_eq!(value.physical.value, 4_660.0);
        assert_eq!(value.physical.unit.as_deref(), Some("Nm"));
        assert_eq!(
            value.provenance,
            CalibrationProvenance {
                ecu_address: 0x8001,
                bin_offset: 1,
                byte_length: 2,
            }
        );
    }

    #[test]
    fn matrix_read_declares_row_major_shape_and_preserves_each_source() {
        let image = FirmwareImage::new("fixture", vec![1, 2, 3, 4]);
        let mapping = EcuAddressMap::new(0x4000, 0, 4).unwrap();
        let definition = CalibrationDefinition::matrix(
            "spark",
            0x4000,
            PrimitiveType::U8,
            ByteOrder::BigEndian,
            Conversion::Identical,
            None,
            vec![2, 2],
            MatrixLayout::RowMajor,
        )
        .unwrap();

        let matrix = definition.read_matrix(&image, &mapping).unwrap();

        assert_eq!(matrix.dimensions, vec![2, 2]);
        assert_eq!(matrix.layout, MatrixLayout::RowMajor);
        assert_eq!(
            matrix
                .values
                .iter()
                .map(|value| &value.raw)
                .collect::<Vec<_>>(),
            vec![
                &RawValue::Unsigned(1),
                &RawValue::Unsigned(2),
                &RawValue::Unsigned(3),
                &RawValue::Unsigned(4),
            ]
        );
        assert_eq!(matrix.values[3].provenance.bin_offset, 3);
    }

    #[test]
    fn imports_only_complete_identical_a2l_metadata() {
        let document = parse_a2l(
            r#"/begin PROJECT p "project"
              /begin MODULE m "module"
                /begin MOD_COMMON "common"
                  BYTE_ORDER MSB_FIRST
                /end MOD_COMMON
                /begin RECORD_LAYOUT rl
                  FNC_VALUES 1 UWORD ROW_DIR DIRECT
                /end RECORD_LAYOUT
                /begin COMPU_METHOD identity "identity" IDENTICAL "%6.2" "Nm"
                /end COMPU_METHOD
                /begin CHARACTERISTIC torque "torque" VALUE 0x8000 rl 0 identity 0 500
                /end CHARACTERISTIC
              /end MODULE
            /end PROJECT"#,
        )
        .unwrap();
        let module = &document.project.modules[0];

        let definition =
            CalibrationDefinition::from_a2l(module, &module.characteristics[0]).unwrap();

        assert_eq!(definition.data_type, PrimitiveType::U16);
        assert_eq!(definition.byte_order, ByteOrder::BigEndian);
        assert_eq!(definition.conversion, Conversion::Identical);
        assert_eq!(definition.unit.as_deref(), Some("Nm"));
    }

    #[test]
    fn rejects_a2l_without_a_record_layout() {
        let document = parse_a2l(
            r#"/begin PROJECT p "project"
              /begin MODULE m "module"
                /begin COMPU_METHOD scaled "scaled" LINEAR "%6.2" "Nm"
                /end COMPU_METHOD
                /begin CHARACTERISTIC no_layout "no layout" VALUE 0x8000 NO_RECORD_LAYOUT 0 scaled 0 500
                /end CHARACTERISTIC
              /end MODULE
            /end PROJECT"#,
        )
        .unwrap();
        let module = &document.project.modules[0];

        assert_eq!(
            CalibrationDefinition::from_a2l(module, &module.characteristics[0]),
            Err(CalibrationError::MissingA2lMetadata {
                characteristic: "no_layout".into(),
                field: "record layout",
            })
        );
    }

    #[test]
    fn reports_non_identical_a2l_conversion_as_unsupported() {
        let document = parse_a2l(
            r#"/begin PROJECT p "project"
              /begin MODULE m "module"
                /begin MOD_COMMON "common"
                  BYTE_ORDER MSB_FIRST
                /end MOD_COMMON
                /begin RECORD_LAYOUT rl
                  FNC_VALUES 1 UWORD ROW_DIR DIRECT
                /end RECORD_LAYOUT
                /begin COMPU_METHOD scaled "scaled" LINEAR "%6.2" "Nm"
                /end COMPU_METHOD
                /begin CHARACTERISTIC torque "torque" VALUE 0x8000 rl 0 scaled 0 500
                /end CHARACTERISTIC
              /end MODULE
            /end PROJECT"#,
        )
        .unwrap();
        let module = &document.project.modules[0];

        assert_eq!(
            CalibrationDefinition::from_a2l(module, &module.characteristics[0]),
            Err(CalibrationError::UnsupportedA2lConversion {
                characteristic: "torque".into(),
                conversion: "scaled".into(),
            })
        );
    }
    fn u8_matrix(dimensions: Vec<usize>, address: u64) -> CalibrationDefinition {
        CalibrationDefinition::matrix(
            "m",
            address,
            PrimitiveType::U8,
            ByteOrder::BigEndian,
            Conversion::Identical,
            None,
            dimensions,
            MatrixLayout::RowMajor,
        )
        .unwrap()
    }

    #[test]
    fn matrix_read_outside_the_image_fails_before_allocating() {
        let image = FirmwareImage::new("fixture", vec![0; 4]);
        let mapping = EcuAddressMap::new(0x4000, 0, 4).unwrap();
        let definition = u8_matrix(vec![1024, 1024], 0x4000);

        assert_eq!(
            definition.read_matrix(&image, &mapping),
            Err(CalibrationError::Firmware(
                FirmwareError::RangeCrossesRegion
            ))
        );
        let short_image = FirmwareImage::new("fixture", vec![0; 2]);
        assert_eq!(
            u8_matrix(vec![4], 0x4000).read_matrix(&short_image, &mapping),
            Err(CalibrationError::Firmware(FirmwareError::OutOfBounds))
        );
    }

    #[test]
    fn matrix_definition_rejects_oversized_or_overflowing_dimensions() {
        let build = |dimensions: Vec<usize>| {
            CalibrationDefinition::matrix(
                "m",
                0,
                PrimitiveType::U32,
                ByteOrder::BigEndian,
                Conversion::Identical,
                None,
                dimensions,
                MatrixLayout::RowMajor,
            )
        };
        assert_eq!(
            build(vec![MAX_MATRIX_ELEMENTS + 1]).unwrap_err(),
            CalibrationError::MatrixTooLarge {
                count: MAX_MATRIX_ELEMENTS + 1,
                max: MAX_MATRIX_ELEMENTS
            }
        );
        assert_eq!(
            build(vec![usize::MAX, 2]).unwrap_err(),
            CalibrationError::MatrixSizeOverflow
        );
        assert_eq!(
            build(vec![2, 0]).unwrap_err(),
            CalibrationError::InvalidMatrixDimensions
        );
        assert!(build(vec![MAX_MATRIX_ELEMENTS]).is_ok());
    }

    #[test]
    fn matrix_must_not_straddle_two_regions() {
        let image = FirmwareImage::new("fixture", vec![1, 2, 3, 4]);
        let mapping = EcuAddressMap::from_regions(vec![
            MemoryRegion::new(0x100, 2, 0).unwrap(),
            MemoryRegion::new(0x102, 2, 2).unwrap(),
        ])
        .unwrap();

        assert_eq!(
            u8_matrix(vec![4], 0x100).read_matrix(&image, &mapping),
            Err(CalibrationError::Firmware(
                FirmwareError::RangeCrossesRegion
            ))
        );
        let value = u8_matrix(vec![2], 0x102)
            .read_matrix(&image, &mapping)
            .unwrap();
        assert_eq!(value.values[0].provenance.bin_offset, 2);
        assert_eq!(value.values[1].raw, RawValue::Unsigned(4));
    }

    fn a2l_module(layout_items: &str, characteristic: &str) -> String {
        format!(
            r#"/begin PROJECT p "project"
              /begin MODULE m "module"
                /begin MOD_COMMON "common"
                  BYTE_ORDER MSB_FIRST
                /end MOD_COMMON
                /begin RECORD_LAYOUT rl
                  {layout_items}
                /end RECORD_LAYOUT
                /begin COMPU_METHOD identity "identity" IDENTICAL "%6.2" "Nm"
                /end COMPU_METHOD
                {characteristic}
              /end MODULE
            /end PROJECT"#
        )
    }

    fn import(source: &str) -> Result<CalibrationDefinition, CalibrationError> {
        let document = parse_a2l(source).unwrap();
        let module = &document.project.modules[0];
        CalibrationDefinition::from_a2l(module, &module.characteristics[0])
    }

    const BLK: &str = r#"/begin CHARACTERISTIC blk "block" VAL_BLK 0x8000 rl 0 identity 0 500
                  MATRIX_DIM 4
                /end CHARACTERISTIC"#;

    #[test]
    fn imports_a_one_dimensional_row_dir_value_block() {
        let definition = import(&a2l_module("FNC_VALUES 1 UBYTE ROW_DIR DIRECT", BLK)).unwrap();
        assert_eq!(definition.dimensions, vec![4]);
        assert_eq!(definition.layout, Some(MatrixLayout::RowMajor));
        let number = BLK.replace("MATRIX_DIM 4", "NUMBER 4");
        let definition = import(&a2l_module("FNC_VALUES 1 UBYTE ROW_DIR DIRECT", &number)).unwrap();
        assert_eq!(definition.dimensions, vec![4]);
    }

    #[test]
    fn rejects_unsupported_a2l_index_modes_and_address_types() {
        for (items, fragment) in [
            ("FNC_VALUES 1 UBYTE COLUMN_DIR DIRECT", "COLUMN_DIR"),
            (
                "FNC_VALUES 1 UBYTE ALTERNATE_WITH_X DIRECT",
                "ALTERNATE_WITH_X",
            ),
            ("FNC_VALUES 1 UBYTE ROW_DIR PBYTE", "PBYTE"),
        ] {
            match import(&a2l_module(items, BLK)) {
                Err(CalibrationError::UnsupportedA2lLayout { reason, .. }) => {
                    assert!(reason.contains(fragment), "{reason}")
                }
                other => panic!("expected unsupported layout, got {other:?}"),
            }
        }
        assert_eq!(
            import(&a2l_module("FNC_VALUES 1 UBYTE", BLK)),
            Err(CalibrationError::MissingA2lMetadata {
                characteristic: "blk".into(),
                field: "FNC_VALUES index mode"
            })
        );
    }

    #[test]
    fn rejects_record_layouts_with_extra_items_and_multi_dimensional_blocks() {
        assert!(matches!(
            import(&a2l_module(
                "NO_AXIS_PTS_X 1 UBYTE\n FNC_VALUES 2 UBYTE ROW_DIR DIRECT",
                BLK
            )),
            Err(CalibrationError::UnsupportedA2lLayout { .. })
        ));
        let matrix = BLK.replace("MATRIX_DIM 4", "MATRIX_DIM 2 2");
        assert!(matches!(
            import(&a2l_module("FNC_VALUES 1 UBYTE ROW_DIR DIRECT", &matrix)),
            Err(CalibrationError::UnsupportedA2lLayout { .. })
        ));
        let missing = BLK.replace("MATRIX_DIM 4", "");
        assert_eq!(
            import(&a2l_module("FNC_VALUES 1 UBYTE ROW_DIR DIRECT", &missing)),
            Err(CalibrationError::MissingA2lMetadata {
                characteristic: "blk".into(),
                field: "MATRIX_DIM or NUMBER"
            })
        );
    }

    fn assert_unsupported_layout(
        result: Result<CalibrationDefinition, CalibrationError>,
        fragment: &str,
    ) {
        match result {
            Err(CalibrationError::UnsupportedA2lLayout { reason, .. }) => {
                assert!(reason.contains(fragment), "{reason}")
            }
            other => panic!("expected unsupported layout `{fragment}`, got {other:?}"),
        }
    }

    #[test]
    fn rejects_fnc_values_position_other_than_one() {
        for position in ["0", "2", "3", "-1", "x"] {
            let items = format!("FNC_VALUES {position} UBYTE ROW_DIR DIRECT");
            assert_unsupported_layout(import(&a2l_module(&items, BLK)), "FNC_VALUES position");
        }
        assert_eq!(
            import(&a2l_module("FNC_VALUES", BLK)),
            Err(CalibrationError::MissingA2lMetadata {
                characteristic: "blk".into(),
                field: "FNC_VALUES position"
            })
        );
        assert!(import(&a2l_module("FNC_VALUES 1 UBYTE ROW_DIR DIRECT", BLK)).is_ok());
    }

    #[test]
    fn accepts_alignment_directives_when_the_address_honours_them() {
        let layout = "ALIGNMENT_BYTE 1\n ALIGNMENT_WORD 2\n ALIGNMENT_LONG 4\n ALIGNMENT_FLOAT32_IEEE 4\n FNC_VALUES 1 UWORD ROW_DIR DIRECT";
        let definition = import(&a2l_module(layout, BLK)).unwrap();
        // Alignment never pads inside a packed array: dimensions stay as declared.
        assert_eq!(definition.dimensions, vec![4]);
        // Alignment above the data type size is harmless at an aligned address.
        let wide = "ALIGNMENT_WORD 4\n FNC_VALUES 1 UWORD ROW_DIR DIRECT";
        assert!(import(&a2l_module(wide, BLK)).is_ok());
    }

    #[test]
    fn rejects_alignment_that_the_address_violates_or_that_is_malformed() {
        let odd = BLK.replace("0x8000", "0x8001");
        assert_unsupported_layout(
            import(&a2l_module(
                "ALIGNMENT_WORD 2\n FNC_VALUES 1 UWORD ROW_DIR DIRECT",
                &odd,
            )),
            "violates",
        );
        for (items, fragment) in [
            (
                "ALIGNMENT_WORD 3\n FNC_VALUES 1 UBYTE ROW_DIR DIRECT",
                "power-of-two",
            ),
            (
                "ALIGNMENT_WORD 0\n FNC_VALUES 1 UBYTE ROW_DIR DIRECT",
                "power-of-two",
            ),
            (
                "ALIGNMENT_WORD\n FNC_VALUES 1 UBYTE ROW_DIR DIRECT",
                "power-of-two",
            ),
            (
                "ALIGNMENT_WORD 2\n ALIGNMENT_WORD 2\n FNC_VALUES 1 UBYTE ROW_DIR DIRECT",
                "duplicate",
            ),
            (
                "ALIGNMENT_FOO 2\n FNC_VALUES 1 UBYTE ROW_DIR DIRECT",
                "unknown alignment",
            ),
        ] {
            assert_unsupported_layout(import(&a2l_module(items, BLK)), fragment);
        }
    }

    #[test]
    fn refuses_a2l_curves_and_maps_instead_of_guessing_axes() {
        let map = r#"/begin CHARACTERISTIC map "map" MAP 0x8000 rl 0 identity 0 500
                  /begin AXIS_DESCR STD_AXIS NO_INPUT_QUANTITY identity 4 0 1
                    MAX_AXIS_PTS 4
                  /end AXIS_DESCR
                /end CHARACTERISTIC"#;
        assert!(matches!(
            import(&a2l_module("FNC_VALUES 1 UBYTE ROW_DIR DIRECT", map)),
            Err(CalibrationError::UnsupportedA2lCharacteristicType { characteristic_type, .. })
                if characteristic_type == "MAP"
        ));
    }

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
