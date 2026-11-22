use hmi_core::*;

fn minimal_project() -> ProjectDocument {
    ProjectDocument {
        schema_version: 1,
        id: "project.test".into(),
        name: "测试工程".into(),
        design_size: [1600, 900],
        tags: vec![TagDefinition {
            id: "bas.temp".into(),
            name: "送风温度".into(),
            value_type: TagValueType::Number,
            unit: "°C".into(),
            min: Some(0.0),
            max: Some(50.0),
            precision: 1,
            initial: TagValue::Number(20.0),
            writable: false,
            simulator: SimulatorKind::Sine,
        }],
        startup_screen: "screen.2d".into(),
        screens: vec![Screen::TwoD {
            id: "screen.2d".into(),
            name: "工艺图".into(),
            background: HexColor("#071217".into()),
            nodes: vec![UiNode {
                id: "node.value".into(),
                name: "温度".into(),
                rect: Rect {
                    x: 10.0,
                    y: 10.0,
                    width: 200.0,
                    height: 80.0,
                },
                rotation_deg: 0.0,
                z: 1,
                visible: true,
                locked: false,
                widget: Widget2D::ValueCard {
                    label: "送风温度".into(),
                },
                bindings: vec![Binding {
                    tag_id: "bas.temp".into(),
                    target: BindingTarget::Text,
                    mapping: BindingMapping::Format {
                        precision: None,
                        unit: None,
                    },
                }],
            }],
        }],
    }
}

fn messages(project: &ProjectDocument) -> Vec<String> {
    validate_project(project)
        .unwrap_err()
        .into_iter()
        .map(|issue| issue.message)
        .collect()
}

#[test]
fn accepts_minimal_valid_project() {
    validate_project(&minimal_project()).unwrap();
}

#[test]
fn rejects_bad_schema_and_design_size() {
    let mut project = minimal_project();
    project.schema_version = 2;
    project.design_size = [1920, 1080];
    let messages = messages(&project);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("schema_version"))
    );
    assert!(messages.iter().any(|message| message.contains("1600")));
}

#[test]
fn rejects_duplicate_ids_and_missing_references() {
    let mut project = minimal_project();
    project.tags.push(project.tags[0].clone());
    project.startup_screen = "missing".into();
    let Screen::TwoD { nodes, .. } = &mut project.screens[0] else {
        unreachable!()
    };
    nodes[0].bindings[0].tag_id = "missing.tag".into();
    let messages = messages(&project);
    assert!(messages.iter().any(|message| message.contains("重复")));
    assert!(
        messages
            .iter()
            .any(|message| message.contains("startup_screen"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("missing.tag"))
    );
}

#[test]
fn rejects_mismatched_initial_value_and_simulator() {
    let mut project = minimal_project();
    project.tags[0].initial = TagValue::Bool(true);
    project.tags[0].simulator = SimulatorKind::Pulse;
    let messages = messages(&project);
    assert!(messages.iter().any(|message| message.contains("initial")));
    assert!(messages.iter().any(|message| message.contains("simulator")));
}

#[test]
fn validates_glb_paths() {
    assert!(is_valid_glb_path("models/ahu.glb"));
    for bad in [
        "/models/ahu.glb",
        "../ahu.glb",
        "https://x/ahu.glb",
        "models\\ahu.glb",
        "models/ahu.png",
    ] {
        assert!(!is_valid_glb_path(bad), "{bad} should fail");
    }
}
