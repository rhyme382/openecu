use openecu_calibration::{AxisReconstructionError, reconstruct_com_axis, reconstruct_std_axis};
use openecu_definitions::{
    a2l::{Characteristic, Module},
    parse_a2l,
};
use openecu_firmware::{EcuAddressMap, FirmwareImage, MemoryRegion};

const A2L: &str = r#"
/begin PROJECT p "p"
  /begin MODULE m "m"
    /begin MOD_COMMON "common"
      BYTE_ORDER MSB_FIRST
    /end MOD_COMMON
    /begin RECORD_LAYOUT axis_rl
      NO_AXIS_PTS_X 1 UWORD
      AXIS_PTS_X 2 UWORD INDEX_INCR DIRECT
      ALIGNMENT_WORD 2
    /end RECORD_LAYOUT
    /begin COMPU_METHOD identity "identity" IDENTICAL "%6.2" "rpm"
    /end COMPU_METHOD
    /begin CHARACTERISTIC standard "standard" CURVE 0x1000 axis_rl 0 identity 0 1000
      /begin AXIS_DESCR STD_AXIS speed identity 4 0 1000
      /end AXIS_DESCR
    /end CHARACTERISTIC
    /begin AXIS_PTS shared "shared" 0x2000 speed axis_rl 0 identity 4 0 1000
    /end AXIS_PTS
    /begin CHARACTERISTIC combined "combined" CURVE 0x3000 axis_rl 0 identity 0 1000
      /begin AXIS_DESCR COM_AXIS speed identity 4 0 1000
        AXIS_PTS_REF shared
      /end AXIS_DESCR
    /end CHARACTERISTIC
  /end MODULE
/end PROJECT
"#;

fn fixture() -> (Module, Vec<Characteristic>) {
    let document = parse_a2l(A2L).unwrap();
    let module = document.project.modules[0].clone();
    let characteristics = module.characteristics.clone();
    (module, characteristics)
}

fn image() -> (FirmwareImage, EcuAddressMap) {
    let mut bytes = vec![0; 0x20];
    // Two records: count=3 followed by 10, 20, 30 at each mapped address.
    bytes[0x00..0x02].copy_from_slice(&3u16.to_be_bytes());
    bytes[0x02..0x08].copy_from_slice(&[0, 10, 0, 20, 0, 30]);
    bytes[0x10..0x12].copy_from_slice(&3u16.to_be_bytes());
    bytes[0x12..0x18].copy_from_slice(&[0, 40, 0, 50, 0, 60]);
    (
        FirmwareImage::new("synthetic", bytes),
        EcuAddressMap::new(0x1000, 0, 0x20).unwrap(),
    )
}

#[test]
fn reconstructs_std_axis_points_from_bin_with_provenance() {
    let (module, characteristics) = fixture();
    let (image, mapping) = image();
    let axis = reconstruct_std_axis(&module, &characteristics[0], 0, &image, &mapping).unwrap();

    assert_eq!(axis.name, "standard");
    assert_eq!(
        axis.values.iter().map(|v| v.raw).collect::<Vec<_>>(),
        [
            openecu_calibration::RawValue::Unsigned(10),
            openecu_calibration::RawValue::Unsigned(20),
            openecu_calibration::RawValue::Unsigned(30),
        ]
    );
    assert_eq!(axis.values[0].physical.value, 10.0);
    assert_eq!(axis.values[2].provenance.ecu_address, 0x1006);
    assert_eq!(axis.values[2].provenance.bin_offset, 6);
}

#[test]
fn resolves_com_axis_ref_to_shared_axis_pts_and_reads_its_bin() {
    let (module, characteristics) = fixture();
    let (image, _) = image();
    let mapping = EcuAddressMap::from_regions(vec![
        MemoryRegion::new(0x1000, 0x08, 0).unwrap(),
        MemoryRegion::new(0x2000, 0x08, 0x10).unwrap(),
    ])
    .unwrap();
    let axis = reconstruct_com_axis(&module, &characteristics[1], 0, &image, &mapping).unwrap();

    assert_eq!(axis.source, "shared");
    assert_eq!(
        axis.values
            .iter()
            .map(|v| v.physical.value)
            .collect::<Vec<_>>(),
        [40.0, 50.0, 60.0]
    );
}

#[test]
fn rejects_non_identical_axis_conversion_and_unsupported_axis_type() {
    let (mut module, characteristics) = fixture();
    module.compu_methods[0].conversion_type = Some("LINEAR".into());
    let (image, mapping) = image();
    assert!(matches!(
        reconstruct_std_axis(&module, &characteristics[0], 0, &image, &mapping),
        Err(AxisReconstructionError::UnsupportedConversion { .. })
    ));
    let mut characteristic = characteristics[0].clone();
    characteristic.axis_descriptions[0].attribute = Some(openecu_definitions::AxisAttribute::Fix);
    assert!(matches!(
        reconstruct_std_axis(&module, &characteristic, 0, &image, &mapping),
        Err(AxisReconstructionError::UnsupportedAxisAttribute { .. })
    ));
}

#[test]
fn rejects_axis_count_above_declared_capacity_before_reading_points() {
    let (module, mut characteristics) = fixture();
    characteristics[0].axis_descriptions[0].max_axis_points = Some(2);
    let (image, mapping) = image();
    assert!(matches!(
        reconstruct_std_axis(&module, &characteristics[0], 0, &image, &mapping),
        Err(AxisReconstructionError::CapacityExceeded {
            count: 3,
            capacity: 2,
            ..
        })
    ));
}
