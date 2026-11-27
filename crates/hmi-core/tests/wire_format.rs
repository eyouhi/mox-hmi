use hmi_core::{
    Binding, BindingMapping, BindingTarget, ButtonAction, DeviceModel, DeviceNode, EditorCommand,
    EquipmentKind, HexColor, ProjectDocument, Rect, Screen, TagDefinition, TagValue, TagValueType,
    UiNode, Widget2D,
};

fn sample_project() -> ProjectDocument {
    let tag = TagDefinition {
        id: "bas.supply_fan.run".into(),
        name: "送风机运行".into(),
        value_type: TagValueType::Bool,
        unit: String::new(),
        min: None,
        max: None,
        precision: 0,
        initial: TagValue::Bool(false),
        writable: true,
        simulator: hmi_core::SimulatorKind::Manual,
    };
    let node = UiNode {
        id: "node.supply_fan".into(),
        name: "送风机 SF-01".into(),
        rect: Rect {
            x: 360.0,
            y: 220.0,
            width: 160.0,
            height: 120.0,
        },
        rotation_deg: 0.0,
        z: 10,
        visible: true,
        locked: false,
        widget: Widget2D::EquipmentSymbol {
            equipment: EquipmentKind::Fan,
            label: "SF-01".into(),
        },
        bindings: vec![Binding {
            tag_id: tag.id.clone(),
            target: BindingTarget::Color,
            mapping: BindingMapping::BoolColor {
                true_color: HexColor("#37e6d4".into()),
                false_color: HexColor("#5f7780".into()),
            },
        }],
    };
    ProjectDocument {
        schema_version: 1,
        id: "project.bas_demo".into(),
        name: "车站 BAS 环控示例".into(),
        design_size: [1600, 900],
        tags: vec![tag],
        startup_screen: "screen.bas_2d".into(),
        screens: vec![
            Screen::TwoD {
                id: "screen.bas_2d".into(),
                name: "环控工艺图".into(),
                background: HexColor("#071217".into()),
                nodes: vec![node],
            },
            Screen::ThreeD {
                id: "screen.bas_3d".into(),
                name: "环控机房".into(),
                background: HexColor("#061016".into()),
                nodes: vec![DeviceNode {
                    id: "node.ahu".into(),
                    name: "AHU-01".into(),
                    position: [0.0, 0.0, 0.0],
                    rotation_deg: [0.0, 0.0, 0.0],
                    scale: [1.0, 1.0, 1.0],
                    visible: true,
                    locked: false,
                    label: Some("AHU-01".into()),
                    model: DeviceModel::Ahu {
                        size: [3.0, 2.0, 2.0],
                        color: HexColor("#23434d".into()),
                    },
                    bindings: vec![],
                }],
            },
        ],
    }
}

#[test]
fn serializes_all_enum_values_as_snake_case() {
    let json = serde_json::to_string(&sample_project()).unwrap();
    for expected in [
        "\"kind\":\"two_d\"",
        "\"kind\":\"three_d\"",
        "\"type\":\"equipment_symbol\"",
        "\"target\":\"color\"",
        "\"type\":\"bool_color\"",
        "\"value_type\":\"bool\"",
        "\"simulator\":\"manual\"",
    ] {
        assert!(json.contains(expected), "missing {expected} in {json}");
    }
    for forbidden in [
        "\"TwoD\"",
        "\"ThreeD\"",
        "\"EquipmentSymbol\"",
        "\"BoolColor\"",
        "\"Manual\"",
    ] {
        assert!(!json.contains(forbidden), "PascalCase leaked: {forbidden}");
    }
}

#[test]
fn rejects_pascal_case_and_unknown_fields() {
    let value = serde_json::to_value(sample_project()).unwrap();
    let mut bad_kind = value.clone();
    bad_kind["screens"][0]["kind"] = "TwoD".into();
    assert!(serde_json::from_value::<ProjectDocument>(bad_kind).is_err());

    let mut unknown = value;
    unknown["unexpected"] = true.into();
    assert!(serde_json::from_value::<ProjectDocument>(unknown).is_err());
}

#[test]
fn button_action_uses_type_tag() {
    let action = ButtonAction::ToggleTag {
        tag_id: "bas.supply_fan.run".into(),
    };
    let json = serde_json::to_string(&action).unwrap();
    assert_eq!(
        json,
        r#"{"type":"toggle_tag","tag_id":"bas.supply_fan.run"}"#
    );
}

#[test]
fn complex_equipment_models_use_snake_case_tags() {
    let color = HexColor("#23434d".into());
    for (model, expected) in [
        (
            DeviceModel::Chiller {
                size: [3.0, 2.0, 2.0],
                color: color.clone(),
            },
            "\"type\":\"chiller\"",
        ),
        (
            DeviceModel::CoolingTower {
                size: [2.0, 3.0, 2.0],
                color: color.clone(),
            },
            "\"type\":\"cooling_tower\"",
        ),
        (
            DeviceModel::HeatExchanger {
                size: [2.4, 1.6, 1.2],
                color: color.clone(),
            },
            "\"type\":\"heat_exchanger\"",
        ),
        (
            DeviceModel::ControlCabinet {
                size: [1.2, 2.0, 0.8],
                color,
            },
            "\"type\":\"control_cabinet\"",
        ),
    ] {
        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains(expected), "missing {expected} in {json}");
    }
}

#[test]
fn editor_commands_round_trip_with_snake_case_types() {
    let command = EditorCommand::RenameScreen {
        screen_id: "screen.bas_2d".into(),
        name: "AI 修改后的画面".into(),
    };
    let value = serde_json::to_value(&command).unwrap();
    assert_eq!(value["type"], "rename_screen");
    assert_eq!(
        serde_json::from_value::<EditorCommand>(value).unwrap(),
        command
    );
}
