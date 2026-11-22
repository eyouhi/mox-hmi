use hmi_core::*;

fn tag(value_type: TagValueType, unit: &str) -> TagDefinition {
    TagDefinition {
        id: "tag.value".into(),
        name: "值".into(),
        value_type,
        unit: unit.into(),
        min: Some(0.0),
        max: Some(100.0),
        precision: 1,
        initial: match value_type {
            TagValueType::Number => TagValue::Number(25.0),
            TagValueType::Bool => TagValue::Bool(true),
            TagValueType::Enum => TagValue::Enum("run".into()),
        },
        writable: false,
        simulator: SimulatorKind::Constant,
    }
}

fn binding(target: BindingTarget, mapping: BindingMapping, tag_id: &str) -> Binding {
    Binding {
        tag_id: tag_id.into(),
        target,
        mapping,
    }
}

#[test]
fn format_inherits_tag_precision_and_unit() {
    let tag = tag(TagValueType::Number, "°C");
    let patch = resolve_binding(
        "n",
        &binding(
            BindingTarget::Text,
            BindingMapping::Format {
                precision: None,
                unit: None,
            },
            "tag.value",
        ),
        &tag,
        &TagValue::Number(23.456),
    )
    .unwrap();
    assert_eq!(
        patch,
        VisualPatch {
            node_id: "n".into(),
            value: VisualValue::Text("23.5 °C".into())
        }
    );
}

#[test]
fn format_overrides_and_handles_bool_and_enum() {
    let numeric = tag(TagValueType::Number, "Hz");
    let patch = resolve_binding(
        "n",
        &binding(
            BindingTarget::Text,
            BindingMapping::Format {
                precision: Some(0),
                unit: Some("rpm".into()),
            },
            "tag.value",
        ),
        &numeric,
        &TagValue::Number(12.6),
    )
    .unwrap();
    assert_eq!(patch.value, VisualValue::Text("13 rpm".into()));

    let boolean = tag(TagValueType::Bool, "");
    let patch = resolve_binding(
        "n",
        &binding(
            BindingTarget::Text,
            BindingMapping::Format {
                precision: None,
                unit: None,
            },
            "tag.value",
        ),
        &boolean,
        &TagValue::Bool(true),
    )
    .unwrap();
    assert_eq!(patch.value, VisualValue::Text("运行".into()));
}

#[test]
fn bool_and_threshold_colors_select_exact_stops() {
    let boolean = tag(TagValueType::Bool, "");
    let bool_patch = resolve_binding(
        "n",
        &binding(
            BindingTarget::Color,
            BindingMapping::BoolColor {
                true_color: HexColor("#00ff00".into()),
                false_color: HexColor("#ff0000".into()),
            },
            "tag.value",
        ),
        &boolean,
        &TagValue::Bool(false),
    )
    .unwrap();
    assert_eq!(
        bool_patch.value,
        VisualValue::Color(HexColor("#ff0000".into()))
    );

    let numeric = tag(TagValueType::Number, "");
    let mapping = BindingMapping::ThresholdColor {
        thresholds: vec![
            ThresholdStop {
                value: 10.0,
                color: HexColor("#00ff00".into()),
            },
            ThresholdStop {
                value: 20.0,
                color: HexColor("#ffcc00".into()),
            },
            ThresholdStop {
                value: 30.0,
                color: HexColor("#ff0000".into()),
            },
        ],
    };
    let at_25 = resolve_binding(
        "n",
        &binding(BindingTarget::Color, mapping.clone(), "tag.value"),
        &numeric,
        &TagValue::Number(25.0),
    )
    .unwrap();
    assert_eq!(at_25.value, VisualValue::Color(HexColor("#ffcc00".into())));
    let below = resolve_binding(
        "n",
        &binding(BindingTarget::Color, mapping, "tag.value"),
        &numeric,
        &TagValue::Number(1.0),
    )
    .unwrap();
    assert_eq!(below.value, VisualValue::Color(HexColor("#00ff00".into())));
}

#[test]
fn range_clamps_and_direct_coerces_targets() {
    let numeric = tag(TagValueType::Number, "");
    let range = BindingMapping::Range {
        input_min: 0.0,
        input_max: 50.0,
        output_min: 0.0,
        output_max: 1.0,
        clamp: true,
    };
    let high = resolve_binding(
        "n",
        &binding(BindingTarget::Progress, range, "tag.value"),
        &numeric,
        &TagValue::Number(75.0),
    )
    .unwrap();
    assert_eq!(high.value, VisualValue::Progress(1.0));
    let direct = resolve_binding(
        "n",
        &binding(
            BindingTarget::RotationSpeed,
            BindingMapping::Direct,
            "tag.value",
        ),
        &numeric,
        &TagValue::Number(12.0),
    )
    .unwrap();
    assert_eq!(direct.value, VisualValue::RotationSpeed(12.0));
    let boolean = tag(TagValueType::Bool, "");
    let visible = resolve_binding(
        "n",
        &binding(BindingTarget::Visible, BindingMapping::Direct, "tag.value"),
        &boolean,
        &TagValue::Bool(true),
    )
    .unwrap();
    assert_eq!(visible.value, VisualValue::Visible(true));
}

#[test]
fn invalid_target_mapping_is_rejected() {
    let numeric = tag(TagValueType::Number, "");
    let result = resolve_binding(
        "n",
        &binding(BindingTarget::Text, BindingMapping::Direct, "tag.value"),
        &numeric,
        &TagValue::Number(1.0),
    );
    assert!(result.is_err());
}

#[test]
fn resolve_bindings_keeps_order_and_skips_unknown_tags() {
    let number = tag(TagValueType::Number, "°C");
    let tags = std::collections::BTreeMap::from([(number.id.clone(), number)]);
    let snapshot = TagSnapshot::new(
        std::collections::BTreeMap::from([("tag.value".to_string(), TagValue::Number(21.0))]),
        1,
    );
    let bindings = vec![
        binding(
            BindingTarget::Text,
            BindingMapping::Format {
                precision: None,
                unit: None,
            },
            "tag.value",
        ),
        binding(
            BindingTarget::Text,
            BindingMapping::Format {
                precision: None,
                unit: None,
            },
            "missing",
        ),
    ];
    let patches = resolve_bindings("n", &bindings, &tags, &snapshot);
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].value, VisualValue::Text("21.0 °C".into()));
}
