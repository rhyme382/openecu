use openecu_definitions::{RawElement, Severity, parse_a2l};

#[test]
fn accepts_asap2_version_before_project_and_preserves_root_order() {
    let document = parse_a2l(
        "ASAP2_VERSION 1 71\n\
         /begin PROJECT p \"project\"\n\
         /end PROJECT\n\
         /begin VENDOR_ROOT extension\n\
         /end VENDOR_ROOT",
    )
    .unwrap();

    assert_eq!(document.project.version.as_deref(), Some("1.71"));
    assert!(matches!(
        &document.root_elements[..],
        [RawElement::Item(item), RawElement::Block(project), RawElement::Block(vendor)]
            if item.keyword == "ASAP2_VERSION"
                && item.line == 1
                && document.raw_block(*project).is_some_and(|block| block.keyword == "PROJECT")
                && document.raw_block(*vendor).is_some_and(|block| block.keyword == "VENDOR_ROOT")
    ));
}

#[test]
fn recognizes_supported_internal_keywords_in_their_own_namespaces() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin CHARACTERISTIC characteristic "d" VALUE 1 NO_RECORD_LAYOUT 0 NO_COMPU_METHOD 0 1
            ARRAY_SIZE 4
            REF_MEMORY_SEGMENT ram
          /end CHARACTERISTIC
          /begin AXIS_PTS axis "d" 1 NO_INPUT_QUANTITY NO_RECORD_LAYOUT 0 NO_COMPU_METHOD 1 0 1
            MAX_GRAD 1
            REF_MEMORY_SEGMENT ram
          /end AXIS_PTS
          /begin MEASUREMENT measurement "d" UWORD NO_COMPU_METHOD 1 0 0 1
            READ_WRITE
            REF_MEMORY_SEGMENT ram
          /end MEASUREMENT
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    for keyword in ["ARRAY_SIZE", "MAX_GRAD", "READ_WRITE", "REF_MEMORY_SEGMENT"] {
        assert!(
            !document.diagnostics.iter().any(|diagnostic| {
                diagnostic.severity == Severity::Unsupported && diagnostic.message.contains(keyword)
            }),
            "unexpected unsupported diagnostic for {keyword}"
        );
    }
}

#[test]
fn preserves_and_diagnoses_each_uninterpreted_raw_item_once() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin CHARACTERISTIC c "d" VALUE 1 NO_RECORD_LAYOUT 0 NO_COMPU_METHOD 0 1
            VENDOR_CHARACTERISTIC alpha
          /end CHARACTERISTIC
          /begin COMPU_METHOD cm "d" IDENTICAL "%" "u"
            VENDOR_COMPU beta
          /end COMPU_METHOD
          /begin RECORD_LAYOUT rl
            VENDOR_LAYOUT gamma
          /end RECORD_LAYOUT
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    for keyword in ["VENDOR_CHARACTERISTIC", "VENDOR_COMPU", "VENDOR_LAYOUT"] {
        assert_eq!(
            document
                .diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.severity == Severity::Unsupported
                        && diagnostic.message.contains(keyword)
                })
                .count(),
            1,
            "{keyword} should be diagnosed exactly once"
        );
    }
    assert_eq!(
        document.project.modules[0].characteristics[0].extra[0].values,
        ["alpha"]
    );
}

#[test]
fn applies_compu_method_and_record_layout_keyword_namespaces_separately() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin COMPU_METHOD cm "d" IDENTICAL "%" "u"
            COEFFS_LINEAR 1 2
          /end COMPU_METHOD
          /begin RECORD_LAYOUT rl
            FNC_VALUES 0 UBYTE INDEX_INCR DIRECT
          /end RECORD_LAYOUT
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    for keyword in ["COEFFS_LINEAR", "FNC_VALUES"] {
        assert!(
            !document.diagnostics.iter().any(|diagnostic| {
                diagnostic.severity == Severity::Unsupported && diagnostic.message.contains(keyword)
            }),
            "unexpected unsupported diagnostic for {keyword}"
        );
    }
}

