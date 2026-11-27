use crate::{
    Binding, BindingMapping, BindingTarget, ButtonAction, DeviceModel, HexColor, ProjectDocument,
    SCHEMA_VERSION, Screen, SimulatorKind, TagDefinition, TagValue, TagValueType, UiNode, Widget2D,
};
use std::collections::{HashMap, HashSet};

pub const MAX_TAGS: usize = 300;
pub const MAX_NODES_2D: usize = 200;
pub const MAX_NODES_3D: usize = 80;
pub const MAX_IMPORT_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

fn issue(path: impl Into<String>, message: impl Into<String>) -> ValidationIssue {
    ValidationIssue {
        path: path.into(),
        message: message.into(),
    }
}

pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
}

pub fn is_valid_glb_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    !path.is_empty()
        && path.len() <= 256
        && lower.ends_with(".glb")
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path
            .split(['/', '\\'])
            .any(|part| part == ".." || part == ".")
        && !lower.starts_with("http:")
        && !lower.starts_with("https:")
}

pub fn validate_tag_value(tag: &TagDefinition) -> Result<(), ValidationIssue> {
    let initial_path = format!("tags[{}].initial", tag.id);
    if !tag.initial.matches_type(tag.value_type) {
        return Err(issue(initial_path, "initial 与 value_type 不一致"));
    }

    let simulator_ok = match tag.simulator {
        SimulatorKind::Sine | SimulatorKind::RandomWalk | SimulatorKind::Step => {
            tag.value_type == TagValueType::Number
        }
        SimulatorKind::Pulse => tag.value_type == TagValueType::Bool,
        SimulatorKind::Constant | SimulatorKind::Manual => true,
    };
    if !simulator_ok {
        return Err(issue(
            format!("tags[{}].simulator", tag.id),
            "simulator 与 value_type 不兼容",
        ));
    }

    if tag.precision > 6 {
        return Err(issue(
            format!("tags[{}].precision", tag.id),
            "precision 不能超过 6",
        ));
    }

    match tag.value_type {
        TagValueType::Number => {
            let min = tag
                .min
                .ok_or_else(|| issue(format!("tags[{}].min", tag.id), "Number Tag 必须提供 min"))?;
            let max = tag
                .max
                .ok_or_else(|| issue(format!("tags[{}].max", tag.id), "Number Tag 必须提供 max"))?;
            if !min.is_finite() || !max.is_finite() || min >= max {
                return Err(issue(
                    format!("tags[{}]", tag.id),
                    "min/max 必须是有限值且 min < max",
                ));
            }
            let TagValue::Number(initial) = &tag.initial else {
                return Err(issue(
                    format!("tags[{}].initial", tag.id),
                    "initial 与 value_type 不一致",
                ));
            };
            if !initial.is_finite() {
                return Err(issue(
                    format!("tags[{}].initial", tag.id),
                    "initial 必须是有限数值",
                ));
            }
            if *initial < min || *initial > max {
                return Err(issue(
                    format!("tags[{}].initial", tag.id),
                    "initial 必须位于 min/max 之间",
                ));
            }
        }
        TagValueType::Bool | TagValueType::Enum => {
            if tag.min.is_some() || tag.max.is_some() {
                return Err(issue(
                    format!("tags[{}]", tag.id),
                    "Bool/Enum Tag 不应设置 min/max",
                ));
            }
            if let TagValue::Enum(value) = &tag.initial
                && (value.trim().is_empty() || value.len() > 120)
            {
                return Err(issue(
                    format!("tags[{}].initial", tag.id),
                    "Enum 初始值不能为空且不能超过 120 字符",
                ));
            }
        }
    }
    Ok(())
}

fn check_color(errors: &mut Vec<ValidationIssue>, path: &str, color: &HexColor) {
    if color.parse_rgb().is_none() {
        errors.push(issue(path, "颜色必须是 #RRGGBB"));
    }
}

