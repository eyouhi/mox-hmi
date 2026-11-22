use crate::bridge;
use hmi_core::{BindingTarget, DeviceModel, HmiError, TagValueType, Widget2D};

pub fn sanitize_test_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

pub fn focus_canvas() {
    #[cfg(target_arch = "wasm32")]
    let _ = dioxus::document::eval(
        r##"document.querySelector("#bevy-canvas")?.focus({ preventScroll: true })"##,
    );
}

pub fn format_number(value: f32) -> String {
    if value.fract().abs() < 0.0001 {
        format!("{value:.0}")
    } else {
        let text = format!("{value:.3}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn widget_label(widget: &Widget2D) -> &'static str {
    match widget {
        Widget2D::Panel { .. } => "Panel",
        Widget2D::Label { .. } => "Label",
        Widget2D::ValueCard { .. } => "ValueCard",
        Widget2D::StatusLamp { .. } => "StatusLamp",
        Widget2D::BarGauge { .. } => "BarGauge",
        Widget2D::Button { .. } => "Button",
        Widget2D::Pipe { .. } => "Pipe",
        Widget2D::EquipmentSymbol { .. } => "EquipmentSymbol",
    }
}

pub fn model_label(model: &DeviceModel) -> &'static str {
    match model {
        DeviceModel::Ahu { .. } => "3D AHU",
        DeviceModel::Fan { .. } => "3D Fan",
        DeviceModel::Pump { .. } => "3D Pump",
        DeviceModel::Duct { .. } => "3D Duct",
        DeviceModel::Pipe { .. } => "3D Pipe",
        DeviceModel::Valve { .. } => "3D Valve",
        DeviceModel::Glb { .. } => "3D GLB",
    }
}

pub fn valid_targets(widget: &Widget2D) -> &'static [BindingTarget] {
    match widget {
        Widget2D::Label { .. } | Widget2D::ValueCard { .. } => &[
            BindingTarget::Text,
            BindingTarget::Color,
            BindingTarget::Visible,
        ],
        Widget2D::StatusLamp { .. } => &[BindingTarget::Color, BindingTarget::Visible],
        Widget2D::BarGauge { .. } => &[BindingTarget::Progress, BindingTarget::Visible],
        Widget2D::Button { .. } => &[BindingTarget::Visible],
        Widget2D::Panel { .. } | Widget2D::Pipe { .. } => {
            &[BindingTarget::Color, BindingTarget::Visible]
        }
        Widget2D::EquipmentSymbol { .. } => &[
            BindingTarget::Color,
            BindingTarget::Visible,
            BindingTarget::Progress,
            BindingTarget::RotationSpeed,
        ],
    }
}

pub fn valid_targets_3d() -> &'static [BindingTarget] {
    &[
        BindingTarget::Text,
        BindingTarget::Color,
        BindingTarget::Visible,
        BindingTarget::Progress,
        BindingTarget::RotationSpeed,
    ]
}

pub fn target_label(target: BindingTarget) -> &'static str {
    match target {
        BindingTarget::Text => "文本",
        BindingTarget::Color => "颜色",
        BindingTarget::Visible => "可见",
        BindingTarget::Progress => "进度",
        BindingTarget::RotationSpeed => "转速",
    }
}

pub fn mapping_label(mapping: &hmi_core::BindingMapping) -> &'static str {
    match mapping {
        hmi_core::BindingMapping::Format { .. } => "格式化",
        hmi_core::BindingMapping::BoolColor { .. } => "布尔颜色",
        hmi_core::BindingMapping::ThresholdColor { .. } => "阈值颜色",
        hmi_core::BindingMapping::Range { .. } => "范围映射",
        hmi_core::BindingMapping::Direct => "直接",
    }
}