#[test]
fn imports_a_supported_a2l_document() {
    let document = parse_a2l(include_str!("fixtures/valid.a2l")).unwrap();

    assert_eq!(document.project.name, "demo");
    assert_eq!(document.project.version.as_deref(), Some("1.71"));
    let module = &document.project.modules[0];
    assert_eq!(module.name, "engine");
    assert_eq!(module.characteristics[0].address, Some(0x1000));
    assert_eq!(module.characteristics[0].axis_references, vec!["rpm_axis"]);
    assert_eq!(module.compu_methods[0].unit, Some("%".into()));
    assert!(!document.has_errors());
}

#[test]
fn preserves_and_reports_an_unknown_keyword() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          VENDOR_EXTENSION alpha beta
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    assert!(document.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Unsupported
            && diagnostic.message.contains("VENDOR_EXTENSION")
    }));
}

#[test]
fn reports_an_unknown_block_without_discarding_it() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin VENDOR_BLOCK ext
            VALUE 42
          /end VENDOR_BLOCK
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    let block = document.unknown_block(document.unknown_blocks[0]).unwrap();
    assert_eq!(block.keyword, "VENDOR_BLOCK");
    assert!(matches!(&block.elements[0], RawElement::Item(item) if item.values == ["42"]));
}

#[test]
fn rejects_an_unterminated_block_with_its_location() {
    let error = parse_a2l("/begin PROJECT p \"project\"").unwrap_err();

    assert!(error.to_string().contains("unterminated /begin PROJECT"));
    assert!(error.to_string().contains("1:27"));
}

#[test]
fn reports_missing_references_and_duplicate_identifiers() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin CHARACTERISTIC same "a" VALUE 0x10 missing_layout 0 missing_conversion 0 1
          /end CHARACTERISTIC
          /begin CHARACTERISTIC same "b" VALUE 0x11 missing_layout 0 missing_conversion 0 1
          /end CHARACTERISTIC
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    assert!(document.has_errors());
    assert!(
        document
            .diagnostics
            .iter()
            .any(|d| d.message.contains("duplicate identifier `same`"))
    );
    assert!(document.diagnostics.iter().any(|d| {
        d.message
            .contains("missing conversion `missing_conversion`")
    }));
    assert!(
        document
            .diagnostics
            .iter()
            .any(|d| d.message.contains("missing record layout `missing_layout`"))
    );
    assert!(
        document
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .all(|d| d.line > 0 && d.column > 0)
    );
}

#[test]
fn reports_malformed_numeric_fields() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin CHARACTERISTIC c "desc" VALUE 0xnothex NO_RECORD_LAYOUT 0 NO_COMPU_METHOD 0 1
          /end CHARACTERISTIC
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    assert!(
        document
            .diagnostics
            .iter()
            .any(|d| d.message.contains("invalid unsigned number `0xnothex`"))
    );
}

#[test]
fn accepts_a_reasonably_large_document() {
    let mut source = String::from("/begin PROJECT p \"project\"\n/begin MODULE m \"module\"\n");
    for index in 0..500 {
        source.push_str(&format!("/begin MEASUREMENT m{index} \"signal {index}\" UWORD NO_COMPU_METHOD 1 0 0 1\n/end MEASUREMENT\n"));
    }
    source.push_str("/end MODULE\n/end PROJECT\n");

    let document = parse_a2l(&source).unwrap();
    assert_eq!(document.project.modules[0].measurements.len(), 500);
}

#[test]
fn rejects_thousands_of_nested_blocks_without_panicking() {
    let mut source = String::from("/begin PROJECT p \"project\"\n");
    for _ in 0..4_000 {
        source.push_str("/begin VENDOR_EXTENSION\n");
    }
    for _ in 0..4_000 {
        source.push_str("/end VENDOR_EXTENSION\n");
    }
    source.push_str("/end PROJECT\n");

    let error = parse_a2l(&source).unwrap_err();
    assert!(error.to_string().contains("nesting limit exceeded"));
}

#[test]
fn preserves_unicode_in_words_and_quoted_strings() {
    let document = parse_a2l(
        "/begin PROJECT démo \"Projet accéléré 日本語\"\n\
         /begin MODULE moteur \"Moteur\"\n/end MODULE\n/end PROJECT",
    )
    .unwrap();

    assert_eq!(document.project.name, "démo");
    assert_eq!(
        document.project.description.as_deref(),
        Some("Projet accéléré 日本語")
    );
}