fn check_finite(errors: &mut Vec<ValidationIssue>, path: &str, value: f32) {
    if !value.is_finite() {
        errors.push(issue(path, "必须是有限数值"));
    }
}

fn check_non_empty(errors: &mut Vec<ValidationIssue>, path: &str, value: &str, label: &str) {
    if value.trim().is_empty() {
        errors.push(issue(path, format!("{label}不能为空")));
    } else if value.chars().count() > 120 {
        errors.push(issue(path, format!("{label}不能超过 120 字符")));
    }
}

fn validate_action(
    errors: &mut Vec<ValidationIssue>,
    path: &str,
    action: &ButtonAction,
    tags: &HashMap<String, &TagDefinition>,
) {
    let (tag_id, value) = match action {
        ButtonAction::ToggleTag { tag_id } => (tag_id, None),
        ButtonAction::SetTag { tag_id, value } => (tag_id, Some(value)),
    };
    let Some(tag) = tags.get(tag_id) else {
        errors.push(issue(path, format!("按钮引用的 Tag `{tag_id}` 不存在")));
        return;
    };
    if !tag.writable {
        errors.push(issue(path, format!("按钮 Tag `{tag_id}` 不可写")));
    }
    if let Some(value) = value {
        if !value.matches_type(tag.value_type) {
            errors.push(issue(path, "SetTag 的值类型与 Tag 不一致"));
        }
    } else if tag.value_type != TagValueType::Bool {
        errors.push(issue(path, "ToggleTag 只支持 Bool Tag"));
    }
}

fn validate_binding(
    errors: &mut Vec<ValidationIssue>,
    path: &str,
    binding: &Binding,
    tags: &HashMap<String, &TagDefinition>,
) {
    let Some(tag) = tags.get(&binding.tag_id) else {
        errors.push(issue(
            path,
            format!("绑定的 Tag `{}` 不存在", binding.tag_id),
        ));
        return;
    };

    let legal = match (binding.target, &binding.mapping) {
        (BindingTarget::Text, BindingMapping::Format { .. }) => true,
        (BindingTarget::Color, BindingMapping::BoolColor { .. }) => {
            tag.value_type == TagValueType::Bool
        }
        (BindingTarget::Color, BindingMapping::ThresholdColor { .. }) => {
            tag.value_type == TagValueType::Number
        }
        (BindingTarget::Visible, BindingMapping::Direct) => true,
        (BindingTarget::Progress | BindingTarget::RotationSpeed, BindingMapping::Direct) => true,
        (BindingTarget::Progress | BindingTarget::RotationSpeed, BindingMapping::Range { .. }) => {
            tag.value_type == TagValueType::Number
        }
        _ => false,
    };
    if !legal {
        errors.push(issue(
            path,
            format!(
                "绑定目标 {:?} 与映射类型或 Tag `{}` 的类型不兼容",
                binding.target, binding.tag_id
            ),
        ));
    }

    match &binding.mapping {
        BindingMapping::Format { precision, unit } => {
            if precision.is_some_and(|precision| precision > 6) {
                errors.push(issue(path, "Format precision 不能超过 6"));
            }
            if unit.as_ref().is_some_and(|unit| unit.len() > 32) {
                errors.push(issue(path, "Format unit 不能超过 32 字节"));
            }
        }
        BindingMapping::BoolColor {
            true_color,
            false_color,
        } => {
            check_color(errors, path, true_color);
            check_color(errors, path, false_color);
        }
        BindingMapping::ThresholdColor { thresholds } => {
            if thresholds.is_empty() || thresholds.len() > 8 {
                errors.push(issue(path, "ThresholdColor 需要 1–8 个阈值"));
            }
            let mut previous = f32::NEG_INFINITY;
            for (index, stop) in thresholds.iter().enumerate() {
                let stop_path = format!("{path}.thresholds[{index}]");
                check_finite(errors, &stop_path, stop.value);
                check_color(errors, &stop_path, &stop.color);
                if stop.value <= previous {
                    errors.push(issue(&stop_path, "thresholds 必须按 value 严格升序排列"));
                }
                previous = stop.value;
            }
        }
        BindingMapping::Range {
            input_min,
            input_max,
            output_min,
            output_max,
            ..
        } => {
            for (name, value) in [
                ("input_min", *input_min),
                ("input_max", *input_max),
                ("output_min", *output_min),
                ("output_max", *output_max),
            ] {
                check_finite(errors, &format!("{path}.{name}"), value);
            }
            if *input_min == *input_max {
                errors.push(issue(path, "Range 的 input_min 不能等于 input_max"));
            }
        }
        BindingMapping::Direct => {}
    }
}

