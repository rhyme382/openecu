use openecu_definitions::fixed_axis::{
    FixedAxisConversion, FixedAxisError, FixedAxisMethod, FixedAxisRequest, calculate_fixed_axis,
};
use openecu_definitions::{CompuMethod, parse_a2l};

fn axis(source: &str) -> openecu_definitions::AxisDescription {
    let doc = parse_a2l(source).unwrap();
    doc.project.modules[0].characteristics[0].axis_descriptions[0].clone()
}

fn source(axis_descr: &str) -> String {
    format!(
        "/begin PROJECT p \"p\"\n/begin MODULE m \"m\"\n/begin CHARACTERISTIC c \"c\" CURVE 0x1000 rl 0 cm 0 100\n{axis_descr}\n/end CHARACTERISTIC\n/end MODULE\n/end PROJECT"
    )
}

fn request<'a>(axis: &'a openecu_definitions::AxisDescription) -> FixedAxisRequest<'a> {
    FixedAxisRequest {
        axis,
        capacity: Some(axis.max_axis_points.unwrap_or(1024)),
        dimensions: None,
        conversion: FixedAxisConversion::Raw,
    }
}

#[test]
fn calculates_fix_axis_par_as_raw_values_with_provenance() {
    let a = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 5 0 200\nFIX_AXIS_PAR 100 1 5\n/end AXIS_DESCR",
    ));
    let result = calculate_fixed_axis(request(&a)).unwrap();
    assert_eq!(result.raw_values(), &[100.0, 102.0, 104.0, 106.0, 108.0]);
    assert!(result.physical_values().is_none());
    assert_eq!(
        result.method(),
        &FixedAxisMethod::Par(a.fix_axis_par.clone().unwrap())
    );
    assert_eq!(result.provenance().indices(), &[0, 1, 2, 3, 4]);
}

#[test]
fn calculates_negative_shift_without_implicit_physical_conversion() {
    let a = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 4 0 2\nFIX_AXIS_PAR 0 -1 4\n/end AXIS_DESCR",
    ));
    let result = calculate_fixed_axis(request(&a)).unwrap();
    assert_eq!(result.raw_values(), &[0.0, 0.5, 1.0, 1.5]);
    assert!(result.physical_values().is_none());
}

#[test]
fn calculates_fix_axis_par_dist() {
    let a = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 4 0 30\nFIX_AXIS_PAR_DIST 10 5 4\n/end AXIS_DESCR",
    ));
    let result = calculate_fixed_axis(request(&a)).unwrap();
    assert_eq!(result.raw_values(), &[10.0, 15.0, 20.0, 25.0]);
}

#[test]
fn identical_conversion_is_physical_only_when_explicitly_resolved() {
    let a = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY cm 3 0 4\nFIX_AXIS_PAR 0 1 3\n/end AXIS_DESCR",
    ));
    let method = CompuMethod {
        name: "cm".into(),
        description: None,
        conversion_type: Some("IDENTICAL".into()),
        format: None,
        unit: None,
        extra: vec![],
        raw_blocks: vec![],
        raw_block: 0,
        source: openecu_definitions::SourceLocation { line: 1, column: 1 },
    };
    let mut r = request(&a);
    r.capacity = Some(3);
    r.dimensions = Some(&[3]);
    r.conversion = FixedAxisConversion::Identical(&method);
    let result = calculate_fixed_axis(r).unwrap();
    assert_eq!(result.physical_values(), Some(vec![0.0, 2.0, 4.0]));
}

#[test]
fn rejects_syntax_cardinality_and_capacity_errors() {
    let mut a = axis(&source(
        "/begin AXIS_DESCR STD_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 3 0 3\nFIX_AXIS_PAR 0 1 3\n/end AXIS_DESCR",
    ));
    assert!(matches!(
        calculate_fixed_axis(request(&a)),
        Err(FixedAxisError::NotFixedAxis)
    ));

    a.attribute = Some(openecu_definitions::AxisAttribute::Fix);
    a.fix_axis_par = Some(openecu_definitions::FixAxisPar {
        offset: 0,
        shift: 1,
        number_of_points: 3,
    });
    a.fix_axis_par_dist = Some(openecu_definitions::FixAxisParDist {
        offset: 0,
        distance: 1,
        number_of_points: 3,
    });
    assert!(matches!(
        calculate_fixed_axis(request(&a)),
        Err(FixedAxisError::ContradictoryParameters)
    ));

    a.fix_axis_par_dist = None;
    let mut r = request(&a);
    r.capacity = None;
    assert!(matches!(
        calculate_fixed_axis(r),
        Err(FixedAxisError::MissingCapacity)
    ));
}

#[test]
fn rejects_shift_overflow_capacity_dimensions_conversion_and_nonfinite_inputs() {
    let mut a = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 2 0 1\nFIX_AXIS_PAR 0 1024 2\n/end AXIS_DESCR",
    ));
    assert!(matches!(
        calculate_fixed_axis(request(&a)),
        Err(FixedAxisError::InvalidShift { .. })
    ));

    a.fix_axis_par = Some(openecu_definitions::FixAxisPar {
        offset: i64::MAX,
        shift: 0,
        number_of_points: 2,
    });
    assert!(matches!(
        calculate_fixed_axis(request(&a)),
        Err(FixedAxisError::NonFiniteValue { .. }
            | FixedAxisError::ArithmeticOverflow { .. }
            | FixedAxisError::InexactInteger { .. })
    ));

    let mut r = request(&a);
    r.capacity = Some(1);
    assert!(matches!(
        calculate_fixed_axis(r),
        Err(FixedAxisError::CapacityExceeded { .. })
    ));

    let dimensions_axis = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 2 0 1\nFIX_AXIS_PAR 0 0 2\n/end AXIS_DESCR",
    ));
    let mut r = request(&dimensions_axis);
    r.dimensions = Some(&[1, 3]);
    assert!(matches!(
        calculate_fixed_axis(r),
        Err(FixedAxisError::InconsistentDimensions { .. })
    ));

    a.conversion_reference = Some("unknown".into());
    a.fix_axis_par = Some(openecu_definitions::FixAxisPar {
        offset: 0,
        shift: 0,
        number_of_points: 2,
    });
    let mut r = request(&a);
    r.capacity = Some(2);
    r.dimensions = Some(&[2]);
    assert!(matches!(
        calculate_fixed_axis(r),
        Err(FixedAxisError::UnknownConversion { .. })
    ));
}

