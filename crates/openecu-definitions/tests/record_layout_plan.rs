//! T002-C2.1: RECORD_LAYOUT planning (positions, alignment, counts, overflow).
use openecu_definitions::parse_a2l;
use openecu_definitions::record_layout::{
    AxisError, ElementKind, Extent, LayoutVerdict, PointCounts, RecordAxis, RecordPlan,
};

fn plan(body: &str) -> Result<RecordPlan, AxisError> {
    let doc = parse_a2l(&format!(
        "/begin PROJECT p \"p\"\n/begin MODULE m \"m\"\n/begin RECORD_LAYOUT rl\n{body}\n/end RECORD_LAYOUT\n/end MODULE\n/end PROJECT"
    ))
    .unwrap();
    RecordPlan::from_layout_with_mod_common(
        &doc.project.modules[0].record_layouts[0],
        doc.project.modules[0].mod_common.as_ref(),
    )
}

fn plan_with_mod_common(mod_common: &str, body: &str) -> Result<RecordPlan, AxisError> {
    let doc = parse_a2l(&format!(
        "/begin PROJECT p \"p\"\n/begin MODULE m \"m\"\n/begin MOD_COMMON \"c\"\n{mod_common}\n/end MOD_COMMON\n/begin RECORD_LAYOUT rl\n{body}\n/end RECORD_LAYOUT\n/end MODULE\n/end PROJECT"
    ))
    .unwrap();
    RecordPlan::from_layout_with_mod_common(
        &doc.project.modules[0].record_layouts[0],
        doc.project.modules[0].mod_common.as_ref(),
    )
}

fn err(body: &str) -> AxisError {
    plan(body).expect_err("layout must be rejected")
}

const CURVE: &str = "NO_AXIS_PTS_X 1 UWORD\nAXIS_PTS_X 2 UWORD INDEX_INCR DIRECT\nFNC_VALUES 3 UWORD ROW_DIR DIRECT";

#[test]
fn plans_a_curve_layout_with_static_prefix_offsets() {
    let p = plan(CURVE).unwrap();
    assert_eq!(p.elements().len(), 3);
    let no = p.element(ElementKind::NoAxisPts(RecordAxis::X)).unwrap();
    assert_eq!((no.static_offset, no.static_size), (Some(0), Some(2)));
    let ax = p.element(ElementKind::AxisPts(RecordAxis::X)).unwrap();
    assert_eq!(ax.static_offset, Some(2));
    assert_eq!(ax.static_size, None);
    assert_eq!(
        ax.extent,
        Extent::PerPoint(openecu_definitions::record_layout::CountSource::Axis(
            RecordAxis::X
        ))
    );
    // FNC_VALUES follows a count-dependent element: offset unknown until resolve.
    assert_eq!(
        p.element(ElementKind::FncValues).unwrap().static_offset,
        None
    );
    assert_eq!(p.required_base_alignment(), 2);
}

#[test]
fn resolves_offsets_from_counts() {
    let p = plan(CURVE).unwrap();
    let counts = PointCounts::new()
        .with_axis(RecordAxis::X, 5)
        .with_fnc_values(5);
    let r = p.resolve(&counts).unwrap();
    let get = |k| *r.placement(k).unwrap();
    assert_eq!(
        (
            get(ElementKind::NoAxisPts(RecordAxis::X)).offset,
            get(ElementKind::NoAxisPts(RecordAxis::X)).size
        ),
        (0, 2)
    );
    assert_eq!(
        (
            get(ElementKind::AxisPts(RecordAxis::X)).offset,
            get(ElementKind::AxisPts(RecordAxis::X)).size
        ),
        (2, 10)
    );
    assert_eq!(
        (
            get(ElementKind::FncValues).offset,
            get(ElementKind::FncValues).size
        ),
        (12, 10)
    );
    assert_eq!(r.total_size, 22);
    assert_eq!(get(ElementKind::FncValues).range().unwrap(), 12..22);
}