fn validate_device_model(errors: &mut Vec<ValidationIssue>, path: &str, model: &DeviceModel) {
    fn positive_vec(errors: &mut Vec<ValidationIssue>, path: &str, size: [f32; 3]) {
        for (index, value) in size.iter().enumerate() {
            if !value.is_finite() || *value <= 0.0 {
                errors.push(issue(
                    format!("{path}.size[{index}]"),
                    "尺寸必须是大于 0 的有限数值",
                ));
            }
        }
    }

    fn positive(errors: &mut Vec<ValidationIssue>, path: &str, value: f32) {
        if !value.is_finite() || value <= 0.0 {
            errors.push(issue(path, "尺寸必须是大于 0 的有限数值"));
        }
    }

    match model {
        DeviceModel::Ahu { size, color }
        | DeviceModel::Fan { size, color }
        | DeviceModel::Pump { size, color }
        | DeviceModel::Chiller { size, color }
        | DeviceModel::CoolingTower { size, color }
        | DeviceModel::HeatExchanger { size, color }
        | DeviceModel::ControlCabinet { size, color } => {
            positive_vec(errors, path, *size);
            check_color(errors, path, color);
        }
        DeviceModel::Duct {
            length,
            diameter,
            color,
        }
        | DeviceModel::Pipe {
            length,
            diameter,
            color,
        } => {
            positive(errors, &format!("{path}.length"), *length);
            positive(errors, &format!("{path}.diameter"), *diameter);
            check_color(errors, path, color);
        }
        DeviceModel::Valve { size, color } => {
            positive(errors, &format!("{path}.size"), *size);
            check_color(errors, path, color);
        }
        DeviceModel::Glb { path: glb_path } => {
            if !is_valid_glb_path(glb_path) {
                errors.push(issue(path, "GLB 路径必须是同源相对路径且以 .glb 结尾"));
            }
        }
    }
}

