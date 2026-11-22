//! T002-C1: typed AXIS_DESCR / RECORD_LAYOUT projections and keyword audit.
use openecu_definitions::a2l::{AxisAttribute, RecordLayoutElement};
use openecu_definitions::{A2lDocument, Severity, SourceLocation, parse_a2l};

fn module(body: &str) -> A2lDocument {
    parse_a2l(&format!(
        "/begin PROJECT p \"project\"\n/begin MODULE m \"module\"\n{body}\n/end MODULE\n/end PROJECT"
    ))
    .unwrap()
}

// Occupies lines 3..=5 of the generated module (line 1 PROJECT, line 2 MODULE).
const SUPPORT: &str = "/begin COMPU_METHOD cm \"d\" IDENTICAL \"%6.2\" \"rpm\" /end COMPU_METHOD\n\
/begin RECORD_LAYOUT rl FNC_VALUES 1 UWORD ROW_DIR DIRECT /end RECORD_LAYOUT\n\
/begin AXIS_PTS shared \"shared axis\" 0x2000 NO_INPUT_QUANTITY rl 0 cm 8 0 8000 /end AXIS_PTS";

// The CHARACTERISTIC starts on line 6; `axis` starts on line 7.
fn with_characteristic(axis: &str) -> A2lDocument {
    module(&format!(
        "{SUPPORT}\n/begin CHARACTERISTIC c \"d\" CURVE 0x1000 rl 0 cm 0 100\n{axis}\n/end CHARACTERISTIC"
    ))
}

fn errors(d: &A2lDocument) -> Vec<String> {
    d.diagnostics
        .iter()
        .filter(|x| x.severity == Severity::Error)
        .map(|x| x.message.clone())
        .collect()
}

fn count(d: &A2lDocument, severity: Severity, needle: &str) -> usize {
    d.diagnostics
        .iter()
        .filter(|x| x.severity == severity && x.message.contains(needle))
        .count()
}

#[test]
fn std_axis_positional_arguments_are_projected_with_provenance() {
    let d = with_characteristic(
        "/begin AXIS_DESCR STD_AXIS rpm_in cm 16 0 8000\n  PHYS_UNIT \"rpm\"\n  MONOTONY STRICT_INCREASE\n  READ_ONLY\n/end AXIS_DESCR",
    );
    assert!(errors(&d).is_empty(), "{:?}", errors(&d));
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert_eq!(a.attribute, Some(AxisAttribute::Std));
    assert_eq!(a.input_quantity.as_deref(), Some("rpm_in"));
    assert_eq!(a.conversion_reference.as_deref(), Some("cm"));
    assert_eq!(a.max_axis_points, Some(16));
    assert_eq!(a.lower_limit.as_ref().map(|n| n.value()), Some(0.0));
    assert_eq!(a.upper_limit.as_ref().map(|n| n.value()), Some(8000.0));
    assert_eq!(a.phys_unit.as_deref(), Some("rpm"));
    assert_eq!(a.monotony.as_deref(), Some("STRICT_INCREASE"));
    assert!(a.read_only);
    assert!(a.axis_pts_ref.is_none() && a.fix_axis_par.is_none());
    // "/begin AXIS_DESCR STD_AXIS rpm_in cm 16 0 8000" on line 7.
    assert_eq!(
        a.conversion_reference_source,
        Some(SourceLocation {
            line: 7,
            column: 35
        })
    );
    assert_eq!(
        a.max_axis_points_source,
        Some(SourceLocation {
            line: 7,
            column: 38
        })
    );
    assert_eq!(a.source.line, 7);
    assert_eq!(d.raw_block(a.raw_block).unwrap().keyword, "AXIS_DESCR");
    assert_eq!(count(&d, Severity::Unsupported, "PHYS_UNIT"), 0);
    assert_eq!(count(&d, Severity::Unsupported, "MONOTONY"), 0);
}

#[test]
fn positional_axis_max_does_not_change_legacy_dimensions() {
    let d = with_characteristic(
        "/begin AXIS_DESCR STD_AXIS NO_INPUT_QUANTITY cm 16 0 1 /end AXIS_DESCR",
    );
    assert!(
        d.project.modules[0].characteristics[0]
            .dimensions
            .is_empty()
    );
}

