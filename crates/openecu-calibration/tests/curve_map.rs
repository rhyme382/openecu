use openecu_calibration::{
    AxisReconstructionError, AxisValueSource, CalibrationError, CharacteristicKind,
    decode_characteristic,
};
use openecu_definitions::parse_a2l;
use openecu_firmware::{EcuAddressMap, FirmwareImage};

const A2L: &str = r#"
/begin PROJECT p "p"
  /begin MODULE m "m"
    /begin MOD_COMMON "common"
      BYTE_ORDER MSB_FIRST
    /end MOD_COMMON
    /begin RECORD_LAYOUT curve_rl
      NO_AXIS_PTS_X 1 UWORD
      AXIS_PTS_X 2 UWORD INDEX_INCR DIRECT
      FNC_VALUES 3 SWORD ROW_DIR DIRECT
      ALIGNMENT_WORD 2
    /end RECORD_LAYOUT
    /begin RECORD_LAYOUT map_rl
      NO_AXIS_PTS_X 1 UWORD
      AXIS_PTS_X 2 UWORD INDEX_INCR DIRECT
      NO_AXIS_PTS_Y 3 UWORD
      AXIS_PTS_Y 4 UWORD INDEX_INCR DIRECT
      FNC_VALUES 5 UWORD ROW_DIR DIRECT
      ALIGNMENT_WORD 2
    /end RECORD_LAYOUT
    /begin COMPU_METHOD ident "ident" IDENTICAL "%6.2" "rpm"
    /end COMPU_METHOD
    /begin CHARACTERISTIC curve "curve" CURVE 0x1000 curve_rl 0 ident 0 1000
      /begin AXIS_DESCR STD_AXIS x ident 3 0 1000
      /end AXIS_DESCR
    /end CHARACTERISTIC
    /begin CHARACTERISTIC map "map" MAP 0x1100 map_rl 0 ident 0 1000
      /begin AXIS_DESCR STD_AXIS x ident 3 0 1000
      /end AXIS_DESCR
      /begin AXIS_DESCR STD_AXIS y ident 2 0 1000
      /end AXIS_DESCR
    /end CHARACTERISTIC
  /end MODULE
/end PROJECT
"#;

fn module() -> openecu_definitions::a2l::Module {
    parse_a2l(A2L).unwrap().project.modules[0].clone()
}

#[test]
fn decodes_curve_and_preserves_x_to_value_correspondence() {
    let module = module();
    let image = FirmwareImage::new(
        "curve",
        vec![0, 3, 0, 10, 0, 20, 0, 30, 0xff, 0xec, 0, 40, 0, 50],
    );
    let mapping = EcuAddressMap::new(0x1000, 0, 14).unwrap();
    let result =
        decode_characteristic(&module, &module.characteristics[0], &image, &mapping).unwrap();

    assert_eq!(result.kind, CharacteristicKind::Curve);
    assert_eq!(result.axes[0].raw_values(), &[10.0, 20.0, 30.0]);
    assert_eq!(
        result.values.iter().map(|v| v.raw).collect::<Vec<_>>(),
        [
            openecu_calibration::RawValue::Signed(-20),
            openecu_calibration::RawValue::Signed(40),
            openecu_calibration::RawValue::Signed(50),
        ]
    );
    assert_eq!(
        result.pairs()[0].1.raw,
        openecu_calibration::RawValue::Signed(-20)
    );
    assert!(matches!(result.axes[0].source, AxisValueSource::Binary));
    assert_eq!(result.conversion_reference, "ident");
    assert_eq!(
        result.axes[0].values[0].conversion_reference.as_deref(),
        Some("ident")
    );
}

#[test]
fn decodes_map_as_z_y_x_only_for_row_dir() {
    let module = module();
    let mut bytes = vec![0u8; 26];
    bytes[0..2].copy_from_slice(&3u16.to_be_bytes());
    bytes[2..8].copy_from_slice(&[0, 10, 0, 20, 0, 30]);
    bytes[8..10].copy_from_slice(&2u16.to_be_bytes());
    bytes[10..14].copy_from_slice(&[0, 100, 0, 200]);
    bytes[14..26].copy_from_slice(&[0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6]);
    let image = FirmwareImage::new("map", bytes);
    let mapping = EcuAddressMap::new(0x1100, 0, 26).unwrap();
    let result =
        decode_characteristic(&module, &module.characteristics[1], &image, &mapping).unwrap();

    assert_eq!(result.kind, CharacteristicKind::Map);
    assert_eq!(result.dimensions, vec![2, 3]);
    assert_eq!(
        result.z_at(0, 0).unwrap().raw,
        openecu_calibration::RawValue::Unsigned(1)
    );
    assert_eq!(
        result.z_at(0, 2).unwrap().raw,
        openecu_calibration::RawValue::Unsigned(3)
    );
    assert_eq!(
        result.z_at(1, 0).unwrap().raw,
        openecu_calibration::RawValue::Unsigned(4)
    );
    assert_eq!(
        result.z_at(1, 2).unwrap().raw,
        openecu_calibration::RawValue::Unsigned(6)
    );
}