fn validate_ui_node(
    errors: &mut Vec<ValidationIssue>,
    path: &str,
    node: &UiNode,
    tags: &HashMap<String, &TagDefinition>,
) {
    check_non_empty(errors, path, &node.name, "节点名称");
    check_finite(errors, path, node.rect.x);
    check_finite(errors, path, node.rect.y);
    check_finite(errors, path, node.rect.width);
    check_finite(errors, path, node.rect.height);
    if !node.rect.width.is_finite() || !node.rect.height.is_finite() {
        return;
    }
    if node.rect.width <= 0.0 || node.rect.height <= 0.0 {
        errors.push(issue(path, "节点 width/height 必须大于 0"));
    }
    check_finite(errors, path, node.rotation_deg);

    let widget_path = format!("{path}.widget");
    match &node.widget {
        Widget2D::Panel {
            fill,
            stroke,
            stroke_width,
        } => {
            check_color(errors, &widget_path, fill);
            check_color(errors, &widget_path, stroke);
            check_finite(errors, &widget_path, *stroke_width);
            if *stroke_width < 0.0 {
                errors.push(issue(&widget_path, "stroke_width 不能为负"));
            }
        }
        Widget2D::Label {
            text,
            font_size,
            color,
            ..
        } => {
            if text.chars().count() > 500 {
                errors.push(issue(&widget_path, "Label 文本不能超过 500 字符"));
            }
            check_finite(errors, &widget_path, *font_size);
            if *font_size < 8.0 || *font_size > 200.0 {
                errors.push(issue(&widget_path, "font_size 必须在 8–200 之间"));
            }
            check_color(errors, &widget_path, color);
        }
        Widget2D::ValueCard { label } | Widget2D::StatusLamp { label } => {
            check_non_empty(errors, &widget_path, label, "标签");
        }
        Widget2D::BarGauge { min, max, .. } => {
            check_finite(errors, &widget_path, *min);
            check_finite(errors, &widget_path, *max);
            if *min >= *max {
                errors.push(issue(&widget_path, "BarGauge 的 min 必须小于 max"));
            }
        }
        Widget2D::Button { label, action } => {
            check_non_empty(errors, &widget_path, label, "按钮文本");
            validate_action(errors, &format!("{widget_path}.action"), action, tags);
        }
        Widget2D::Pipe {
            points,
            thickness,
            color,
        } => {
            if points.len() < 2 || points.len() > 64 {
                errors.push(issue(&widget_path, "Pipe 需要 2–64 个点"));
            }
            for (index, point) in points.iter().enumerate() {
                for (axis, value) in point.iter().enumerate() {
                    check_finite(
                        errors,
                        &format!("{widget_path}.points[{index}][{axis}]"),
                        *value,
                    );
                }
            }
            check_finite(errors, &widget_path, *thickness);
            if *thickness <= 0.0 {
                errors.push(issue(&widget_path, "Pipe thickness 必须大于 0"));
            }
            check_color(errors, &widget_path, color);
        }
        Widget2D::EquipmentSymbol { label, .. } => {
            check_non_empty(errors, &widget_path, label, "设备标签");
        }
    }

    for binding in &node.bindings {
        validate_binding(errors, &format!("{path}.bindings"), binding, tags);
    }
}