#[test]
fn com_axis_keeps_shared_axis_reference_and_its_location() {
    let d = with_characteristic(
        "/begin AXIS_DESCR\n COM_AXIS NO_INPUT_QUANTITY cm 8 0 8000\n AXIS_PTS_REF shared\n/end AXIS_DESCR",
    );
    assert!(errors(&d).is_empty(), "{:?}", errors(&d));
    let c = &d.project.modules[0].characteristics[0];
    let a = &c.axis_descriptions[0];
    assert_eq!(a.attribute, Some(AxisAttribute::Com));
    assert_eq!(a.axis_pts_ref.as_deref(), Some("shared"));
    assert_eq!(
        a.axis_pts_ref_source,
        Some(SourceLocation {
            line: 9,
            column: 15
        })
    );
    assert_eq!(c.axis_references, ["shared"]);
}

#[test]
fn com_axis_with_missing_shared_axis_is_an_error_at_the_reference() {
    let d = with_characteristic(
        "/begin AXIS_DESCR COM_AXIS NO_INPUT_QUANTITY cm 8 0 1\n AXIS_PTS_REF ghost\n/end AXIS_DESCR",
    );
    assert_eq!(count(&d, Severity::Error, "missing AXIS_PTS `ghost`"), 1);
}

#[test]
fn com_axis_without_shared_reference_warns_without_inventing_one() {
    let d = with_characteristic(
        "/begin AXIS_DESCR COM_AXIS NO_INPUT_QUANTITY cm 8 0 1 /end AXIS_DESCR",
    );
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert!(a.axis_pts_ref.is_none());
    assert_eq!(count(&d, Severity::Warning, "COM_AXIS"), 1);
    assert!(errors(&d).is_empty());
}

#[test]
fn fix_axis_parameters_are_typed_and_absent_ones_stay_none() {
    let d = with_characteristic(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY cm 4 0 3\n FIX_AXIS_PAR 0 1 4\n/end AXIS_DESCR\n\
         /begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY cm 4 0 3\n FIX_AXIS_PAR_DIST -2 5 4\n/end AXIS_DESCR\n\
         /begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY cm 3 0 3\n FIX_AXIS_PAR_LIST 0 1.5 3\n/end AXIS_DESCR",
    );
    assert!(errors(&d).is_empty(), "{:?}", errors(&d));
    let axes = &d.project.modules[0].characteristics[0].axis_descriptions;
    assert_eq!(axes.len(), 3);
    let p = axes[0].fix_axis_par.as_ref().unwrap();
    assert_eq!((p.offset, p.shift, p.number_of_points), (0, 1, 4));
    assert!(axes[0].fix_axis_par_dist.is_none() && axes[0].fix_axis_par_list.is_none());
    let q = axes[1].fix_axis_par_dist.as_ref().unwrap();
    assert_eq!((q.offset, q.distance, q.number_of_points), (-2, 5, 4));
    let l: Vec<f64> = axes[2]
        .fix_axis_par_list
        .as_ref()
        .unwrap()
        .iter()
        .map(|n| n.value())
        .collect();
    assert_eq!(l, [0.0, 1.5, 3.0]);
    assert_eq!(axes[0].attribute, Some(AxisAttribute::Fix));
}

#[test]
fn fix_axis_without_parameters_does_not_invent_values() {
    let d = with_characteristic(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY cm 4 0 3 /end AXIS_DESCR",
    );
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert!(
        a.fix_axis_par.is_none() && a.fix_axis_par_dist.is_none() && a.fix_axis_par_list.is_none()
    );
}

#[test]
fn malformed_fix_axis_parameters_are_errors_at_their_value() {
    let d = with_characteristic(
        "/begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY cm 4 0 3\n FIX_AXIS_PAR 0 x 4\n/end AXIS_DESCR\n\
         /begin AXIS_DESCR FIX_AXIS NO_INPUT_QUANTITY cm 4 0 3\n FIX_AXIS_PAR 0 1\n/end AXIS_DESCR",
    );
    // " FIX_AXIS_PAR 0 x 4" is line 8; `x` is at column 17.
    assert!(
        d.diagnostics
            .iter()
            .any(|x| x.severity == Severity::Error && x.line == 8 && x.column == 17),
        "{:?}",
        d.diagnostics
    );
    assert_eq!(count(&d, Severity::Error, "FIX_AXIS_PAR is incomplete"), 1);
}