#[test]
fn source_order_does_not_matter_positions_do() {
    let p = plan("FNC_VALUES 2 UBYTE ROW_DIR DIRECT\nNO_AXIS_PTS_X 1 UBYTE\n").unwrap();
    assert_eq!(p.elements()[0].kind, ElementKind::NoAxisPts(RecordAxis::X));
    assert_eq!(p.elements()[1].kind, ElementKind::FncValues);
}

#[test]
fn fix_no_axis_pts_provides_the_count_without_a_position() {
    let p = plan(
        "FIX_NO_AXIS_PTS_X 4\nAXIS_PTS_X 1 UBYTE INDEX_INCR DIRECT\nFNC_VALUES 2 UWORD ROW_DIR DIRECT",
    )
    .unwrap();
    assert_eq!(p.fixed_axis_points(RecordAxis::X), Some(4));
    let r = p.resolve(&PointCounts::new().with_fnc_values(4)).unwrap();
    assert_eq!(r.placement(ElementKind::FncValues).unwrap().offset, 4);
    assert_eq!(r.total_size, 12);
    // Same count from the caller is fine, a different one is ambiguous.
    assert!(
        p.resolve(
            &PointCounts::new()
                .with_axis(RecordAxis::X, 4)
                .with_fnc_values(4)
        )
        .is_ok()
    );
    let e = p
        .resolve(
            &PointCounts::new()
                .with_axis(RecordAxis::X, 5)
                .with_fnc_values(4),
        )
        .unwrap_err();
    assert_eq!(
        e,
        AxisError::ConflictingCounts {
            axis: RecordAxis::X
        }
    );
    assert_eq!(e.verdict(), LayoutVerdict::Ambiguous);
}

#[test]
fn resolve_requires_every_count() {
    let p = plan(CURVE).unwrap();
    let e = p
        .resolve(&PointCounts::new().with_axis(RecordAxis::X, 2))
        .unwrap_err();
    assert_eq!(
        e,
        AxisError::MissingCount {
            keyword: "FNC_VALUES".into()
        }
    );
    assert_eq!(e.verdict(), LayoutVerdict::Ambiguous);
}

#[test]
fn resolve_overflow_is_reported_not_wrapped() {
    let p = plan("FNC_VALUES 1 ULONG ROW_DIR DIRECT").unwrap();
    let e = p
        .resolve(&PointCounts::new().with_fnc_values(u64::MAX))
        .unwrap_err();
    assert!(matches!(e, AxisError::LimitExceeded { .. }));
    // Sum overflow: two elements each close to u64::MAX / 2.
    let p =
        plan("AXIS_PTS_X 1 UBYTE INDEX_INCR DIRECT\nFNC_VALUES 2 UBYTE ROW_DIR DIRECT").unwrap();
    let c = PointCounts::new()
        .with_axis(RecordAxis::X, u64::MAX)
        .with_fnc_values(1);
    assert!(matches!(
        p.resolve(&c).unwrap_err(),
        AxisError::LimitExceeded { .. }
    ));
}

#[test]
fn placement_range_is_checked_against_usize() {
    let p = plan("FNC_VALUES 1 UBYTE ROW_DIR DIRECT").unwrap();
    let r = p.resolve(&PointCounts::new().with_fnc_values(16)).unwrap();
    assert_eq!(r.placements[0].range().unwrap(), 0..16);
}

#[test]
fn rejects_position_zero() {
    let e = err("FNC_VALUES 0 UWORD ROW_DIR DIRECT");
    assert!(matches!(e, AxisError::InvalidPosition { position: 0, .. }));
    assert_eq!(e.verdict(), LayoutVerdict::Malformed);
}

#[test]
fn rejects_duplicate_positions() {
    let e = err("NO_AXIS_PTS_X 1 UWORD\nFNC_VALUES 1 UWORD ROW_DIR DIRECT");
    assert!(matches!(
        e,
        AxisError::DuplicatePosition { position: 1, .. }
    ));
}