#[test]
fn preserves_unknown_nested_blocks_in_known_blocks_in_source_order() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin CHARACTERISTIC c "desc" VALUE 1 NO_RECORD_LAYOUT 0 NO_COMPU_METHOD 0 1
            /begin AXIS_DESCR
              /begin VENDOR_AXIS first second
                VENDOR_ITEM alpha
              /end VENDOR_AXIS
            /end AXIS_DESCR
            /begin VENDOR_CHARACTERISTIC third
            /end VENDOR_CHARACTERISTIC
          /end CHARACTERISTIC
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    let characteristic = &document.project.modules[0].characteristics[0];
    let axis_descr = document.raw_block(characteristic.raw_blocks[0]).unwrap();
    assert_eq!(axis_descr.keyword, "AXIS_DESCR");
    let RawElement::Block(vendor_axis_id) = axis_descr.elements[0] else {
        panic!("expected vendor block")
    };
    let vendor_axis = document.raw_block(vendor_axis_id).unwrap();
    assert_eq!(vendor_axis.keyword, "VENDOR_AXIS");
    assert_eq!(vendor_axis.arguments, vec!["first", "second"]);
    assert!(matches!(vendor_axis.elements[0], RawElement::Item(_)));
    assert_eq!(
        document
            .raw_block(characteristic.raw_blocks[1])
            .unwrap()
            .keyword,
        "VENDOR_CHARACTERISTIC"
    );
    assert!(
        document
            .diagnostics
            .iter()
            .any(|d| { d.severity == Severity::Unsupported && d.message.contains("VENDOR_AXIS") })
    );
}

#[test]
fn reports_u32_overflow_at_the_numeric_value() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin AXIS_PTS a "axis" 1 NO_INPUT_QUANTITY NO_RECORD_LAYOUT 0 NO_COMPU_METHOD 0x100000000 0 1
          /end AXIS_PTS
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    assert!(document.diagnostics.iter().any(|d| {
        d.message.contains("outside u32 range `0x100000000`") && d.line == 3 && d.column > 1
    }));
}

#[test]
fn accepts_the_u32_upper_bound() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin AXIS_PTS a "axis" 1 NO_INPUT_QUANTITY NO_RECORD_LAYOUT 0 NO_COMPU_METHOD 0xffffffff 0 1
          /end AXIS_PTS
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    assert_eq!(
        document.project.modules[0].axis_points[0].max_axis_points,
        Some(u32::MAX)
    );
    assert!(
        !document
            .diagnostics
            .iter()
            .any(|d| d.message.contains("u32 range"))
    );
}

#[test]
fn validates_conversion_and_layout_sentinels_independently_with_locations() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin CHARACTERISTIC conversion_ok "d" VALUE 1 NO_RECORD_LAYOUT 0 NO_COMPU_METHOD 0 1
          /end CHARACTERISTIC
          /begin CHARACTERISTIC conversion_wrong "d" VALUE 1 NO_RECORD_LAYOUT 0 NO_RECORD_LAYOUT 0 1
          /end CHARACTERISTIC
          /begin CHARACTERISTIC layout_wrong "d" VALUE 1 NO_COMPU_METHOD 0 NO_COMPU_METHOD 0 1
          /end CHARACTERISTIC
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    assert!(
        document
            .diagnostics
            .iter()
            .any(|d| d.message.contains("conversion_wrong") && d.line == 5)
    );
    assert!(
        document
            .diagnostics
            .iter()
            .any(|d| d.message.contains("layout_wrong") && d.line == 7)
    );
    assert!(
        !document
            .diagnostics
            .iter()
            .any(|d| d.message.contains("conversion_ok") && d.severity == Severity::Error)
    );
}

#[test]
fn accepts_known_positional_arguments_split_across_lines() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin RECORD_LAYOUT rl
          /end RECORD_LAYOUT
          /begin COMPU_METHOD cm
            "conversion" IDENTICAL "%6.2" "V"
          /end COMPU_METHOD
          /begin AXIS_PTS axis
            "axis" 1 NO_INPUT_QUANTITY rl 0 cm 16 0 100
          /end AXIS_PTS
          /begin CHARACTERISTIC c
            "characteristic" VALUE 2 rl 0 cm 0 10
          /end CHARACTERISTIC
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    let module = &document.project.modules[0];
    assert_eq!(module.compu_methods[0].unit.as_deref(), Some("V"));
    assert_eq!(module.axis_points[0].max_axis_points, Some(16));
    assert_eq!(
        module.characteristics[0].conversion_reference.as_deref(),
        Some("cm")
    );
}