#[test]
fn legacy_axis_descr_without_positional_arguments_is_kept_with_a_warning() {
    let d = parse_a2l(include_str!("fixtures/valid.a2l")).unwrap();
    assert!(!d.has_errors());
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert_eq!(a.attribute, None);
    assert_eq!(a.max_axis_points, None);
    assert_eq!(a.conversion_reference, None);
    assert_eq!(a.axis_pts_ref.as_deref(), Some("rpm_axis"));
    assert_eq!(count(&d, Severity::Warning, "AXIS_DESCR"), 1);
}

#[test]
fn incomplete_axis_descr_is_an_error_and_keeps_the_given_arguments() {
    let d = with_characteristic(
        "/begin AXIS_DESCR STD_AXIS NO_INPUT_QUANTITY\n MONOTONY MON_INCREASE\n/end AXIS_DESCR",
    );
    assert_eq!(
        count(&d, Severity::Error, "AXIS_DESCR declaration is incomplete"),
        1
    );
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert_eq!(a.attribute, Some(AxisAttribute::Std));
    assert_eq!(a.input_quantity.as_deref(), Some("NO_INPUT_QUANTITY"));
    assert_eq!(a.conversion_reference, None);
    assert_eq!(a.max_axis_points, None);
    assert_eq!(a.monotony.as_deref(), Some("MON_INCREASE"));
}

#[test]
fn axis_descr_arguments_may_span_lines_without_absorbing_keywords() {
    let d = with_characteristic(
        "/begin AXIS_DESCR STD_AXIS\n NO_INPUT_QUANTITY\n cm\n 8\n 0\n 1\n FORMAT \"%4.0\"\n/end AXIS_DESCR",
    );
    assert!(errors(&d).is_empty(), "{:?}", errors(&d));
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert_eq!(a.upper_limit.as_ref().map(|n| n.value()), Some(1.0));
    assert_eq!(a.format.as_deref(), Some("%4.0"));
}

#[test]
fn axis_descr_conversion_must_exist_unless_no_compu_method() {
    let missing = with_characteristic(
        "/begin AXIS_DESCR STD_AXIS NO_INPUT_QUANTITY nope 8 0 1 /end AXIS_DESCR",
    );
    assert_eq!(
        count(&missing, Severity::Error, "missing conversion `nope`"),
        1
    );
    let sentinel = with_characteristic(
        "/begin AXIS_DESCR STD_AXIS NO_INPUT_QUANTITY NO_COMPU_METHOD 8 0 1 /end AXIS_DESCR",
    );
    assert!(errors(&sentinel).is_empty());
}

#[test]
fn curve_axis_reference_must_name_a_characteristic() {
    let d = with_characteristic(
        "/begin AXIS_DESCR CURVE_AXIS NO_INPUT_QUANTITY cm 8 0 1\n CURVE_AXIS_REF c\n/end AXIS_DESCR\n\
         /begin AXIS_DESCR CURVE_AXIS NO_INPUT_QUANTITY cm 8 0 1\n CURVE_AXIS_REF ghost\n/end AXIS_DESCR",
    );
    let axes = &d.project.modules[0].characteristics[0].axis_descriptions;
    assert_eq!(axes[0].attribute, Some(AxisAttribute::Curve));
    assert_eq!(axes[1].curve_axis_ref.as_deref(), Some("ghost"));
    assert_eq!(count(&d, Severity::Error, "CURVE_AXIS_REF"), 1);
}

#[test]
fn invalid_axis_numbers_are_errors_at_their_arguments() {
    let d = with_characteristic(
        "/begin AXIS_DESCR STD_AXIS NO_INPUT_QUANTITY cm many 0 inf /end AXIS_DESCR",
    );
    assert_eq!(
        count(&d, Severity::Error, "invalid unsigned number `many`"),
        1
    );
    assert_eq!(count(&d, Severity::Error, "invalid finite number `inf`"), 1);
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert_eq!(a.max_axis_points, None);
    assert_eq!(a.upper_limit, None);
    assert!(a.lower_limit.is_some());
}