#[test]
fn rejects_declared_dimensions_that_disagree_with_reconstructed_axes() {
    let mut module = module();
    module.characteristics[0].dimensions = vec![4];
    let image = FirmwareImage::new("curve", vec![0, 3, 0, 10, 0, 20, 0, 30, 0, 40]);
    let mapping = EcuAddressMap::new(0x1000, 0, 10).unwrap();

    assert!(matches!(
        decode_characteristic(&module, &module.characteristics[0], &image, &mapping),
        Err(CalibrationError::UnsupportedA2lLayout { .. })
    ));
}

#[test]
fn rejects_column_dir_instead_of_guessing_matrix_orientation() {
    let mut module = module();
    module.record_layouts[1].elements[4].index_mode = Some("COLUMN_DIR".into());
    let image = FirmwareImage::new("map", vec![0; 18]);
    let mapping = EcuAddressMap::new(0x1100, 0, 18).unwrap();
    assert!(matches!(
        decode_characteristic(&module, &module.characteristics[1], &image, &mapping),
        Err(CalibrationError::UnsupportedA2lLayout { .. })
    ));
}

#[test]
fn rejects_fnc_values_before_a_variable_axis() {
    let mut module = module();
    let elements = &mut module.record_layouts[1].elements;
    elements
        .iter_mut()
        .find(|element| element.keyword == "FNC_VALUES")
        .unwrap()
        .position = Some(2);
    elements
        .iter_mut()
        .find(|element| element.keyword == "AXIS_PTS_X")
        .unwrap()
        .position = Some(3);
    elements
        .iter_mut()
        .find(|element| element.keyword == "NO_AXIS_PTS_Y")
        .unwrap()
        .position = Some(4);
    elements
        .iter_mut()
        .find(|element| element.keyword == "AXIS_PTS_Y")
        .unwrap()
        .position = Some(5);
    let image = FirmwareImage::new("map", vec![0; 26]);
    let mapping = EcuAddressMap::new(0x1100, 0, 26).unwrap();

    assert!(matches!(
        decode_characteristic(&module, &module.characteristics[1], &image, &mapping),
        Err(CalibrationError::UnsupportedA2lLayout { .. })
    ));
}

#[test]
fn rejects_axis_cardinality_above_declared_capacity() {
    let module = module();
    let image = FirmwareImage::new("curve", vec![0xff, 0xff]);
    let mapping = EcuAddressMap::new(0x1000, 0, 2).unwrap();

    assert!(matches!(
        decode_characteristic(&module, &module.characteristics[0], &image, &mapping),
        Err(CalibrationError::Axis(
            AxisReconstructionError::CapacityExceeded { .. }
        ))
    ));
}

#[test]
fn rejects_truncated_data_and_unknown_conversion() {
    let module = module();
    let image = FirmwareImage::new("short", vec![3, 0, 10]);
    let mapping = EcuAddressMap::new(0x1000, 0, 3).unwrap();
    assert!(matches!(
        decode_characteristic(&module, &module.characteristics[0], &image, &mapping),
        Err(CalibrationError::Axis(_))
    ));

    let mut bad = module.clone();
    bad.characteristics[0].conversion_reference = Some("missing".into());
    assert!(matches!(
        decode_characteristic(&bad, &bad.characteristics[0], &image, &mapping),
        Err(CalibrationError::MissingA2lMetadata {
            field: "conversion",
            ..
        })
    ));
}

#[test]
fn decodes_little_endian_values_and_fixed_axis_without_firmware_axis_provenance() {
    let mut module = module();
    module.mod_common.as_mut().unwrap().byte_order = Some("MSB_LAST".into());
    module.characteristics[0].axis_descriptions[0].attribute =
        Some(openecu_definitions::AxisAttribute::Fix);
    module.characteristics[0].axis_descriptions[0].fix_axis_par =
        Some(openecu_definitions::FixAxisPar {
            offset: 10,
            shift: 1,
            number_of_points: 3,
        });
    let image = FirmwareImage::new(
        "little",
        vec![3, 0, 10, 0, 20, 0, 30, 0, 0xec, 0xff, 40, 0, 50, 0],
    );
    let mapping = EcuAddressMap::new(0x1000, 0, 14).unwrap();
    let result =
        decode_characteristic(&module, &module.characteristics[0], &image, &mapping).unwrap();
    assert_eq!(
        result.values.iter().map(|v| v.raw).collect::<Vec<_>>(),
        [
            openecu_calibration::RawValue::Signed(-20),
            openecu_calibration::RawValue::Signed(40),
            openecu_calibration::RawValue::Signed(50),
        ]
    );
    assert_eq!(result.axes[0].raw_values(), &[10.0, 12.0, 14.0]);
    assert!(
        result.axes[0]
            .values
            .iter()
            .all(|value| value.provenance.is_none())
    );
    assert!(matches!(result.axes[0].source, AxisValueSource::Generated));
}

#[test]
fn rejects_an_unmapped_characteristic_address() {
    let mut module = module();
    module.characteristics[0].address = Some(0x9000);
    let image = FirmwareImage::new("curve", vec![0; 14]);
    let mapping = EcuAddressMap::new(0x1000, 0, 14).unwrap();
    assert!(matches!(
        decode_characteristic(&module, &module.characteristics[0], &image, &mapping),
        Err(CalibrationError::Axis(AxisReconstructionError::Firmware(_)))
    ));
}