#[test]
fn rejects_incomplete_multiline_declarations_without_consuming_internal_keywords() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin COMPU_METHOD cm
            "conversion" IDENTICAL
            FORMAT "%6.2"
            COEFFS 1 2 3 4 5 6
          /end COMPU_METHOD
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    assert!(document.has_errors());
    assert!(document.diagnostics.iter().any(|d| {
        d.severity == Severity::Error
            && d.message.contains("COMPU_METHOD declaration is incomplete")
    }));
    let method = &document.project.modules[0].compu_methods[0];
    assert_eq!(method.format, None);
    assert!(method.extra.iter().any(|item| item.keyword == "FORMAT"));
    assert!(method.extra.iter().any(|item| item.keyword == "COEFFS"));
}

#[test]
fn diagnoses_extra_roots_and_duplicate_mod_common_without_losing_them() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin MOD_COMMON "first"
          /end MOD_COMMON
          /begin MOD_COMMON "second"
          /end MOD_COMMON
        /end MODULE
      /end PROJECT
      /begin PROJECT p2 "second project"
      /end PROJECT
      /begin VENDOR_ROOT
      /end VENDOR_ROOT"#,
    )
    .unwrap();

    assert!(document.has_errors());
    assert!(
        document
            .diagnostics
            .iter()
            .any(|d| d.message.contains("exactly one PROJECT"))
    );
    assert!(
        document
            .diagnostics
            .iter()
            .any(|d| d.message.contains("multiple MOD_COMMON"))
    );
    assert_eq!(
        document.project.modules[0]
            .mod_common
            .as_ref()
            .and_then(|x| x.description.as_deref()),
        Some("first")
    );
    assert!(!document.project.modules[0].raw_blocks.is_empty());
    assert!(document.unknown_blocks.len() >= 2);
}

#[test]
fn preserves_deep_and_wide_unknown_block_trees_without_recursive_duplication() {
    let mut source = String::from("/begin PROJECT p \"project\"\n/begin MODULE m \"module\"\n");
    for index in 0..30 {
        source.push_str(&format!("/begin VENDOR_{index} item_{index}\n"));
    }
    source.push_str("PAYLOAD preserved\n");
    for index in (0..30).rev() {
        source.push_str(&format!("/end VENDOR_{index}\n"));
    }
    for index in 0..250 {
        source.push_str(&format!(
            "/begin WIDE_VENDOR_{index} data_{index}\n/end WIDE_VENDOR_{index}\n"
        ));
    }
    source.push_str("/end MODULE\n/end PROJECT\n");

    let document = parse_a2l(&source).unwrap();
    assert_eq!(document.unknown_blocks.len(), 251);
    assert_eq!(document.raw_blocks.len(), 282);
    let nested = document.unknown_block(document.unknown_blocks[0]).unwrap();
    assert_eq!(nested.keyword, "VENDOR_0");
    assert_eq!(nested.elements.len(), 1);
    assert!(
        document
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Unsupported)
            .count()
            >= 280
    );
}

#[test]
fn accepts_one_hundred_nested_unknown_blocks_with_data_intact() {
    let mut source = String::from("/begin PROJECT p \"project\"\n");
    for index in 0..100 {
        source.push_str(&format!("/begin DEPTH_{index}\n"));
    }
    source.push_str("LEAF value\n");
    for index in (0..100).rev() {
        source.push_str(&format!("/end DEPTH_{index}\n"));
    }
    source.push_str("/end PROJECT\n");

    let document = parse_a2l(&source).unwrap();
    assert_eq!(document.raw_blocks.len(), 101);
    let root = document.unknown_block(document.unknown_blocks[0]).unwrap();
    assert_eq!(root.keyword, "DEPTH_0");
}