#[test]
fn unknown_axis_attribute_is_preserved_verbatim_and_reported() {
    let d = with_characteristic(
        "/begin AXIS_DESCR VENDOR_AXIS NO_INPUT_QUANTITY cm 8 0 1 /end AXIS_DESCR",
    );
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert_eq!(
        a.attribute,
        Some(AxisAttribute::Unknown("VENDOR_AXIS".into()))
    );
    assert_eq!(
        count(&d, Severity::Unsupported, "axis attribute `VENDOR_AXIS`"),
        1
    );
}

#[test]
fn unknown_axis_descr_keywords_and_blocks_are_preserved_and_diagnosed_once() {
    let d = with_characteristic(
        "/begin AXIS_DESCR STD_AXIS NO_INPUT_QUANTITY cm 8 0 1\n VENDOR_ITEM 1 2\n /begin VENDOR_BLOCK x\n /end VENDOR_BLOCK\n/end AXIS_DESCR",
    );
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert_eq!(a.extra.len(), 1);
    assert_eq!(a.extra[0].keyword, "VENDOR_ITEM");
    assert_eq!(a.raw_blocks.len(), 1);
    assert_eq!(count(&d, Severity::Unsupported, "VENDOR_ITEM"), 1);
    assert_eq!(count(&d, Severity::Unsupported, "VENDOR_BLOCK"), 1);
}

#[test]
fn axis_pts_exposes_input_quantity_limits_and_unit_additively() {
    let d = module(
        "/begin AXIS_PTS a \"d\" 0x10 in rl 0 cm 8 -5 8000\n PHYS_UNIT \"rpm\"\n MONOTONY MON_INCREASE\n/end AXIS_PTS",
    );
    let a = &d.project.modules[0].axis_points[0];
    assert_eq!(a.input_quantity.as_deref(), Some("in"));
    assert_eq!(a.lower_limit.as_ref().map(|n| n.value()), Some(-5.0));
    assert_eq!(a.upper_limit.as_ref().map(|n| n.value()), Some(8000.0));
    assert_eq!(a.phys_unit.as_deref(), Some("rpm"));
    assert_eq!(a.max_axis_points, Some(8));
    assert_eq!(count(&d, Severity::Unsupported, "MONOTONY"), 0);
}

fn layout(d: &A2lDocument) -> &[RecordLayoutElement] {
    &d.project.modules[0].record_layouts[0].elements
}

#[test]
fn record_layout_elements_keep_order_positions_types_and_index_modes() {
    let d = module(
        "/begin RECORD_LAYOUT rl\n\
         NO_AXIS_PTS_X 1 UWORD\n\
         AXIS_PTS_X 2 UWORD INDEX_INCR DIRECT\n\
         NO_AXIS_PTS_Y 3 UBYTE\n\
         AXIS_PTS_Y 4 UWORD INDEX_DECR DIRECT\n\
         FNC_VALUES 5 SWORD COLUMN_DIR DIRECT\n\
         AXIS_RESCALE_X 6 UWORD 4 INDEX_INCR DIRECT\n\
         FIX_NO_AXIS_PTS_Z 7\n\
         RESERVED 8 WORD\n\
         ALIGNMENT_WORD 2\n\
         /end RECORD_LAYOUT",
    );
    assert!(errors(&d).is_empty(), "{:?}", errors(&d));
    assert_eq!(
        d.diagnostics
            .iter()
            .filter(|x| x.severity == Severity::Unsupported)
            .count(),
        0,
        "{:?}",
        d.diagnostics
    );
    let e = layout(&d);
    let kinds: Vec<_> = e.iter().map(|x| x.keyword.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "NO_AXIS_PTS_X",
            "AXIS_PTS_X",
            "NO_AXIS_PTS_Y",
            "AXIS_PTS_Y",
            "FNC_VALUES",
            "AXIS_RESCALE_X",
            "FIX_NO_AXIS_PTS_Z",
            "RESERVED"
        ]
    );
    assert_eq!(
        (e[0].position, e[0].data_type.as_deref()),
        (Some(1), Some("UWORD"))
    );
    assert_eq!(e[1].index_increment.as_deref(), Some("INDEX_INCR"));
    assert_eq!(e[1].address_type.as_deref(), Some("DIRECT"));
    assert_eq!(e[3].index_increment.as_deref(), Some("INDEX_DECR"));
    assert_eq!(e[4].index_mode.as_deref(), Some("COLUMN_DIR"));
    assert_eq!(e[4].index_increment, None);
    assert_eq!(e[5].max_rescale_pairs, Some(4));
    assert_eq!(e[6].fixed_axis_points, Some(7));
    assert_eq!(e[6].position, None);
    assert_eq!(
        (e[7].position, e[7].data_type.as_deref()),
        (Some(8), Some("WORD"))
    );
    // FNC_VALUES is on line 8 (PROJECT, MODULE, RECORD_LAYOUT, then the fifth element).
    assert_eq!(e[4].source, SourceLocation { line: 8, column: 1 });
    assert_eq!(
        e[4].argument_locations[0],
        SourceLocation {
            line: 8,
            column: 12
        }
    );
}