pub fn legal_mappings(target: BindingTarget, tag_type: TagValueType) -> Vec<&'static str> {
    match (target, tag_type) {
        (BindingTarget::Text, _) => vec!["format"],
        (BindingTarget::Color, TagValueType::Bool) => vec!["bool_color", "direct"],
        (BindingTarget::Color, TagValueType::Number) => vec!["threshold_color", "direct"],
        (BindingTarget::Color, TagValueType::Enum) => vec!["direct"],
        (BindingTarget::Visible | BindingTarget::Progress | BindingTarget::RotationSpeed, _) => {
            vec!["direct", "range"]
        }
    }
}

pub fn default_mapping_for_kind(
    target: BindingTarget,
    tag: &hmi_core::TagDefinition,
    kind: &str,
) -> hmi_core::BindingMapping {
    match (target, kind, tag.value_type) {
        (BindingTarget::Color, "bool_color", _) => hmi_core::BindingMapping::BoolColor {
            true_color: hmi_core::HexColor("#37e6d4".to_string()),
            false_color: hmi_core::HexColor("#31424b".to_string()),
        },
        (BindingTarget::Color, "threshold_color", _) => hmi_core::BindingMapping::ThresholdColor {
            thresholds: vec![hmi_core::ThresholdStop {
                value: tag.min.unwrap_or(0.0),
                color: hmi_core::HexColor("#f2b84b".to_string()),
            }],
        },
        (BindingTarget::Text, _, _) => hmi_core::BindingMapping::Format {
            precision: None,
            unit: None,
        },
        (BindingTarget::Progress | BindingTarget::RotationSpeed, "range", _) => {
            hmi_core::BindingMapping::Range {
                input_min: tag.min.unwrap_or(0.0),
                input_max: tag.max.unwrap_or(100.0),
                output_min: 0.0,
                output_max: if target == BindingTarget::RotationSpeed {
                    15.0
                } else {
                    1.0
                },
                clamp: true,
            }
        }
        _ => hmi_core::BindingMapping::Direct,
    }
}

pub fn execute_command(command: hmi_core::EditorCommand, success: Option<&str>) {
    match bridge::execute(command) {
        Ok(_) => {
            if let Some(message) = success {
                bridge::set_notice(bridge::NoticeLevel::Info, message);
            }
        }
        Err(error) => bridge::set_notice(bridge::NoticeLevel::Error, format_hmi_error(&error)),
    }
}

pub fn report_error(error: impl ToString) {
    bridge::set_notice(bridge::NoticeLevel::Error, error.to_string());
}

pub fn format_hmi_error(error: &HmiError) -> String {
    match error {
        HmiError::TagInUse { tag_id, nodes } => {
            format!("Tag `{tag_id}` 正被以下节点引用：{}", nodes.join("、"))
        }
        _ => error.to_string(),
    }
}

pub fn selected_node_locked() -> bool {
    let selected = bridge::selected_nodes();
    if selected.is_empty() {
        return false;
    }
    let Some(screen) = bridge::active_screen_snapshot() else {
        return false;
    };
    selected.iter().all(|selected| match &screen {
        hmi_core::Screen::TwoD { nodes, .. } => nodes
            .iter()
            .find(|node| &node.id == selected)
            .is_some_and(|node| node.locked),
        hmi_core::Screen::ThreeD { nodes, .. } => nodes
            .iter()
            .find(|node| &node.id == selected)
            .is_some_and(|node| node.locked),
    })
}

pub fn node_kind_label(project: &hmi_core::ProjectDocument, node_id: &str) -> String {
    for screen in &project.screens {
        match screen {
            hmi_core::Screen::TwoD { nodes, .. } => {
                if let Some(node) = nodes.iter().find(|node| node.id == node_id) {
                    return format!("2D {}", widget_label(&node.widget));
                }
            }
            hmi_core::Screen::ThreeD { nodes, .. } => {
                if let Some(node) = nodes.iter().find(|node| node.id == node_id) {
                    return model_label(&node.model).to_string();
                }
            }
        }
    }
    "未知节点".to_string()
}