pub fn validate_project(project: &ProjectDocument) -> Result<(), Vec<ValidationIssue>> {
    let mut errors = Vec::new();

    if project.schema_version != SCHEMA_VERSION {
        errors.push(issue(
            "schema_version",
            format!("只支持 schema_version {SCHEMA_VERSION}"),
        ));
    }
    if project.design_size != [1600, 900] {
        errors.push(issue("design_size", "设计空间必须固定为 1600 × 900"));
    }
    if !is_valid_id(&project.id) {
        errors.push(issue("id", "工程 ID 非法"));
    }
    check_non_empty(&mut errors, "name", &project.name, "工程名称");
    if project.tags.len() > MAX_TAGS {
        errors.push(issue("tags", format!("工程最多 {MAX_TAGS} 个 Tag")));
    }

    let mut tags = HashMap::new();
    for (index, tag) in project.tags.iter().enumerate() {
        let path = format!("tags[{index}]");
        if !is_valid_id(&tag.id) {
            errors.push(issue(&path, "Tag ID 非法"));
        }
        if tags.insert(tag.id.clone(), tag).is_some() {
            errors.push(issue(&path, format!("Tag ID `{}` 重复", tag.id)));
        }
        check_non_empty(&mut errors, &path, &tag.name, "Tag 名称");
        if tag.unit.len() > 32 {
            errors.push(issue(&path, "Tag unit 不能超过 32 字节"));
        }
        if !tag.initial.matches_type(tag.value_type) {
            errors.push(issue(
                format!("{path}.initial"),
                "initial 与 value_type 不一致",
            ));
        }
        let simulator_ok = match tag.simulator {
            SimulatorKind::Sine | SimulatorKind::RandomWalk | SimulatorKind::Step => {
                tag.value_type == TagValueType::Number
            }
            SimulatorKind::Pulse => tag.value_type == TagValueType::Bool,
            SimulatorKind::Constant | SimulatorKind::Manual => true,
        };
        if !simulator_ok {
            errors.push(issue(
                format!("{path}.simulator"),
                "simulator 与 value_type 不兼容",
            ));
        }
        if let Err(error) = validate_tag_value(tag) {
            errors.push(error);
        }
    }

    if project.screens.is_empty() {
        errors.push(issue("screens", "工程至少需要一个画面"));
    }
    if project.screens.len() > 100 {
        errors.push(issue("screens", "工程最多 100 个画面"));
    }

    let mut screen_ids = HashSet::new();
    let mut node_ids = HashSet::new();
    let mut found_startup = false;

    for (screen_index, screen) in project.screens.iter().enumerate() {
        let screen_path = format!("screens[{screen_index}]");
        if !is_valid_id(screen.id()) {
            errors.push(issue(&screen_path, "画面 ID 非法"));
        }
        if !screen_ids.insert(screen.id().to_string()) {
            errors.push(issue(
                &screen_path,
                format!("画面 ID `{}` 重复", screen.id()),
            ));
        }
        check_non_empty(&mut errors, &screen_path, screen.name(), "画面名称");
        if screen.id() == project.startup_screen {
            found_startup = true;
        }

        match screen {
            Screen::TwoD {
                background, nodes, ..
            } => {
                check_color(&mut errors, &screen_path, background);
                if nodes.len() > MAX_NODES_2D {
                    errors.push(issue(
                        &screen_path,
                        format!("2D 画面最多 {MAX_NODES_2D} 个节点"),
                    ));
                }
                for (node_index, node) in nodes.iter().enumerate() {
                    let node_path = format!("{screen_path}.nodes[{node_index}]");
                    if !is_valid_id(&node.id) {
                        errors.push(issue(&node_path, "节点 ID 非法"));
                    }
                    if !node_ids.insert(node.id.clone()) {
                        errors.push(issue(&node_path, format!("节点 ID `{}` 重复", node.id)));
                    }
                    validate_ui_node(&mut errors, &node_path, node, &tags);
                }
            }
            Screen::ThreeD {
                background, nodes, ..
            } => {
                check_color(&mut errors, &screen_path, background);
                if nodes.len() > MAX_NODES_3D {
                    errors.push(issue(
                        &screen_path,
                        format!("3D 画面最多 {MAX_NODES_3D} 个设备节点"),
                    ));
                }
                for (node_index, node) in nodes.iter().enumerate() {
                    let node_path = format!("{screen_path}.nodes[{node_index}]");
                    if !is_valid_id(&node.id) {
                        errors.push(issue(&node_path, "节点 ID 非法"));
                    }
                    if !node_ids.insert(node.id.clone()) {
                        errors.push(issue(&node_path, format!("节点 ID `{}` 重复", node.id)));
                    }
                    check_non_empty(&mut errors, &node_path, &node.name, "设备名称");
                    for value in node.position.iter().chain(node.rotation_deg.iter()) {
                        check_finite(&mut errors, &node_path, *value);
                    }
                    for value in node.scale {
                        if !value.is_finite() || value <= 0.0 {
                            errors.push(issue(&node_path, "scale 必须全部大于 0"));
                        }
                    }
                    if let Some(label) = &node.label
                        && label.chars().count() > 120
                    {
                        errors.push(issue(&node_path, "label 不能超过 120 字符"));
                    }
                    validate_device_model(&mut errors, &node_path, &node.model);
                    for binding in &node.bindings {
                        validate_binding(
                            &mut errors,
                            &format!("{node_path}.bindings"),
                            binding,
                            &tags,
                        );
                    }
                }
            }
        }
    }

    if !found_startup {
        errors.push(issue(
            "startup_screen",
            format!(
                "startup_screen 引用的画面 `{}` 不存在",
                project.startup_screen
            ),
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