#[test]
fn record_layout_incomplete_or_malformed_elements_are_errors_but_kept() {
    let d = module(
        "/begin RECORD_LAYOUT rl\n FNC_VALUES 1 UWORD\n AXIS_PTS_X abc UWORD INDEX_INCR DIRECT\n NO_AXIS_PTS_X\n/end RECORD_LAYOUT",
    );
    let e = layout(&d);
    assert_eq!(e.len(), 3);
    assert_eq!(e[0].index_mode, None);
    assert_eq!(e[0].address_type, None);
    assert_eq!(e[1].position, None);
    assert_eq!(e[2].data_type, None);
    assert_eq!(count(&d, Severity::Error, "FNC_VALUES is incomplete"), 1);
    assert_eq!(count(&d, Severity::Error, "NO_AXIS_PTS_X is incomplete"), 1);
    // " AXIS_PTS_X abc" is line 5; `abc` is at column 13.
    assert!(
        d.diagnostics
            .iter()
            .any(|x| x.severity == Severity::Error && x.line == 5 && x.column == 13),
        "{:?}",
        d.diagnostics
    );
}

#[test]
fn record_layout_unknown_keywords_stay_raw_and_not_in_elements() {
    let d = module(
        "/begin RECORD_LAYOUT rl\n FNC_VALUES 1 UWORD ROW_DIR DIRECT\n VENDOR_LAYOUT 9 x\n/end RECORD_LAYOUT",
    );
    assert_eq!(layout(&d).len(), 1);
    let r = &d.project.modules[0].record_layouts[0];
    assert!(r.extra.iter().any(|i| i.keyword == "VENDOR_LAYOUT"));
    assert_eq!(count(&d, Severity::Unsupported, "VENDOR_LAYOUT"), 1);
}

#[test]
fn recognized_record_layout_families_do_not_swallow_each_other_when_incomplete() {
    let d = module(
        "/begin RECORD_LAYOUT\n DIST_OP_X 1 UWORD\n OFFSET_X 2 UWORD\n SRC_ADDR_Y 3 ULONG\n RIP_ADDR_X 4 ULONG\n SHIFT_OP_4 5 UBYTE\n NO_RESCALE_X 6 UWORD\n STATIC_RECORD_LAYOUT\n/end RECORD_LAYOUT",
    );
    assert_eq!(
        count(&d, Severity::Unsupported, "keyword"),
        0,
        "{:?}",
        d.diagnostics
    );
    assert_eq!(
        count(
            &d,
            Severity::Error,
            "RECORD_LAYOUT declaration is incomplete"
        ),
        1
    );
    assert_eq!(layout(&d).len(), 6);
}

#[test]
fn mod_common_and_characteristic_internal_keywords_are_recognized() {
    let d = module(
        "/begin MOD_COMMON \"c\"\n ALIGNMENT_BYTE 1\n DATA_SIZE 16\n DEPOSIT ABSOLUTE\n S_REC_LAYOUT rl\n BYTE_ORDER MSB_LAST\n/end MOD_COMMON\n\
         /begin CHARACTERISTIC\n c \"d\" VAL_BLK 1 NO_RECORD_LAYOUT 0 NO_COMPU_METHOD 0 1\n MATRIX_DIM 4\n DISCRETE\n/end CHARACTERISTIC",
    );
    assert_eq!(
        d.diagnostics
            .iter()
            .filter(|x| x.severity == Severity::Unsupported)
            .count(),
        0,
        "{:?}",
        d.diagnostics
    );
    let c = &d.project.modules[0].characteristics[0];
    assert_eq!(
        c.extra
            .iter()
            .find(|i| i.keyword == "MATRIX_DIM")
            .unwrap()
            .values,
        ["4"]
    );
}