#[test]
fn accepts_representable_large_positive_shifts_without_i128_limit() {
    for shift in [127, 128, 1023] {
        let count = if shift == 1023 { 1 } else { 2 };
        let mut a = axis(&source(&format!(
            "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD {count} 0 1e400\nFIX_AXIS_PAR 0 {shift} {count}\n/end AXIS_DESCR"
        )));
        a.upper_limit = None;
        let mut r = request(&a);
        r.capacity = Some(count);
        let result = calculate_fixed_axis(r).unwrap();
        assert_eq!(result.values().len(), count as usize);
        assert!(result.values().iter().all(|v| v.raw.is_finite()));
    }
}

#[test]
fn checks_negative_shift_boundaries_without_underflowing_to_zero() {
    for shift in [-1074, -1023, -1022] {
        let mut a = axis(&source(&format!(
            "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 2 -1 1\nFIX_AXIS_PAR 0 {shift} 2\n/end AXIS_DESCR"
        )));
        a.lower_limit = None;
        a.upper_limit = None;
        let values = calculate_fixed_axis(request(&a)).unwrap().raw_values();
        assert_eq!(values[0], 0.0);
        assert!(values[1] > 0.0);
    }

    let a = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 1 0 1\nFIX_AXIS_PAR 0 -1075 1\n/end AXIS_DESCR",
    ));
    assert!(matches!(
        calculate_fixed_axis(request(&a)),
        Err(FixedAxisError::InvalidShift { shift: -1075 })
    ));
}

#[test]
fn rejects_shift_results_that_overflow_binary64() {
    let a = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 3 0 1e400\nFIX_AXIS_PAR 0 1023 3\n/end AXIS_DESCR",
    ));
    let result = calculate_fixed_axis(request(&a));
    assert!(matches!(
        result,
        Err(FixedAxisError::NonFiniteValue { index: 2 })
    ));
}

#[test]
fn rejects_inexact_integer_conversion_in_fixed_axis() {
    let mut a = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 1 0 1\nFIX_AXIS_PAR 9007199254740993 0 1\n/end AXIS_DESCR",
    ));
    a.lower_limit = None;
    a.upper_limit = None;
    assert!(matches!(
        calculate_fixed_axis(request(&a)),
        Err(FixedAxisError::InexactInteger { .. })
    ));

    let mut negative = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 1 0 1\nFIX_AXIS_PAR -9007199254740993 0 1\n/end AXIS_DESCR",
    ));
    negative.lower_limit = None;
    negative.upper_limit = None;
    assert!(matches!(
        calculate_fixed_axis(request(&negative)),
        Err(FixedAxisError::InexactInteger { .. })
    ));
}

#[test]
fn accepts_exact_two_to_the_53_and_rejects_exactness_loss_for_negative_value() {
    for offset in [9_007_199_254_740_992_i64, -9_007_199_254_740_992_i64] {
        let mut a = axis(&source(&format!(
            "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 1 0 1\nFIX_AXIS_PAR {offset} 0 1\n/end AXIS_DESCR"
        )));
        a.lower_limit = None;
        a.upper_limit = None;
        assert!(calculate_fixed_axis(request(&a)).is_ok());
    }
}

#[test]
fn checks_negative_and_zero_distances_with_signed_arithmetic() {
    let negative = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 3 -20 20\nFIX_AXIS_PAR_DIST 10 -5 3\n/end AXIS_DESCR",
    ));
    assert_eq!(
        calculate_fixed_axis(request(&negative))
            .unwrap()
            .raw_values(),
        [10.0, 5.0, 0.0]
    );

    let zero = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 2 0 10\nFIX_AXIS_PAR_DIST 7 0 2\n/end AXIS_DESCR",
    ));
    assert_eq!(
        calculate_fixed_axis(request(&zero)).unwrap().raw_values(),
        [7.0, 7.0]
    );

    let mut overflow = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 2 -9223372036854775808 0\nFIX_AXIS_PAR_DIST -9223372036854775808 -1 2\n/end AXIS_DESCR",
    ));
    overflow.lower_limit = None;
    overflow.upper_limit = None;
    assert!(matches!(
        calculate_fixed_axis(request(&overflow)),
        Err(FixedAxisError::ArithmeticOverflow { index: 1 })
    ));

    let mut positive_overflow = axis(&source(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 2 0 0\nFIX_AXIS_PAR_DIST 9223372036854773760 2048 2\n/end AXIS_DESCR",
    ));
    positive_overflow.lower_limit = None;
    positive_overflow.upper_limit = None;
    assert!(matches!(
        calculate_fixed_axis(request(&positive_overflow)),
        Err(FixedAxisError::ArithmeticOverflow { index: 1 })
    ));
}