#[test]
fn uses_exact_value_locations_for_axis_reference_and_max_axis_points() {
    let document = parse_a2l(
        "/begin PROJECT p \"project\"\n\
         /begin MODULE m \"module\"\n\
         /begin CHARACTERISTIC c \"desc\" VALUE 1 NO_RECORD_LAYOUT 0 NO_COMPU_METHOD 0 1\n\
         /begin AXIS_DESCR\n\
         AXIS_PTS_REF missing_axis\n\
         MAX_AXIS_PTS nope\n\
         /end AXIS_DESCR\n\
         /end CHARACTERISTIC\n\
         /end MODULE\n\
         /end PROJECT",
    )
    .unwrap();

    assert!(document.diagnostics.iter().any(|d| {
        d.message.contains("missing AXIS_PTS `missing_axis`") && d.line == 5 && d.column == 14
    }));
    assert!(document.diagnostics.iter().any(
        |d| d.message.contains("invalid unsigned number `nope`") && d.line == 6 && d.column == 14
    ));
}

#[test]
fn reports_unknown_blocks_inside_every_supported_module_declaration() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin MOD_COMMON "common" /begin VENDOR_COMMON /end VENDOR_COMMON /end MOD_COMMON
          /begin RECORD_LAYOUT rl /begin VENDOR_LAYOUT /end VENDOR_LAYOUT /end RECORD_LAYOUT
          /begin COMPU_METHOD cm "d" IDENTICAL "%" "u" /begin VENDOR_COMPU /end VENDOR_COMPU /end COMPU_METHOD
          /begin AXIS_PTS axis "a" 1 NO_INPUT_QUANTITY rl 0 cm 1 0 1 /begin VENDOR_AXIS /end VENDOR_AXIS /end AXIS_PTS
          /begin MEASUREMENT signal "s" UWORD cm 1 0 0 1 /begin VENDOR_MEASUREMENT /end VENDOR_MEASUREMENT /end MEASUREMENT
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    for keyword in [
        "VENDOR_COMMON",
        "VENDOR_LAYOUT",
        "VENDOR_COMPU",
        "VENDOR_AXIS",
        "VENDOR_MEASUREMENT",
    ] {
        assert!(
            document
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Unsupported && d.message.contains(keyword)),
            "missing diagnostic for {keyword}"
        );
    }
}

#[test]
fn preserves_all_known_declaration_arguments_in_canonical_raw_blocks() {
    let document = parse_a2l(
        "/begin PROJECT p \"project\"\n\
         /begin MODULE m \"module\"\n\
         /begin CHARACTERISTIC characteristic \"characteristic\" VALUE 0x10 layout 0.25 conversion -1.5 2.5\n\
         /end CHARACTERISTIC\n\
         /begin AXIS_PTS axis \"axis\" 0x20 input layout 0.5 conversion 16 -10 10\n\
         /end AXIS_PTS\n\
         /begin MEASUREMENT measurement \"measurement\" UWORD conversion 1 0.1 -2 3\n\
         /end MEASUREMENT\n\
         /end MODULE\n\
         /end PROJECT",
    )
    .unwrap();

    let module = &document.project.modules[0];
    for (id, expected) in [
        (
            module.characteristics[0].raw_block,
            vec![
                "characteristic",
                "characteristic",
                "VALUE",
                "0x10",
                "layout",
                "0.25",
                "conversion",
                "-1.5",
                "2.5",
            ],
        ),
        (
            module.axis_points[0].raw_block,
            vec![
                "axis",
                "axis",
                "0x20",
                "input",
                "layout",
                "0.5",
                "conversion",
                "16",
                "-10",
                "10",
            ],
        ),
        (
            module.measurements[0].raw_block,
            vec![
                "measurement",
                "measurement",
                "UWORD",
                "conversion",
                "1",
                "0.1",
                "-2",
                "3",
            ],
        ),
    ] {
        let block = document.raw_block(id).unwrap();
        assert_eq!(block.arguments, expected);
        assert_eq!(block.argument_locations.len(), block.arguments.len());
        assert!(
            block
                .argument_locations
                .iter()
                .all(|location| location.line > 0 && location.column > 0)
        );
        assert!(
            block
                .argument_locations
                .windows(2)
                .all(|locations| locations[0] <= locations[1])
        );
    }
}