#[test]
fn rejects_position_gaps() {
    let e = err("NO_AXIS_PTS_X 1 UWORD\nFNC_VALUES 3 UWORD ROW_DIR DIRECT");
    assert_eq!(
        e,
        AxisError::PositionGap {
            expected: 2,
            found: 3
        }
    );
    // Layout not starting at 1 is also a gap.
    assert_eq!(
        err("FNC_VALUES 2 UWORD ROW_DIR DIRECT"),
        AxisError::PositionGap {
            expected: 1,
            found: 2
        }
    );
    assert_eq!(
        err("FNC_VALUES 2 UWORD ROW_DIR DIRECT").verdict(),
        LayoutVerdict::Ambiguous
    );
}

#[test]
fn rejects_incomplete_elements() {
    assert!(matches!(
        err("FNC_VALUES 1"),
        AxisError::IncompleteElement { .. }
    ));
    // Missing position (unparseable) and missing address type.
    assert!(matches!(
        err("FNC_VALUES 1 UWORD ROW_DIR"),
        AxisError::IncompleteElement {
            missing: "address type",
            ..
        }
    ));
    assert!(matches!(
        err("AXIS_PTS_X 1 UWORD"),
        AxisError::IncompleteElement { .. }
    ));
    assert!(matches!(
        err("FIX_NO_AXIS_PTS_X"),
        AxisError::IncompleteElement { .. }
    ));
}

#[test]
fn rejects_duplicate_elements_and_fix_counts() {
    assert!(matches!(
        err("FNC_VALUES 1 UWORD ROW_DIR DIRECT\nFNC_VALUES 2 UWORD ROW_DIR DIRECT"),
        AxisError::DuplicateElement { .. }
    ));
    assert!(matches!(
        err("FIX_NO_AXIS_PTS_X 2\nFIX_NO_AXIS_PTS_X 3"),
        AxisError::DuplicateElement { .. }
    ));
}

#[test]
fn unsupported_content_is_classified_unsupported() {
    for body in [
        "FNC_VALUES 1 UWORD ROW_DIR PWORD",
        "FNC_VALUES 1 UWORD ROW_DIR DIRECT\nSRC_ADDR_X 2 UWORD",
        "FNC_VALUES 1 UWORD ROW_DIR DIRECT\nRESERVED 2 BYTE",
        "FNC_VALUES 1 WEIRD ROW_DIR DIRECT",
        "FNC_VALUES 1 UWORD ROW_DIR DIRECT\nSTATIC_RECORD_LAYOUT",
        "AXIS_RESCALE_X 1 UWORD 4 INDEX_INCR DIRECT",
    ] {
        let e = err(body);
        assert_eq!(e.verdict(), LayoutVerdict::Unsupported, "{body}: {e}");
    }
}

#[test]
fn accepts_all_fixed_size_types_with_natural_alignment() {
    let p = plan(
        "FNC_VALUES 1 A_UINT64 ROW_DIR DIRECT\nNO_AXIS_PTS_X 2 ULONG\nNO_AXIS_PTS_Y 3 UWORD\nNO_AXIS_PTS_Z 4 SBYTE\nIDENTIFICATION 5 SBYTE",
    );
    // FNC_VALUES count is unknown, so the offsets behind it are not checked yet.
    let p = p.unwrap();
    assert_eq!(p.required_base_alignment(), 8);
    let e = p.resolve(&PointCounts::new().with_fnc_values(1)).unwrap();
    assert_eq!(e.total_size, 8 + 4 + 2 + 1 + 1);
    assert!(p.check_base_address(16).is_ok());
    assert_eq!(
        p.check_base_address(0x1002),
        Err(AxisError::AddressMisaligned {
            address: 0x1002,
            required: 8
        })
    );
}