#[test]
fn incomplete_characteristic_keeps_matrix_dim_out_of_its_arguments() {
    let d = module("/begin CHARACTERISTIC c \"d\" VAL_BLK 1\n MATRIX_DIM 4\n/end CHARACTERISTIC");
    assert_eq!(
        count(
            &d,
            Severity::Error,
            "CHARACTERISTIC declaration is incomplete"
        ),
        1
    );
    assert_eq!(
        d.project.modules[0].characteristics[0].extra[0].keyword,
        "MATRIX_DIM"
    );
}

// ---- T002-C1 review: A2lNumber invariant under Deserialize ----

#[test]
fn a2l_number_deserialize_rejects_non_finite_and_inconsistent_values() {
    use openecu_definitions::a2l::A2lNumber;
    let ok: A2lNumber = serde_json::from_str(r#"{"value":1.5,"text":"1.5"}"#).unwrap();
    assert_eq!(ok, A2lNumber::parse("1.5").unwrap());
    for bad in [
        r#"{"value":1.0,"text":"inf"}"#,
        r#"{"value":1.0,"text":"NaN"}"#,
        r#"{"value":1.0,"text":"abc"}"#,
        r#"{"value":1.0,"text":"1e999"}"#,
        r#"{"value":2.0,"text":"1.5"}"#,
        r#"{"value":null,"text":"1.5"}"#,
    ] {
        assert!(serde_json::from_str::<A2lNumber>(bad).is_err(), "{bad}");
    }
}

#[test]
fn a2l_number_roundtrip_preserves_spelling_and_equality() {
    use openecu_definitions::a2l::A2lNumber;
    let n = A2lNumber::parse("1.50E+2").unwrap();
    let back: A2lNumber = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
    assert_eq!(back, n);
    assert_eq!(back.text(), "1.50E+2");
    assert_ne!(n, A2lNumber::parse("150").unwrap());
    assert_eq!(n, n.clone());
}

// ---- T002-C1 review: incomplete AXIS_DESCR vs unknown vendor keyword ----

#[test]
fn incomplete_axis_descr_does_not_absorb_vendor_keyword_in_numeric_slot() {
    let d =
        with_characteristic("/begin AXIS_DESCR STD_AXIS rpm_in cm\n  VENDOR_KW 5\n/end AXIS_DESCR");
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert!(a.max_axis_points.is_none());
    assert!(a.lower_limit.is_none() && a.upper_limit.is_none());
    assert_eq!(count(&d, Severity::Error, "invalid integer"), 0);
    assert_eq!(count(&d, Severity::Error, "incomplete"), 1);
    assert_eq!(count(&d, Severity::Unsupported, "VENDOR_KW"), 1);
    assert!(
        a.extra
            .iter()
            .any(|i| i.keyword == "VENDOR_KW" && i.values == ["5"])
    );
}

#[test]
fn multiline_positional_arguments_still_parse() {
    let d = with_characteristic(
        "/begin AXIS_DESCR\n STD_AXIS\n rpm_in\n cm\n 16\n 0\n 8000\n  PHYS_UNIT \"rpm\"\n/end AXIS_DESCR",
    );
    assert!(errors(&d).is_empty(), "{:?}", errors(&d));
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert_eq!(a.max_axis_points, Some(16));
    assert_eq!(a.upper_limit.as_ref().map(|n| n.value()), Some(8000.0));
    assert_eq!(a.phys_unit.as_deref(), Some("rpm"));
}

#[test]
fn vendor_keyword_in_non_numeric_slot_is_ambiguous_but_lossless() {
    // Slot 2 is an identifier: a vendor keyword cannot be told apart from it.
    let d =
        with_characteristic("/begin AXIS_DESCR STD_AXIS rpm_in\n  VENDOR_KW 5\n/end AXIS_DESCR");
    let a = &d.project.modules[0].characteristics[0].axis_descriptions[0];
    assert_eq!(a.conversion_reference.as_deref(), Some("VENDOR_KW"));
    assert!(format!("{d:?}").contains("VENDOR_KW"));
}