#[test]
fn incomplete_declarations_preserve_internal_items_without_consuming_them() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin CHARACTERISTIC c "characteristic" VALUE 1 layout 0
            COMPU_TAB_REF table
            FORMAT "%6.2"
            COEFFS 1 2 3 4 5 6
          /end CHARACTERISTIC
          /begin AXIS_PTS a "axis" 1 input layout 0
            FORMAT "%5.1"
          /end AXIS_PTS
          /begin MEASUREMENT m "measurement" UWORD conversion 1
            FORMAT "%4.0"
          /end MEASUREMENT
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    assert!(document.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("CHARACTERISTIC declaration is incomplete")
    }));
    assert!(document.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("AXIS_PTS declaration is incomplete")
    }));
    assert!(document.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic
                .message
                .contains("MEASUREMENT declaration is incomplete")
    }));

    let module = &document.project.modules[0];
    assert_eq!(
        module.characteristics[0]
            .extra
            .iter()
            .map(|item| item.keyword.as_str())
            .collect::<Vec<_>>(),
        ["COMPU_TAB_REF", "FORMAT", "COEFFS"]
    );
    assert_eq!(module.characteristics[0].extra[0].values, ["table"]);
    assert_eq!(module.characteristics[0].extra[0].line, 4);
    assert_eq!(module.characteristics[0].extra[0].column, 13);
    assert_eq!(module.axis_points[0].extra[0].keyword, "FORMAT");
    assert_eq!(module.measurements[0].extra[0].keyword, "FORMAT");
}

#[test]
fn characteristic_uses_asap2_argument_order_with_value_at_0x1000() {
    let document = parse_a2l(
        "/begin PROJECT p \"project\"\n\
         /begin MODULE m \"module\"\n\
         /begin CHARACTERISTIC torque \"Torque\" VALUE 0x1000 NO_RECORD_LAYOUT 0 NO_COMPU_METHOD -10 500\n\
         /end CHARACTERISTIC\n\
         /end MODULE\n\
         /end PROJECT",
    )
    .unwrap();

    let characteristic = &document.project.modules[0].characteristics[0];
    assert_eq!(characteristic.characteristic_type.as_deref(), Some("VALUE"));
    assert_eq!(characteristic.address, Some(0x1000));
    assert_eq!(
        characteristic.layout_reference.as_deref(),
        Some("NO_RECORD_LAYOUT")
    );
    assert!(!document.has_errors());
}

#[test]
fn preserves_all_eight_recognized_declaration_headers_once_in_canonical_storage() {
    let document = parse_a2l(
        "/begin PROJECT p \"project\"\n\
         /begin MODULE m \"module\"\n\
         /begin MOD_COMMON \"common\" /end MOD_COMMON\n\
         /begin RECORD_LAYOUT rl /end RECORD_LAYOUT\n\
         /begin COMPU_METHOD cm \"conversion\" IDENTICAL \"%6.2\" \"V\" /end COMPU_METHOD\n\
         /begin AXIS_PTS axis \"axis\" 1 NO_INPUT_QUANTITY rl 0 cm 1 0 1 /end AXIS_PTS\n\
         /begin CHARACTERISTIC characteristic \"characteristic\" VALUE 0x10 rl 0 cm 0 1 /end CHARACTERISTIC\n\
         /begin MEASUREMENT measurement \"measurement\" UWORD cm 1 0 0 1 /end MEASUREMENT\n\
         /end MODULE\n\
         /end PROJECT",
    )
    .unwrap();

    let expected = [
        ("PROJECT", vec!["p", "project"]),
        ("MODULE", vec!["m", "module"]),
        ("MOD_COMMON", vec!["common"]),
        ("RECORD_LAYOUT", vec!["rl"]),
        (
            "COMPU_METHOD",
            vec!["cm", "conversion", "IDENTICAL", "%6.2", "V"],
        ),
        (
            "AXIS_PTS",
            vec![
                "axis",
                "axis",
                "1",
                "NO_INPUT_QUANTITY",
                "rl",
                "0",
                "cm",
                "1",
                "0",
                "1",
            ],
        ),
        (
            "CHARACTERISTIC",
            vec![
                "characteristic",
                "characteristic",
                "VALUE",
                "0x10",
                "rl",
                "0",
                "cm",
                "0",
                "1",
            ],
        ),
        (
            "MEASUREMENT",
            vec![
                "measurement",
                "measurement",
                "UWORD",
                "cm",
                "1",
                "0",
                "0",
                "1",
            ],
        ),
    ];
    for (keyword, arguments) in expected {
        let blocks = document
            .raw_blocks
            .iter()
            .filter(|block| block.keyword == keyword)
            .collect::<Vec<_>>();
        assert_eq!(blocks.len(), 1, "{keyword} should be stored once");
        assert_eq!(blocks[0].arguments, arguments);
        assert_eq!(
            blocks[0].argument_locations.len(),
            blocks[0].arguments.len()
        );
    }
}