#[test]
fn padding_dependent_layouts_are_ambiguous_not_guessed() {
    // A UBYTE then a UWORD: the UWORD sits at offset 1, which would need padding.
    let e = err("NO_AXIS_PTS_X 1 UBYTE\nNO_AXIS_PTS_Y 2 UWORD");
    assert_eq!(
        e,
        AxisError::AmbiguousPadding {
            keyword: "NO_AXIS_PTS_Y".into(),
            offset: 1,
            required_alignment: 2,
            padded_offset: Some(2),
        }
    );
    assert_eq!(e.verdict(), LayoutVerdict::Ambiguous);
}

#[test]
fn ambiguity_is_detected_after_resolve_when_counts_cause_misalignment() {
    // 3 bytes of axis points then a UWORD: only resolve knows the offset is 3.
    let p = plan("AXIS_PTS_X 1 UBYTE INDEX_INCR DIRECT\nNO_AXIS_PTS_Y 2 UWORD").unwrap();
    let ok = p
        .resolve(&PointCounts::new().with_axis(RecordAxis::X, 4))
        .unwrap();
    assert_eq!(ok.total_size, 6);
    let e = p
        .resolve(&PointCounts::new().with_axis(RecordAxis::X, 3))
        .unwrap_err();
    assert!(matches!(e, AxisError::AmbiguousPadding { offset: 3, .. }));
}

#[test]
fn declared_alignment_is_honoured_and_never_adds_padding() {
    // ALIGNMENT_WORD 1 relaxes the requirement: the UWORD at offset 1 is exact.
    let p = plan("ALIGNMENT_WORD 1\nNO_AXIS_PTS_X 1 UBYTE\nNO_AXIS_PTS_Y 2 UWORD").unwrap();
    assert!(p.elements()[1].alignment_declared);
    assert_eq!(p.elements()[1].static_offset, Some(1));
    // ALIGNMENT_WORD 4 is stricter: offset 2 is not a multiple of 4 → ambiguous.
    let e = err("ALIGNMENT_WORD 4\nNO_AXIS_PTS_X 1 UWORD\nNO_AXIS_PTS_Y 2 UWORD");
    assert!(matches!(
        e,
        AxisError::AmbiguousPadding {
            offset: 2,
            required_alignment: 4,
            padded_offset: Some(4),
            ..
        }
    ));
    // Declared 4 and satisfied at offset 0.
    let p = plan("ALIGNMENT_WORD 4\nNO_AXIS_PTS_X 1 UWORD").unwrap();
    assert_eq!(p.required_base_alignment(), 4);
}

#[test]
fn mod_common_alignment_word_four_is_honoured() {
    let p = plan_with_mod_common(
        "ALIGNMENT_WORD 4",
        "NO_AXIS_PTS_X 1 UWORD\nNO_AXIS_PTS_Y 2 UWORD",
    );
    let e = p.unwrap_err();
    assert!(matches!(
        e,
        AxisError::AmbiguousPadding {
            required_alignment: 4,
            ..
        }
    ));
}

#[test]
fn rejects_record_layout_extra_arguments() {
    let e = err("FNC_VALUES 1 UWORD ROW_DIR DIRECT EXTRA");
    assert!(matches!(
        e,
        AxisError::InvalidArity {
            expected: 4,
            found: 5,
            ..
        }
    ));
    assert_eq!(e.verdict(), LayoutVerdict::Malformed);
}

#[test]
fn importer_reports_record_layout_extra_arguments() {
    let doc = parse_a2l(
        "/begin PROJECT p \"p\"\n/begin MODULE m \"m\"\n/begin RECORD_LAYOUT rl\nFNC_VALUES 1 UWORD ROW_DIR DIRECT EXTRA\n/end RECORD_LAYOUT\n/end MODULE\n/end PROJECT",
    )
    .unwrap();
    assert!(doc.has_errors());
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.message.contains("too many arguments"))
    );
}