#[test]
fn incomplete_declarations_keep_known_internal_keywords_and_vendor_items() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin CHARACTERISTIC c "characteristic" VALUE 1
            GUARD_RAILS 1 2
            CALIBRATION_ACCESS CALIBRATION
            COMPARISON_QUANTITY quantity
            FUNCTION_LIST function
            MAX_REFRESH 42
            NUMBER 7
            VENDOR_EXTENSION preserved
          /end CHARACTERISTIC
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    let items = &document.project.modules[0].characteristics[0].extra;
    for keyword in [
        "GUARD_RAILS",
        "CALIBRATION_ACCESS",
        "COMPARISON_QUANTITY",
        "FUNCTION_LIST",
        "MAX_REFRESH",
        "NUMBER",
        "VENDOR_EXTENSION",
    ] {
        assert!(
            items.iter().any(|item| item.keyword == keyword),
            "{keyword}"
        );
    }
    assert_eq!(items[0].values, ["1", "2"]);
    assert!(items.iter().all(|item| item.line > 0 && item.column > 0));
}

#[test]
fn reports_each_unknown_block_once() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin VENDOR_OUTER
            /begin VENDOR_INNER
            /end VENDOR_INNER
          /end VENDOR_OUTER
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    for keyword in ["VENDOR_OUTER", "VENDOR_INNER"] {
        assert_eq!(
            document
                .diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.severity == Severity::Unsupported
                        && diagnostic.message.contains(keyword)
                })
                .count(),
            1,
            "{keyword}"
        );
    }
}

#[test]
fn diagnoses_nested_vendor_blocks_in_a_characteristic_once_with_exact_locations() {
    let document = parse_a2l(
        r#"/begin PROJECT p "project"
        /begin MODULE m "module"
          /begin CHARACTERISTIC c "description" VALUE 1 NO_RECORD_LAYOUT 0 NO_COMPU_METHOD 0 1
            /begin VENDOR_OUTER outer
              /begin VENDOR_INNER inner
                /begin VENDOR_LEAF leaf
                /end VENDOR_LEAF
              /end VENDOR_INNER
            /end VENDOR_OUTER
          /end CHARACTERISTIC
        /end MODULE
      /end PROJECT"#,
    )
    .unwrap();

    for (keyword, line, column) in [
        ("VENDOR_OUTER", 4, 20),
        ("VENDOR_INNER", 5, 22),
        ("VENDOR_LEAF", 6, 24),
    ] {
        let diagnostics = document
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.severity == Severity::Unsupported && diagnostic.message.contains(keyword)
            })
            .collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 1, "{keyword}");
        assert_eq!((diagnostics[0].line, diagnostics[0].column), (line, column));
    }

    let characteristic = &document.project.modules[0].characteristics[0];
    let outer = characteristic.raw_blocks[0];
    let RawElement::Block(inner) = document.raw_block(outer).unwrap().elements[0] else {
        panic!("expected VENDOR_INNER raw block")
    };
    let RawElement::Block(leaf) = document.raw_block(inner).unwrap().elements[0] else {
        panic!("expected VENDOR_LEAF raw block")
    };
    assert_eq!(document.raw_block(outer).unwrap().keyword, "VENDOR_OUTER");
    assert_eq!(document.raw_block(inner).unwrap().keyword, "VENDOR_INNER");
    assert_eq!(document.raw_block(leaf).unwrap().keyword, "VENDOR_LEAF");
}

#[test]
fn reports_eof_location_for_incomplete_begin_and_end() {
    for (source, expected) in [("/begin", "1:7"), ("/end", "1:5")] {
        let error = parse_a2l(source).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }

    let source = "/begin PROJECT p \"project\"";
    let error = parse_a2l(source).unwrap_err();
    assert!(error.to_string().contains("1:27"), "{error}");
}