#[test]
fn rejects_unknown_indexing_mode_and_keeps_known_mode() {
    let e = err("FNC_VALUES 1 UWORD DIAGONAL DIRECT");
    assert_eq!(e.verdict(), LayoutVerdict::Unsupported);
    let p = plan("FNC_VALUES 1 UWORD COLUMN_DIR DIRECT").unwrap();
    assert_eq!(
        p.element(ElementKind::FncValues)
            .unwrap()
            .index_mode
            .as_deref(),
        Some("COLUMN_DIR")
    );
}

#[test]
fn rejects_index_decr_but_parser_preserves_it() {
    let doc = parse_a2l(
        "/begin PROJECT p \"p\"\n/begin MODULE m \"m\"\n/begin RECORD_LAYOUT rl\nAXIS_PTS_X 1 UWORD INDEX_DECR DIRECT\n/end RECORD_LAYOUT\n/end MODULE\n/end PROJECT",
    )
    .unwrap();
    let layout = &doc.project.modules[0].record_layouts[0];
    assert_eq!(
        layout.elements[0].index_increment.as_deref(),
        Some("INDEX_DECR")
    );

    let e = RecordPlan::from_layout_with_mod_common(layout, None).unwrap_err();
    assert_eq!(e.verdict(), LayoutVerdict::Unsupported);
    assert!(matches!(
        e,
        AxisError::UnsupportedIndexMode { mode, .. } if mode == "INDEX_DECR"
    ));
}

#[test]
fn legacy_api_rejects_unknown_mod_common_context() {
    let doc = parse_a2l(
        "/begin PROJECT p \"p\"\n/begin MODULE m \"m\"\n/begin MOD_COMMON \"c\"\nALIGNMENT_WORD 4\n/end MOD_COMMON\n/begin RECORD_LAYOUT rl\nNO_AXIS_PTS_X 1 UWORD\nNO_AXIS_PTS_Y 2 UWORD\n/end RECORD_LAYOUT\n/end MODULE\n/end PROJECT",
    )
    .unwrap();
    let e = RecordPlan::from_layout(&doc.project.modules[0].record_layouts[0]).unwrap_err();
    assert_eq!(e, AxisError::UnknownModCommonContext);
    assert_eq!(e.verdict(), LayoutVerdict::Ambiguous);
}

#[test]
fn rejects_no_axis_and_fixed_axis_count_together() {
    let e = err("FIX_NO_AXIS_PTS_X 4\nNO_AXIS_PTS_X 1 UBYTE");
    assert!(matches!(
        e,
        AxisError::ConflictingAxisCountDeclarations {
            axis: RecordAxis::X
        }
    ));
}

#[test]
fn rejects_invalid_alignment_directives() {
    for body in [
        "ALIGNMENT_WORD 3\nFNC_VALUES 1 UWORD ROW_DIR DIRECT",
        "ALIGNMENT_WORD 0\nFNC_VALUES 1 UWORD ROW_DIR DIRECT",
        "ALIGNMENT_WORD 2\nALIGNMENT_WORD 2\nFNC_VALUES 1 UWORD ROW_DIR DIRECT",
        "ALIGNMENT_WORD x\nFNC_VALUES 1 UWORD ROW_DIR DIRECT",
        "ALIGNMENT_BOGUS 2\nFNC_VALUES 1 UWORD ROW_DIR DIRECT",
    ] {
        assert!(
            matches!(err(body), AxisError::InvalidAlignment { .. }),
            "{body}"
        );
    }
}

#[test]
fn point_count_helpers_are_checked() {
    assert_eq!(PointCounts::fnc_items_from_dimensions(&[2, 3]).unwrap(), 6);
    assert!(matches!(
        PointCounts::fnc_items_from_dimensions(&[u64::MAX, 2]),
        Err(AxisError::LimitExceeded { .. })
    ));
}
