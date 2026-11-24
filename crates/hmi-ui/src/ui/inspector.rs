use super::common::{
    default_mapping_for_kind, execute_command, focus_canvas, format_hmi_error, format_number,
    legal_mappings, model_label, report_error, target_label, valid_targets, valid_targets_3d,
    widget_label,
};
use crate::bridge;
use dioxus::prelude::*;
use hmi_core::{
    BarDirection, Binding, BindingMapping, BindingTarget, ButtonAction, DeviceModel, DeviceNode,
    EditorCommand, EquipmentKind, HexColor, TagValueType, TextAlign, UiNode, Widget2D,
    display_axis_to_scene, scene_to_display_axes,
};

#[component]
pub fn Inspector() -> Element {
    let _ = bridge::DOCUMENT_REVISION();
    let _ = bridge::ACTIVE_SCREEN();
    let _ = bridge::SELECTED_NODES();
    let project = bridge::project();
    let active_id = bridge::active_screen().unwrap_or_else(|| project.startup_screen.clone());
    let selected_nodes = bridge::selected_nodes();
    let selected = selected_nodes.last().cloned();
    let multi_selection = selected_nodes.len() > 1;
    let selection_label = if multi_selection {
        format!("{} 个节点", selected_nodes.len())
    } else {
        selected.clone().unwrap_or_else(|| "未选择".to_string())
    };
    let ScreenSelection { node_2d, node_3d } = selected
        .as_deref()
        .and_then(|node_id| {
            project
                .screen(&active_id)
                .and_then(|screen| find_node(screen, node_id))
        })
        .unwrap_or(ScreenSelection {
            node_2d: None,
            node_3d: None,
        });

    rsx! {
        aside {
            class: "right-panel",
            id: "inspector",
            "data-testid": "inspector",
            aria_label: "属性检查器",
            onkeydown: move |event| event.stop_propagation(),
            div { class: "section-heading inspector-heading",
                div {
                    div { class: "eyebrow", "属性检查器" }
                    h2 { class: "section-title", "Selection" }
                }
                span { class: "selection-badge", "data-testid": "selection-id",
                    "{selection_label}"
                }
            }
            if multi_selection {
                MultiInspector {
                    screen_id: active_id,
                    ids: selected_nodes,
                }
            } else if let Some(node) = node_2d {
                Inspector2D {
                    key: "{node.id}-{bridge::DOCUMENT_REVISION()}",
                    screen_id: active_id,
                    node,
                }
            } else if let Some(node) = node_3d {
                Inspector3D {
                    key: "{node.id}-{bridge::DOCUMENT_REVISION()}",
                    screen_id: active_id,
                    node,
                }
            } else {
                div { class: "empty-state",
                    h3 { class: "empty-title", "未选择节点" }
                    p { "在画面或左侧结构树中选择设备，即可编辑几何、组件和 Tag 绑定。" }
                    button {
                        class: "mini-button",
                        onclick: move |_| bridge::reset_camera_3d(),
                        "重置 3D 视角"
                    }
                }
            }
        }
    }
}

#[component]
fn MultiInspector(screen_id: String, ids: Vec<String>) -> Element {
    let project = bridge::project();
    let screen = project.screen(&screen_id);
    let (kind, names, locked_count) = match screen {
        Some(hmi_core::Screen::TwoD { nodes, .. }) => {
            let selected = nodes.iter().filter(|node| ids.contains(&node.id));
            let mut count = 0;
            let mut locked = 0;
            let mut names = Vec::new();
            for node in selected {
                count += 1;
                locked += usize::from(node.locked);
                names.push(node.name.clone());
            }
            (format!("2D · {count} 个图元"), names, locked)
        }
        Some(hmi_core::Screen::ThreeD { nodes, .. }) => {
            let selected = nodes.iter().filter(|node| ids.contains(&node.id));
            let mut count = 0;
            let mut locked = 0;
            let mut names = Vec::new();
            for node in selected {
                count += 1;
                locked += usize::from(node.locked);
                names.push(node.name.clone());
            }
            (format!("3D · {count} 个设备"), names, locked)
        }
        None => (format!("{} 个节点", ids.len()), Vec::new(), 0),
    };
    let primary = ids.last().cloned().unwrap_or_default();
    let all_locked = !names.is_empty() && locked_count == names.len();
    let is_three_d = kind.starts_with("3D");
    let mut preview = names.iter().take(6).cloned().collect::<Vec<_>>().join("、");
    if names.len() > 6 {
        preview.push_str(&format!(" 等 {} 个", names.len()));
    }
    rsx! {
        div { class: "inspector-body", "data-testid": "multi-inspector",
            div { class: "kind-row",
                span { class: "kind-chip", "多选" }
                span { class: "kind-note", "{kind}" }
            }
            InspectorSection { title: "选择摘要",
                p { class: "empty-copy", "{preview}" }
                div { class: "field-grid two",
                    TextOnly { label: "主节点", value: primary }
                    TextOnly { label: "锁定", value: format!("{locked_count} 个") }
                }
            }
            div { class: "inspector-actions multi-actions",
                button {
                    class: "mini-button",
                    "data-testid": "multi-copy",
                    onclick: move |_| {
                        bridge::copy_selection();
                        focus_canvas();
                    },
                    "复制 ⌘C"
                }
                button {
                    class: "mini-button",
                    "data-testid": "multi-cut",
                    onclick: move |_| {
                        if let Err(error) = bridge::cut_selection() {
                            report_error(error);
                        }
                        focus_canvas();
                    },
                    "剪切 ⌘X"
                }
                button {
                    class: "mini-button",
                    "data-testid": "multi-duplicate",
                    disabled: all_locked,
                    onclick: move |_| {
                        if let Err(error) = bridge::duplicate_selection() {
                            report_error(error);
                        }
                        focus_canvas();
                    },
                    "原地复制 ⌘D"
                }
                button {
                    class: "mini-button danger",
                    "data-testid": "multi-delete",
                    disabled: all_locked,
                    onclick: move |_| {
                        if let Err(error) = bridge::delete_selection() {
                            report_error(error);
                        }
                        focus_canvas();
                    },
                    "删除"
                }
                if is_three_d {
                    button {
                        class: "mini-button",
                        "data-testid": "multi-align-3d-x",
                        disabled: all_locked,
                        onclick: move |_| {
                            if let Err(error) = bridge::align_selected_3d(0) {
                                report_error(error);
                            }
                            focus_canvas();
                        },
                        "垂直中心面 X=0"
                    }
                    button {
                        class: "mini-button",
                        "data-testid": "multi-align-3d-y",
                        disabled: all_locked,
                        onclick: move |_| {
                            if let Err(error) = bridge::align_selected_3d(1) {
                                report_error(error);
                            }
                            focus_canvas();
                        },
                        "垂直中心面 Y=0"
                    }
                    button {
                        class: "mini-button",
                        "data-testid": "multi-align-3d-z",
                        disabled: all_locked,
                        onclick: move |_| {
                            if let Err(error) = bridge::align_selected_3d(2) {
                                report_error(error);
                            }
                            focus_canvas();
                        },
                        "水平中心面 Z=0"
                    }
                }
            }
            p { class: "kind-note", "组移动、组轻推、剪切、粘贴和删除都会合并为一条撤销记录。锁定节点不会被组操作修改。" }
        }
    }
}

#[derive(Default)]
struct ScreenSelection {
    node_2d: Option<UiNode>,
    node_3d: Option<DeviceNode>,
}

fn find_node(screen: &hmi_core::Screen, node_id: &str) -> Option<ScreenSelection> {
    match screen {
        hmi_core::Screen::TwoD { nodes, .. } => nodes
            .iter()
            .find(|node| node.id == node_id)
            .cloned()
            .map(|node| ScreenSelection {
                node_2d: Some(node),
                node_3d: None,
            }),
        hmi_core::Screen::ThreeD { nodes, .. } => nodes
            .iter()
            .find(|node| node.id == node_id)
            .cloned()
            .map(|node| ScreenSelection {
                node_2d: None,
                node_3d: Some(node),
            }),
    }
}

fn update_2d(screen_id: &str, node: UiNode) {
    execute_command(
        EditorCommand::Update2dNode {
            screen_id: screen_id.to_string(),
            node,
        },
        None,
    );
}

fn update_3d(screen_id: &str, node: DeviceNode) {
    execute_command(
        EditorCommand::Update3dNode {
            screen_id: screen_id.to_string(),
            node,
        },
        None,
    );
}

fn update_bindings(screen_id: &str, node_id: &str, bindings: Vec<Binding>) {
    execute_command(
        EditorCommand::SetBindings {
            screen_id: screen_id.to_string(),
            node_id: node_id.to_string(),
            bindings,
        },
        Some("绑定已应用"),
    );
}

#[component]
fn Inspector2D(screen_id: String, node: UiNode) -> Element {
    let locked = node.locked;
    let node_id = node.id.clone();
    let duplicate_screen = screen_id.clone();
    let duplicate_id = node_id.clone();
    let delete_screen = screen_id.clone();
    let delete_id = node_id.clone();
    let delete_locked = locked;

    rsx! {
        div { class: "inspector-body",
            div { class: "kind-row",
                span { class: "kind-chip", "{widget_label(&node.widget)}" }
                span { class: "kind-note", "2D 图元" }
            }
            InspectorSection { title: "标识与层级",
                TextField {
                    label: "名称".to_string(),
                    test_id: "node-name".to_string(),
                    value: node.name.clone(),
                    disabled: false,
                    on_commit: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |value: String| {
                            node.name = value;
                            update_2d(&screen_id, node.clone());
                        }
                    },
                }
                div { class: "field-grid two",
                    TextOnly { label: "ID", value: node.id.clone() }
                    NumberField {
                        label: "层级 Z".to_string(),
                        test_id: "node-z".to_string(),
                        value: node.z as f32,
                        disabled: locked,
                        on_commit: {
                            let screen_id = screen_id.clone();
                            let mut node = node.clone();
                            move |value: f32| {
                                node.z = value.round() as i32;
                                update_2d(&screen_id, node.clone());
                            }
                        },
                    }
                }
                div { class: "field-grid two",
                    CheckField {
                        label: "可见".to_string(),
                        test_id: "node-visible".to_string(),
                        checked: node.visible,
                        disabled: false,
                        on_change: {
                            let screen_id = screen_id.clone();
                            let mut node = node.clone();
                            move |value: bool| {
                                node.visible = value;
                                update_2d(&screen_id, node.clone());
                            }
                        },
                    }
                    CheckField {
                        label: "锁定".to_string(),
                        test_id: "node-locked".to_string(),
                        checked: node.locked,
                        disabled: false,
                        on_change: {
                            let screen_id = screen_id.clone();
                            let mut node = node.clone();
                            move |value: bool| {
                                node.locked = value;
                                update_2d(&screen_id, node.clone());
                            }
                        },
                    }
                }
            }

            InspectorSection { title: "几何",
                div { class: "field-grid two",
                    NumberField {
                        label: "X".to_string(),
                        test_id: "node-x".to_string(),
                        value: node.rect.x,
                        disabled: locked,
                        on_commit: {
                            let screen_id = screen_id.clone();
                            let mut node = node.clone();
                            move |value: f32| {
                                node.rect.x = value.clamp(0.0, 1600.0 - node.rect.width.min(1600.0));
                                update_2d(&screen_id, node.clone());
                            }
                        },
                    }
                    NumberField {
                        label: "Y".to_string(),
                        test_id: "node-y".to_string(),
                        value: node.rect.y,
                        disabled: locked,
                        on_commit: {
                            let screen_id = screen_id.clone();
                            let mut node = node.clone();
                            move |value: f32| {
                                node.rect.y = value.clamp(0.0, 900.0 - node.rect.height.min(900.0));
                                update_2d(&screen_id, node.clone());
                            }
                        },
                    }
                    NumberField {
                        label: "宽度".to_string(),
                        test_id: "node-width".to_string(),
                        value: node.rect.width,
                        disabled: locked,
                        on_commit: {
                            let screen_id = screen_id.clone();
                            let mut node = node.clone();
                            move |value: f32| {
                                node.rect.width = value.clamp(8.0, 1600.0);
                                update_2d(&screen_id, node.clone());
                            }
                        },
                    }
                    NumberField {
                        label: "高度".to_string(),
                        test_id: "node-height".to_string(),
                        value: node.rect.height,
                        disabled: locked,
                        on_commit: {
                            let screen_id = screen_id.clone();
                            let mut node = node.clone();
                            move |value: f32| {
                                node.rect.height = value.clamp(8.0, 900.0);
                                update_2d(&screen_id, node.clone());
                            }
                        },
                    }
                }
                div { class: "field-grid two",
                    NumberField {
                        label: "旋转 °".to_string(),
                        test_id: "node-rotation".to_string(),
                        value: node.rotation_deg,
                        disabled: locked,
                        on_commit: {
                            let screen_id = screen_id.clone();
                            let mut node = node.clone();
                            move |value: f32| {
                                node.rotation_deg = value;
                                update_2d(&screen_id, node.clone());
                            }
                        },
                    }
                    div { class: "field field-align-end",
                        button {
                            class: "mini-button",
                            onclick: {
                                let selected = node.id.clone();
                                move |_| bridge::select_node(Some(selected.clone()))
                            },
                            "定位节点"
                        }
                    }
                }
            }

            InspectorSection { title: "组件参数",
                Widget2DEditor { screen_id: screen_id.clone(), node: node.clone() }
            }

            InspectorSection {
                title: "Tag 绑定",
                BindingsEditor {
                    screen_id: screen_id.clone(),
                    node_id: node_id.clone(),
                    bindings: node.bindings.clone(),
                    domain: "2d".to_string(),
                    widget: Some(node.widget.clone()),
                }
            }

            div { class: "inspector-actions",
                button {
                    class: "mini-button",
                    "data-testid": "duplicate-button",
                    onclick: move |_| match bridge::execute(EditorCommand::DuplicateNode {
                        screen_id: duplicate_screen.clone(),
                        node_id: duplicate_id.clone(),
                    }) {
                        Ok(result) => {
                            if let Some(id) = result.affected_id {
                                bridge::select_node(Some(id));
                            }
                        }
                        Err(error) => report_error(error),
                    },
                    "复制节点"
                }
                button {
                    class: "mini-button danger",
                    "data-testid": "delete-button",
                    disabled: delete_locked,
                    onclick: move |_| match bridge::execute(EditorCommand::DeleteNode {
                        screen_id: delete_screen.clone(),
                        node_id: delete_id.clone(),
                    }) {
                        Ok(_) => bridge::select_node(None),
                        Err(error) => report_error(error),
                    },
                    "删除节点"
                }
            }
        }
    }
}

#[component]
fn Widget2DEditor(screen_id: String, node: UiNode) -> Element {
    match &node.widget {
        Widget2D::Panel {
            fill,
            stroke,
            stroke_width,
        } => rsx! {
            ColorField {
                label: "填充".to_string(),
                test_id: "widget-fill".to_string(),
                value: fill.0.clone(),
                on_change: {
                    let screen_id = screen_id.clone();
                    let mut node = node.clone();
                    move |value: String| {
                        if let Widget2D::Panel { fill, .. } = &mut node.widget {
                            *fill = HexColor(value.clone());
                        }
                        update_2d(&screen_id, node.clone());
                    }
                },
            }
            ColorField {
                label: "描边".to_string(),
                test_id: "widget-stroke".to_string(),
                value: stroke.0.clone(),
                on_change: {
                    let screen_id = screen_id.clone();
                    let mut node = node.clone();
                    move |value: String| {
                        if let Widget2D::Panel { stroke, .. } = &mut node.widget {
                            *stroke = HexColor(value.clone());
                        }
                        update_2d(&screen_id, node.clone());
                    }
                },
            }
            NumberField {
                label: "描边宽度".to_string(),
                test_id: "widget-stroke-width".to_string(),
                value: *stroke_width,
                disabled: false,
                on_commit: {
                    let screen_id = screen_id.clone();
                    let mut node = node.clone();
                    move |value: f32| {
                        if let Widget2D::Panel { stroke_width, .. } = &mut node.widget {
                            *stroke_width = value.max(0.0);
                        }
                        update_2d(&screen_id, node.clone());
                    }
                },
            }
        },
        Widget2D::Label {
            text,
            font_size,
            color,
            align,
        } => rsx! {
            TextField {
                label: "文本".to_string(),
                test_id: "widget-text".to_string(),
                value: text.clone(),
                disabled: false,
                on_commit: {
                    let screen_id = screen_id.clone();
                    let mut node = node.clone();
                    move |value: String| {
                        if let Widget2D::Label { text, .. } = &mut node.widget {
                            *text = value;
                        }
                        update_2d(&screen_id, node.clone());
                    }
                },
            }
            NumberField {
                label: "字号".to_string(),
                test_id: "widget-font-size".to_string(),
                value: *font_size,
                disabled: false,
                on_commit: {
                    let screen_id = screen_id.clone();
                    let mut node = node.clone();
                    move |value: f32| {
                        if let Widget2D::Label { font_size, .. } = &mut node.widget {
                            *font_size = value;
                        }
                        update_2d(&screen_id, node.clone());
                    }
                },
            }
            ColorField {
                label: "颜色".to_string(),
                test_id: "widget-color".to_string(),
                value: color.0.clone(),
                on_change: {
                    let screen_id = screen_id.clone();
                    let mut node = node.clone();
                    move |value: String| {
                        if let Widget2D::Label { color, .. } = &mut node.widget {
                            *color = HexColor(value.clone());
                        }
                        update_2d(&screen_id, node.clone());
                    }
                },
            }
            LabeledControl { label: "对齐", control_id: "widget-text-align",
                select {
                    id: "widget-text-align",
                    class: "select-input",
                    value: text_align_value(*align),
                    onchange: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |event| {
                            if let Widget2D::Label { align, .. } = &mut node.widget {
                                *align = parse_text_align(&event.value());
                            }
                            update_2d(&screen_id, node.clone());
                        }
                    },
                    option { value: "left", "左对齐" }
                    option { value: "center", "居中" }
                    option { value: "right", "右对齐" }
                }
            }
        },
        Widget2D::ValueCard { label } | Widget2D::StatusLamp { label } => rsx! {
            TextField {
                label: "标签".to_string(),
                test_id: "widget-label".to_string(),
                value: label.clone(),
                disabled: false,
                on_commit: {
                    let screen_id = screen_id.clone();
                    let mut node = node.clone();
                    move |value: String| {
                        match &mut node.widget {
                            Widget2D::ValueCard { label }
                            | Widget2D::StatusLamp { label } => *label = value,
                            _ => {}
                        }
                        update_2d(&screen_id, node.clone());
                    }
                },
            }
        },
        Widget2D::BarGauge {
            min,
            max,
            direction,
        } => rsx! {
            div { class: "field-grid two",
                NumberField {
                    label: "最小值".to_string(),
                    test_id: "widget-min".to_string(),
                    value: *min,
                    disabled: false,
                    on_commit: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |value: f32| {
                            if let Widget2D::BarGauge { min, .. } = &mut node.widget {
                                *min = value;
                            }
                            update_2d(&screen_id, node.clone());
                        }
                    },
                }
                NumberField {
                    label: "最大值".to_string(),
                    test_id: "widget-max".to_string(),
                    value: *max,
                    disabled: false,
                    on_commit: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |value: f32| {
                            if let Widget2D::BarGauge { max, .. } = &mut node.widget {
                                *max = value;
                            }
                            update_2d(&screen_id, node.clone());
                        }
                    },
                }
            }
            LabeledControl { label: "方向", control_id: "widget-bar-direction",
                select {
                    id: "widget-bar-direction",
                    class: "select-input",
                    value: match direction { BarDirection::Horizontal => "horizontal", BarDirection::Vertical => "vertical" },
                    onchange: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |event| {
                            if let Widget2D::BarGauge { direction, .. } = &mut node.widget {
                                *direction = if event.value() == "vertical" { BarDirection::Vertical } else { BarDirection::Horizontal };
                            }
                            update_2d(&screen_id, node.clone());
                        }
                    },
                    option { value: "horizontal", "水平" }
                    option { value: "vertical", "垂直" }
                }
            }
        },
        Widget2D::Button { label, action } => {
            let action_tag = match action {
                ButtonAction::ToggleTag { tag_id } | ButtonAction::SetTag { tag_id, .. } => {
                    tag_id.clone()
                }
            };
            rsx! {
                TextField {
                    label: "按钮文本".to_string(),
                    test_id: "widget-label".to_string(),
                    value: label.clone(),
                    disabled: false,
                    on_commit: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |value: String| {
                            if let Widget2D::Button { label, .. } = &mut node.widget {
                                *label = value;
                            }
                            update_2d(&screen_id, node.clone());
                        }
                    },
                }
                LabeledControl { label: "动作 Tag", control_id: "widget-action-tag",
                    select {
                        id: "widget-action-tag",
                        class: "select-input",
                        value: action_tag.clone(),
                        onchange: {
                            let screen_id = screen_id.clone();
                            let mut node = node.clone();
                            move |event| {
                                if let Widget2D::Button { action, .. } = &mut node.widget {
                                    *action = ButtonAction::ToggleTag { tag_id: event.value() };
                                }
                                update_2d(&screen_id, node.clone());
                            }
                        },
                        for tag in bridge::project().tags.iter().filter(|tag| tag.writable && tag.value_type == TagValueType::Bool) {
                            option { value: "{tag.id}", "{tag.name}" }
                        }
                    }
                }
            }
        }
        Widget2D::Pipe {
            points,
            thickness,
            color,
        } => rsx! {
            p { class: "kind-note", "画布中拖动圆形折点；拖动端点靠近设备会吸附到设备边缘，Shift 锁定水平/垂直方向。" }
            TextField {
                label: "折点 x,y;x,y".to_string(),
                test_id: "widget-points".to_string(),
                value: format_points(points),
                disabled: false,
                on_commit: {
                    let screen_id = screen_id.clone();
                    let mut node = node.clone();
                    move |value: String| match parse_points(&value) {
                        Ok(points) => {
                            if let Widget2D::Pipe { points: target, .. } = &mut node.widget {
                                *target = points;
                            }
                            update_2d(&screen_id, node.clone());
                        }
                        Err(error) => report_error(error),
                    }
                },
            }
            NumberField {
                label: "线宽".to_string(),
                test_id: "widget-thickness".to_string(),
                value: *thickness,
                disabled: false,
                on_commit: {
                    let screen_id = screen_id.clone();
                    let mut node = node.clone();
                    move |value: f32| {
                        if let Widget2D::Pipe { thickness, .. } = &mut node.widget {
                            *thickness = value.max(0.5);
                        }
                        update_2d(&screen_id, node.clone());
                    }
                },
            }
            ColorField {
                label: "颜色".to_string(),
                test_id: "widget-color".to_string(),
                value: color.0.clone(),
                on_change: {
                    let screen_id = screen_id.clone();
                    let mut node = node.clone();
                    move |value: String| {
                        if let Widget2D::Pipe { color, .. } = &mut node.widget {
                            *color = HexColor(value.clone());
                        }
                        update_2d(&screen_id, node.clone());
                    }
                },
            }
        },
        Widget2D::EquipmentSymbol { equipment, label } => rsx! {
            LabeledControl { label: "设备类型", control_id: "widget-equipment-kind",
                select {
                    id: "widget-equipment-kind",
                    class: "select-input",
                    value: equipment_value(*equipment),
                    onchange: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |event| {
                            if let Widget2D::EquipmentSymbol { equipment, .. } = &mut node.widget {
                                *equipment = parse_equipment(&event.value());
                            }
                            update_2d(&screen_id, node.clone());
                        }
                    },
                    option { value: "ahu", "AHU" }
                    option { value: "fan", "风机" }
                    option { value: "pump", "水泵" }
                    option { value: "valve", "阀门" }
                }
            }
            TextField {
                label: "设备标签".to_string(),
                test_id: "widget-label".to_string(),
                value: label.clone(),
                disabled: false,
                on_commit: {
                    let screen_id = screen_id.clone();
                    let mut node = node.clone();
                    move |value: String| {
                        if let Widget2D::EquipmentSymbol { label, .. } = &mut node.widget {
                            *label = value;
                        }
                        update_2d(&screen_id, node.clone());
                    }
                },
            }
        },
    }
}

#[component]
fn Inspector3D(screen_id: String, node: DeviceNode) -> Element {
    let locked = node.locked;
    let node_id = node.id.clone();
    let duplicate_screen = screen_id.clone();
    let duplicate_id = node_id.clone();
    let delete_screen = screen_id.clone();
    let delete_id = node_id.clone();
    let delete_locked = locked;

    rsx! {
        div { class: "inspector-body",
            div { class: "kind-row",
                span { class: "kind-chip", "{model_label(&node.model)}" }
                span { class: "kind-note", "数字孪生设备" }
            }
            InspectorSection { title: "标识与可见性",
                TextField {
                    label: "名称".to_string(),
                    test_id: "node-name".to_string(),
                    value: node.name.clone(),
                    disabled: false,
                    on_commit: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |value: String| {
                            node.name = value;
                            update_3d(&screen_id, node.clone());
                        }
                    },
                }
                TextField {
                    label: "设备标签".to_string(),
                    test_id: "node-label".to_string(),
                    value: node.label.clone().unwrap_or_default(),
                    disabled: false,
                    on_commit: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |value: String| {
                            node.label = if value.trim().is_empty() { None } else { Some(value) };
                            update_3d(&screen_id, node.clone());
                        }
                    },
                }
                div { class: "field-grid two",
                    CheckField {
                        label: "可见".to_string(),
                        test_id: "node-visible".to_string(),
                        checked: node.visible,
                        disabled: false,
                        on_change: {
                            let screen_id = screen_id.clone();
                            let mut node = node.clone();
                            move |value: bool| {
                                node.visible = value;
                                update_3d(&screen_id, node.clone());
                            }
                        },
                    }
                    CheckField {
                        label: "锁定".to_string(),
                        test_id: "node-locked".to_string(),
                        checked: node.locked,
                        disabled: false,
                        on_change: {
                            let screen_id = screen_id.clone();
                            let mut node = node.clone();
                            move |value: bool| {
                                node.locked = value;
                                update_3d(&screen_id, node.clone());
                            }
                        },
                    }
                }
            }

            InspectorSection { title: "3D 变换",
                p { class: "axis-note", "坐标约定：X 左右 · Y 前后 · Z 垂直地面" }
                AxisFields {
                    labels: ["X", "Y", "Z"],
                    test_ids: ["node-position-x", "node-position-y", "node-position-z"],
                    values: scene_to_display_axes(node.position),
                    disabled: locked,
                    on_commit: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |(axis, value): (usize, f32)| {
                            node.position[display_axis_to_scene(axis)] = value;
                            update_3d(&screen_id, node.clone());
                        }
                    },
                }
                AxisFields {
                    labels: ["RX", "RY", "RZ"],
                    test_ids: ["node-rotation-x", "node-rotation-y", "node-rotation-z"],
                    values: scene_to_display_axes(node.rotation_deg),
                    disabled: locked,
                    on_commit: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |(axis, value): (usize, f32)| {
                            node.rotation_deg[display_axis_to_scene(axis)] = value;
                            update_3d(&screen_id, node.clone());
                        }
                    },
                }
                AxisFields {
                    labels: ["SX", "SY", "SZ"],
                    test_ids: ["node-scale-x", "node-scale-y", "node-scale-z"],
                    values: scene_to_display_axes(node.scale),
                    disabled: locked,
                    on_commit: {
                        let screen_id = screen_id.clone();
                        let mut node = node.clone();
                        move |(axis, value): (usize, f32)| {
                            node.scale[display_axis_to_scene(axis)] = value.max(0.01);
                            update_3d(&screen_id, node.clone());
                        }
                    },
                }
                div { class: "field-grid two",
                    button {
                        class: "mini-button",
                        "data-testid": "align-3d-x",
                        disabled: locked,
                        onclick: move |_| {
                            if let Err(error) = bridge::align_selected_3d(0) {
                                report_error(error);
                            }
                            focus_canvas();
                        },
                        "垂直中心面 X=0"
                    }
                    button {
                        class: "mini-button",
                        "data-testid": "align-3d-y",
                        disabled: locked,
                        onclick: move |_| {
                            if let Err(error) = bridge::align_selected_3d(1) {
                                report_error(error);
                            }
                            focus_canvas();
                        },
                        "垂直中心面 Y=0"
                    }
                    button {
                        class: "mini-button",
                        "data-testid": "align-3d-z",
                        disabled: locked,
                        onclick: move |_| {
                            if let Err(error) = bridge::align_selected_3d(2) {
                                report_error(error);
                            }
                            focus_canvas();
                        },
                        "水平中心面 Z=0"
                    }
                }
                button {
                    class: "mini-button",
                    onclick: move |_| bridge::reset_camera_3d(),
                    "重置相机"
                }
            }

            InspectorSection { title: "设备模型",
                match &node.model {
                    DeviceModel::Ahu { size, color }
                    | DeviceModel::Fan { size, color }
                    | DeviceModel::Pump { size, color } => {
                        let variant = match &node.model {
                            DeviceModel::Ahu { .. } => "Ahu",
                            DeviceModel::Fan { .. } => "Fan",
                            _ => "Pump",
                        };
                        rsx! {
                            AxisFields {
                                labels: ["宽", "高", "深"],
                                test_ids: ["model-size-x", "model-size-y", "model-size-z"],
                                values: *size,
                                disabled: false,
                                on_commit: {
                                    let screen_id = screen_id.clone();
                                    let mut node = node.clone();
                                    move |(axis, value): (usize, f32)| {
                                        match &mut node.model {
                                            DeviceModel::Ahu { size, .. } | DeviceModel::Fan { size, .. } | DeviceModel::Pump { size, .. } => size[axis] = value.max(0.01),
                                            _ => {}
                                        }
                                        update_3d(&screen_id, node.clone());
                                    }
                                },
                            }
                            ColorField {
                                label: "颜色".to_string(),
                                test_id: "model-color".to_string(),
                                value: color.0.clone(),
                                on_change: {
                                    let screen_id = screen_id.clone();
                                    let mut node = node.clone();
                                    move |value: String| {
                                        match &mut node.model {
                                            DeviceModel::Ahu { color, .. } | DeviceModel::Fan { color, .. } | DeviceModel::Pump { color, .. } => *color = HexColor(value.clone()),
                                            _ => {}
                                        }
                                        update_3d(&screen_id, node.clone());
                                    }
                                },
                            }
                            div { class: "kind-note", "模型类型：{variant}" }
                        }
                    }
                    DeviceModel::Duct { length, diameter, color } | DeviceModel::Pipe { length, diameter, color } => rsx! {
                        div { class: "field-grid two",
                            NumberField {
                                label: "长度".to_string(),
                                test_id: "model-length".to_string(),
                                value: *length,
                                disabled: false,
                                on_commit: {
                                    let screen_id = screen_id.clone();
                                    let mut node = node.clone();
                                    move |value: f32| {
                                        match &mut node.model {
                                            DeviceModel::Duct { length, .. } | DeviceModel::Pipe { length, .. } => *length = value.max(0.01),
                                            _ => {}
                                        }
                                        update_3d(&screen_id, node.clone());
                                    }
                                },
                            }
                            NumberField {
                                label: "直径".to_string(),
                                test_id: "model-diameter".to_string(),
                                value: *diameter,
                                disabled: false,
                                on_commit: {
                                    let screen_id = screen_id.clone();
                                    let mut node = node.clone();
                                    move |value: f32| {
                                        match &mut node.model {
                                            DeviceModel::Duct { diameter, .. } | DeviceModel::Pipe { diameter, .. } => *diameter = value.max(0.01),
                                            _ => {}
                                        }
                                        update_3d(&screen_id, node.clone());
                                    }
                                },
                            }
                        }
                        ColorField {
                            label: "颜色".to_string(),
                            test_id: "model-color".to_string(),
                            value: color.0.clone(),
                            on_change: {
                                let screen_id = screen_id.clone();
                                let mut node = node.clone();
                                move |value: String| {
                                    match &mut node.model {
                                        DeviceModel::Duct { color, .. } | DeviceModel::Pipe { color, .. } => *color = HexColor(value.clone()),
                                        _ => {}
                                    }
                                    update_3d(&screen_id, node.clone());
                                }
                            },
                        }
                    },
                    DeviceModel::Valve { size, color } => rsx! {
                        NumberField {
                            label: "阀体尺寸".to_string(),
                            test_id: "model-size".to_string(),
                            value: *size,
                            disabled: false,
                            on_commit: {
                                let screen_id = screen_id.clone();
                                let mut node = node.clone();
                                move |value: f32| {
                                    if let DeviceModel::Valve { size, .. } = &mut node.model {
                                        *size = value.max(0.01);
                                    }
                                    update_3d(&screen_id, node.clone());
                                }
                            },
                        }
                        ColorField {
                            label: "颜色".to_string(),
                            test_id: "model-color".to_string(),
                            value: color.0.clone(),
                            on_change: {
                                let screen_id = screen_id.clone();
                                let mut node = node.clone();
                                move |value: String| {
                                    if let DeviceModel::Valve { color, .. } = &mut node.model {
                                        *color = HexColor(value.clone());
                                    }
                                    update_3d(&screen_id, node.clone());
                                }
                            },
                        }
                    },
                    DeviceModel::Glb { path } => rsx! {
                        TextField {
                            label: "GLB 路径".to_string(),
                            test_id: "model-glb-path".to_string(),
                            value: path.clone(),
                            disabled: false,
                            on_commit: {
                                let screen_id = screen_id.clone();
                                let mut node = node.clone();
                                move |value: String| {
                                    if let DeviceModel::Glb { path } = &mut node.model {
                                        *path = value;
                                    }
                                    update_3d(&screen_id, node.clone());
                                }
                            },
                        }
                    },
                }
            }

            InspectorSection {
                title: "Tag 绑定",
                BindingsEditor {
                    screen_id: screen_id.clone(),
                    node_id: node_id.clone(),
                    bindings: node.bindings.clone(),
                    domain: "3d".to_string(),
                    widget: None,
                }
            }

            div { class: "inspector-actions",
                button {
                    class: "mini-button",
                    "data-testid": "duplicate-button",
                    onclick: move |_| match bridge::execute(EditorCommand::DuplicateNode {
                        screen_id: duplicate_screen.clone(),
                        node_id: duplicate_id.clone(),
                    }) {
                        Ok(result) => {
                            if let Some(id) = result.affected_id {
                                bridge::select_node(Some(id));
                            }
                        }
                        Err(error) => report_error(error),
                    },
                    "复制设备"
                }
                button {
                    class: "mini-button danger",
                    "data-testid": "delete-button",
                    disabled: delete_locked,
                    onclick: move |_| match bridge::execute(EditorCommand::DeleteNode {
                        screen_id: delete_screen.clone(),
                        node_id: delete_id.clone(),
                    }) {
                        Ok(_) => bridge::select_node(None),
                        Err(error) => report_error(error),
                    },
                    "删除设备"
                }
            }
        }
    }
}

#[component]
fn InspectorSection(title: &'static str, children: Element) -> Element {
    rsx! {
        section { class: "inspector-section",
            h3 { class: "inspector-section-title", "{title}" }
            {children}
        }
    }
}

#[component]
fn NumberField(
    label: String,
    test_id: String,
    value: f32,
    disabled: bool,
    on_commit: EventHandler<f32>,
) -> Element {
    let mut draft = use_signal(|| format_number(value));
    let mut dirty = use_signal(|| false);
    let mut invalid = use_signal(|| false);
    let initial = format_number(value);
    let field_id = format!("field-{test_id}");
    let error_id = format!("error-{test_id}");
    let commit = move || {
        if disabled || !dirty() {
            return;
        }
        match draft().trim().parse::<f32>() {
            Ok(parsed) if parsed.is_finite() => {
                dirty.set(false);
                invalid.set(false);
                bridge::clear_field_error_announcement();
                on_commit.call(parsed);
            }
            _ => {
                invalid.set(true);
                bridge::announce_field_error();
            }
        }
    };
    let mut blur_commit = commit;
    let mut key_commit = commit;
    let reset = initial.clone();

    rsx! {
        div { class: "field",
            label { r#for: "{field_id}", "{label}" }
            input {
                id: "{field_id}",
                class: if invalid() { "text-input numeric is-invalid" } else { "text-input numeric" },
                "data-testid": "{test_id}",
                aria_invalid: if invalid() { "true" } else { "false" },
                "aria-describedby": if invalid() { error_id.clone() } else { String::new() },
                value: "{draft}",
                disabled,
                oninput: move |event| {
                    draft.set(event.value());
                    dirty.set(true);
                    invalid.set(false);
                    bridge::clear_field_error_announcement();
                },
                onblur: move |_| blur_commit(),
                onkeydown: move |event| {
                    event.stop_propagation();
                    if event.key() == Key::Named(NamedKey::Enter) {
                        event.prevent_default();
                        if event.is_composing() {
                            return;
                        }
                        key_commit();
                    } else if event.key() == Key::Named(NamedKey::Escape) {
                        draft.set(reset.clone());
                        dirty.set(false);
                        invalid.set(false);
                        bridge::clear_field_error_announcement();
                    }
                },
            }
            if invalid() {
                span { id: "{error_id}", class: "field-error", "data-testid": "{error_id}", "请输入有效数值" }
            }
        }
    }
}

#[component]
fn TextField(
    label: String,
    test_id: String,
    value: String,
    disabled: bool,
    on_commit: EventHandler<String>,
) -> Element {
    let mut draft = use_signal(|| value.clone());
    let mut dirty = use_signal(|| false);
    let initial = value.clone();
    let field_id = format!("field-{test_id}");
    let commit = move || {
        if disabled || !dirty() {
            return;
        }
        dirty.set(false);
        on_commit.call(draft());
    };
    let mut blur_commit = commit;
    let mut key_commit = commit;
    let reset = initial.clone();
    rsx! {
        div { class: "field",
            label { r#for: "{field_id}", "{label}" }
            input {
                id: "{field_id}",
                class: "text-input",
                "data-testid": "{test_id}",
                value: "{draft}",
                disabled,
                oninput: move |event| {
                    draft.set(event.value());
                    dirty.set(true);
                },
                onblur: move |_| blur_commit(),
                onkeydown: move |event| {
                    event.stop_propagation();
                    if event.key() == Key::Named(NamedKey::Enter) {
                        event.prevent_default();
                        if event.is_composing() {
                            return;
                        }
                        key_commit();
                    } else if event.key() == Key::Named(NamedKey::Escape) {
                        draft.set(reset.clone());
                        dirty.set(false);
                    }
                },
            }
        }
    }
}

#[component]
fn ColorField(
    label: String,
    test_id: String,
    value: String,
    on_change: EventHandler<String>,
) -> Element {
    let field_id = format!("field-{test_id}");
    rsx! {
        div { class: "field color-field",
            label { r#for: "{field_id}", "{label}" }
            div { class: "color-control",
                input {
                    id: "{field_id}",
                    r#type: "color",
                    "data-testid": "{test_id}",
                    value: "{value}",
                    onchange: move |event| on_change.call(event.value()),
                }
                code { "{value}" }
            }
        }
    }
}

#[component]
fn CheckField(
    label: String,
    test_id: String,
    checked: bool,
    disabled: bool,
    on_change: EventHandler<bool>,
) -> Element {
    let field_id = format!("field-{test_id}");
    rsx! {
        label { class: "check-field", r#for: "{field_id}",
            input {
                id: "{field_id}",
                r#type: "checkbox",
                "data-testid": "{test_id}",
                checked,
                disabled,
                onchange: move |event| on_change.call(event.checked()),
            }
            span { "{label}" }
        }
    }
}

#[component]
fn LabeledControl(label: &'static str, control_id: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "field",
            label { r#for: "{control_id}", "{label}" }
            {children}
        }
    }
}

#[component]
fn TextOnly(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "field",
            span { "{label}" }
            code { class: "readonly-value", "{value}" }
        }
    }
}

#[component]
fn AxisFields(
    labels: [&'static str; 3],
    test_ids: [&'static str; 3],
    values: [f32; 3],
    disabled: bool,
    on_commit: EventHandler<(usize, f32)>,
) -> Element {
    rsx! {
        div { class: "axis-row",
            for index in 0..3 {
                NumberField {
                    label: labels[index].to_string(),
                    test_id: test_ids[index].to_string(),
                    value: values[index],
                    disabled,
                    on_commit: move |value| on_commit.call((index, value)),
                }
            }
        }
    }
}

#[component]
fn BindingsEditor(
    screen_id: String,
    node_id: String,
    bindings: Vec<Binding>,
    domain: String,
    widget: Option<Widget2D>,
) -> Element {
    let project = bridge::project();
    let tags = project.tags.clone();
    let mut draft = use_signal(|| bindings.clone());
    let mut active = use_signal(|| 0usize);
    let targets: &'static [BindingTarget] = if domain == "3d" {
        valid_targets_3d()
    } else {
        widget.as_ref().map(valid_targets).unwrap_or(&[
            BindingTarget::Text,
            BindingTarget::Color,
            BindingTarget::Visible,
        ])
    };

    let apply_screen = screen_id.clone();
    let apply_node = node_id.clone();
    let add_targets = targets;
    let add_tags = tags.clone();

    rsx! {
        div { class: "bindings-editor", "data-testid": "binding-list",
            onkeydown: move |event| event.stop_propagation(),
            if draft().is_empty() {
                div { class: "empty-copy", "暂无绑定。绑定后运行态会按 Tag 驱动视觉。" }
            }
            for (index, binding) in draft().iter().cloned().enumerate() {
                {
                    let tag_type = project
                        .tag(&binding.tag_id)
                        .map(|tag| tag.value_type)
                        .unwrap_or(TagValueType::Number);
                    let mappings = legal_mappings(binding.target, tag_type);
                    let project_tag = project.clone();
                    let project_target = project.clone();
                    let project_mapping = project.clone();
                    let binding_select_label = format!("选择第 {} 个绑定", index + 1);
                    let binding_tag_label = format!("绑定 {} 的 Tag", index + 1);
                    let binding_target_label = format!("绑定 {} 的目标", index + 1);
                    let binding_mapping_label = format!("绑定 {} 的映射", index + 1);
                    let binding_remove_label = format!("删除第 {} 个绑定", index + 1);
                    rsx! {
                        div {
                            key: "{index}",
                            class: if active() == index { "binding-row is-active" } else { "binding-row" },
                            div { class: "binding-row-head",
                                button {
                                    class: "binding-index",
                                    aria_label: "{binding_select_label}",
                                    aria_pressed: if active() == index { "true" } else { "false" },
                                    onclick: move |_| active.set(index),
                                    "#{index + 1}"
                                }
                                button {
                                    class: "icon-button danger tiny",
                                    "data-testid": "binding-remove-{index}",
                                    aria_label: "{binding_remove_label}",
                                    onclick: {
                                        let mut draft = draft;
                                        move |event| {
                                            event.stop_propagation();
                                            draft.write().remove(index);
                                            if active() >= draft().len() && active() > 0 {
                                                active.set(draft().len().saturating_sub(1));
                                            }
                                        }
                                    },
                                    "×"
                                }
                            }
                            select {
                                class: "select-input",
                                "data-testid": "binding-tag-{index}",
                                aria_label: "{binding_tag_label}",
                                value: "{binding.tag_id}",
                                onchange: {
                                    let mut draft = draft;
                                    move |event| {
                                        let tag_id = event.value();
                                        if let Some(tag) = project_tag.tag(&tag_id) {
                                            let mut list = draft.write();
                                            if let Some(target) = list.get_mut(index) {
                                                let next_target = if targets.contains(&target.target) { target.target } else { targets[0] };
                                                let kind = legal_mappings(next_target, tag.value_type).first().copied().unwrap_or("direct");
                                                *target = Binding {
                                                    tag_id,
                                                    target: next_target,
                                                    mapping: default_mapping_for_kind(next_target, tag, kind),
                                                };
                                            }
                                        }
                                    }
                                },
                                for tag in tags.iter() {
                                    option { value: "{tag.id}", "{tag.name} · {tag.id}" }
                                }
                            }
                            div { class: "field-grid two",
                                select {
                                    class: "select-input",
                                    "data-testid": "binding-target-{index}",
                                    aria_label: "{binding_target_label}",
                                    value: target_value(binding.target),
                                    onchange: {
                                        let mut draft = draft;
                                        move |event| {
                                            let target = parse_target(&event.value());
                                            if let Some(tag) = project_target.tag(&draft()[index].tag_id) {
                                                let kind = legal_mappings(target, tag.value_type).first().copied().unwrap_or("direct");
                                                draft.write()[index].target = target;
                                                draft.write()[index].mapping = default_mapping_for_kind(target, tag, kind);
                                            }
                                        }
                                    },
                                    for target in targets.iter().copied() {
                                        option { value: target_value(target), "{target_label(target)}" }
                                    }
                                }
                                select {
                                    class: "select-input",
                                    "data-testid": "binding-mapping-{index}",
                                    aria_label: "{binding_mapping_label}",
                                    value: mapping_value(&binding.mapping),
                                    onchange: {
                                        let mut draft = draft;
                                        move |event| {
                                            let kind = event.value();
                                            let current = draft()[index].clone();
                                            if let Some(tag) = project_mapping.tag(&current.tag_id) {
                                                draft.write()[index].mapping = default_mapping_for_kind(current.target, tag, &kind);
                                            }
                                        }
                                    },
                                    for mapping in mappings {
                                        option { value: "{mapping}", "{mapping_label_for_key(mapping)}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "binding-add-row",
                button {
                    class: "mini-button",
                    "data-testid": "binding-add",
                    disabled: tags.is_empty(),
                    onclick: move |_| {
                        let Some(tag) = add_tags.first() else { return; };
                        let target = add_targets[0];
                        let kind = legal_mappings(target, tag.value_type).first().copied().unwrap_or("direct");
                        draft.write().push(Binding {
                            tag_id: tag.id.clone(),
                            target,
                            mapping: default_mapping_for_kind(target, tag, kind),
                        });
                        active.set(draft().len().saturating_sub(1));
                    },
                    "+ 绑定"
                }
                if tags.is_empty() {
                    span { class: "hint-copy", "请先创建 Tag" }
                }
            }
            if let Some(binding) = draft().get(active()).cloned() {
                MappingFields {
                    binding,
                    on_change: move |mapping| {
                        if let Some(current) = draft.write().get_mut(active()) {
                            current.mapping = mapping;
                        }
                    },
                }
            }
            button {
                class: "mini-button primary apply-bindings",
                "data-testid": "apply-bindings",
                onclick: move |_| update_bindings(&apply_screen, &apply_node, draft()),
                "应用全部绑定"
            }
        }
    }
}

#[component]
fn MappingFields(binding: Binding, on_change: EventHandler<BindingMapping>) -> Element {
    let mapping = binding.mapping.clone();
    rsx! {
        div { class: "mapping-editor",
            match mapping {
                BindingMapping::Format { precision, unit } => {
                    let precision_unit = unit.clone();
                    let display_unit = unit.clone();
                    rsx! {
                        div { class: "field-grid two",
                            label { class: "field", r#for: "binding-precision",
                                span { "精度（留空继承）" }
                                input {
                                    id: "binding-precision",
                                    class: "text-input numeric",
                                    "data-testid": "binding-precision",
                                    value: precision.map(|value| value.to_string()).unwrap_or_default(),
                                    oninput: move |event| {
                                        on_change.call(BindingMapping::Format {
                                            precision: event.value().trim().parse::<u8>().ok(),
                                            unit: precision_unit.clone(),
                                        });
                                    },
                                }
                            }
                            label { class: "field", r#for: "binding-unit",
                                span { "单位（留空继承）" }
                                input {
                                    id: "binding-unit",
                                    class: "text-input",
                                    "data-testid": "binding-unit",
                                    value: display_unit.clone().unwrap_or_default(),
                                    oninput: move |event| {
                                        let value = event.value();
                                        on_change.call(BindingMapping::Format {
                                            precision,
                                            unit: if value.is_empty() { None } else { Some(value) },
                                        });
                                    },
                                }
                            }
                        }
                    }
                },
                BindingMapping::BoolColor { true_color, false_color } => rsx! {
                    div { class: "field-grid two",
                        label { class: "field color-field", r#for: "binding-true-color",
                            span { "True" }
                            input {
                                id: "binding-true-color",
                                r#type: "color",
                                "data-testid": "binding-true-color",
                                value: "{true_color.0}",
                                onchange: {
                                    let false_color = false_color.clone();
                                    move |event| on_change.call(BindingMapping::BoolColor {
                                        true_color: HexColor(event.value()),
                                        false_color: false_color.clone(),
                                    })
                                },
                            }
                        }
                        label { class: "field color-field", r#for: "binding-false-color",
                            span { "False" }
                            input {
                                id: "binding-false-color",
                                r#type: "color",
                                "data-testid": "binding-false-color",
                                value: "{false_color.0}",
                                onchange: {
                                    let true_color = true_color.clone();
                                    move |event| on_change.call(BindingMapping::BoolColor {
                                        true_color: true_color.clone(),
                                        false_color: HexColor(event.value()),
                                    })
                                },
                            }
                        }
                    }
                },
                BindingMapping::ThresholdColor { thresholds } => {
                    rsx! {
                        div { class: "threshold-list",
                            for (index, stop) in thresholds.iter().cloned().enumerate() {
                                div { key: "{index}", class: "threshold-row",
                                    input {
                                        aria_label: format!("阈值 {} 的数值", index + 1),
                                        class: "text-input numeric",
                                        value: "{stop.value}",
                                        oninput: {
                                            let mut thresholds = thresholds.clone();
                                            move |event| {
                                                if let Ok(value) = event.value().parse::<f32>() {
                                                    thresholds[index].value = value;
                                                    on_change.call(BindingMapping::ThresholdColor { thresholds: thresholds.clone() });
                                                }
                                            }
                                        },
                                    }
                                    input {
                                        aria_label: format!("阈值 {} 的颜色", index + 1),
                                        r#type: "color",
                                        value: "{stop.color.0}",
                                        onchange: {
                                            let mut thresholds = thresholds.clone();
                                            move |event| {
                                                thresholds[index].color = HexColor(event.value());
                                                on_change.call(BindingMapping::ThresholdColor { thresholds: thresholds.clone() });
                                            }
                                        },
                                    }
                                    button {
                                        class: "icon-button tiny",
                                        aria_label: format!("删除阈值 {}", index + 1),
                                        disabled: thresholds.len() <= 1,
                                        onclick: {
                                            let mut thresholds = thresholds.clone();
                                            move |_| {
                                                if thresholds.len() > 1 {
                                                    thresholds.remove(index);
                                                    on_change.call(BindingMapping::ThresholdColor { thresholds: thresholds.clone() });
                                                }
                                            }
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                        button {
                            class: "mini-button",
                            disabled: thresholds.len() >= 8,
                            onclick: {
                                let mut thresholds = thresholds.clone();
                                move |_| {
                                    thresholds.push(hmi_core::ThresholdStop {
                                        value: thresholds.last().map(|stop| stop.value + 1.0).unwrap_or(0.0),
                                        color: HexColor("#37e6d4".to_string()),
                                    });
                                    on_change.call(BindingMapping::ThresholdColor { thresholds: thresholds.clone() });
                                }
                            },
                            "+ 阈值"
                        }
                    }
                }
                BindingMapping::Range { input_min, input_max, output_min, output_max, clamp } => rsx! {
                    div { class: "field-grid two",
                        RangeInput { label: "输入最小", test_id: "binding-range-input-min", value: input_min, on_commit: move |value| on_change.call(BindingMapping::Range { input_min: value, input_max, output_min, output_max, clamp }) }
                        RangeInput { label: "输入最大", test_id: "binding-range-input-max", value: input_max, on_commit: move |value| on_change.call(BindingMapping::Range { input_min, input_max: value, output_min, output_max, clamp }) }
                        RangeInput { label: "输出最小", test_id: "binding-range-output-min", value: output_min, on_commit: move |value| on_change.call(BindingMapping::Range { input_min, input_max, output_min: value, output_max, clamp }) }
                        RangeInput { label: "输出最大", test_id: "binding-range-output-max", value: output_max, on_commit: move |value| on_change.call(BindingMapping::Range { input_min, input_max, output_min, output_max: value, clamp }) }
                    }
                    label { class: "check-field", r#for: "binding-range-clamp",
                        input {
                            id: "binding-range-clamp",
                            r#type: "checkbox",
                            "data-testid": "binding-range-clamp",
                            checked: clamp,
                            onchange: move |event| on_change.call(BindingMapping::Range { input_min, input_max, output_min, output_max, clamp: event.checked() }),
                        }
                        span { "超范围钳制" }
                    }
                },
                BindingMapping::Direct => rsx! {
                    div { class: "hint-copy", "直接映射，无需额外参数。" }
                },
            }
        }
    }
}

#[component]
fn RangeInput(
    label: &'static str,
    test_id: &'static str,
    value: f32,
    on_commit: EventHandler<f32>,
) -> Element {
    let mut draft = use_signal(|| format_number(value));
    let initial = format_number(value);
    let field_id = format!("field-{test_id}");
    rsx! {
        div { class: "field",
            label { r#for: "{field_id}", "{label}" }
            input {
                id: "{field_id}",
                class: "text-input numeric",
                "data-testid": "{test_id}",
                value: "{draft}",
                oninput: move |event| draft.set(event.value()),
                onchange: move |_| {
                    if let Ok(parsed) = draft().trim().parse::<f32>()
                        && parsed.is_finite()
                    {
                        on_commit.call(parsed);
                    }
                },
                onkeydown: move |event| {
                    event.stop_propagation();
                    if event.key() == Key::Named(NamedKey::Escape) {
                        draft.set(initial.clone());
                    }
                },
            }
        }
    }
}

fn format_points(points: &[[f32; 2]]) -> String {
    points
        .iter()
        .map(|point| format!("{},{}", format_number(point[0]), format_number(point[1])))
        .collect::<Vec<_>>()
        .join(";")
}

fn parse_points(value: &str) -> Result<Vec<[f32; 2]>, String> {
    let mut points = Vec::new();
    for pair in value.split(';') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let mut values = pair.split(',');
        let x = values
            .next()
            .ok_or_else(|| "折点格式应为 x,y;x,y".to_string())?
            .trim()
            .parse::<f32>()
            .map_err(|_| "折点 X 不是数值".to_string())?;
        let y = values
            .next()
            .ok_or_else(|| "折点格式应为 x,y;x,y".to_string())?
            .trim()
            .parse::<f32>()
            .map_err(|_| "折点 Y 不是数值".to_string())?;
        if !x.is_finite() || !y.is_finite() {
            return Err("折点必须是有限数值".to_string());
        }
        points.push([x, y]);
    }
    if points.len() < 2 {
        return Err("管线至少需要两个折点".to_string());
    }
    Ok(points)
}

fn target_value(target: BindingTarget) -> &'static str {
    match target {
        BindingTarget::Text => "text",
        BindingTarget::Color => "color",
        BindingTarget::Visible => "visible",
        BindingTarget::Progress => "progress",
        BindingTarget::RotationSpeed => "rotation_speed",
    }
}

fn parse_target(value: &str) -> BindingTarget {
    match value {
        "color" => BindingTarget::Color,
        "visible" => BindingTarget::Visible,
        "progress" => BindingTarget::Progress,
        "rotation_speed" => BindingTarget::RotationSpeed,
        _ => BindingTarget::Text,
    }
}

fn mapping_value(mapping: &BindingMapping) -> &'static str {
    match mapping {
        BindingMapping::Format { .. } => "format",
        BindingMapping::BoolColor { .. } => "bool_color",
        BindingMapping::ThresholdColor { .. } => "threshold_color",
        BindingMapping::Range { .. } => "range",
        BindingMapping::Direct => "direct",
    }
}

fn mapping_label_for_key(key: &str) -> &'static str {
    match key {
        "format" => "格式化",
        "bool_color" => "布尔颜色",
        "threshold_color" => "阈值颜色",
        "range" => "范围映射",
        _ => "直接",
    }
}

fn text_align_value(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

fn parse_text_align(value: &str) -> TextAlign {
    match value {
        "center" => TextAlign::Center,
        "right" => TextAlign::Right,
        _ => TextAlign::Left,
    }
}

fn equipment_value(equipment: EquipmentKind) -> &'static str {
    match equipment {
        EquipmentKind::Fan => "fan",
        EquipmentKind::Pump => "pump",
        EquipmentKind::Ahu => "ahu",
        EquipmentKind::Valve => "valve",
    }
}

fn parse_equipment(value: &str) -> EquipmentKind {
    match value {
        "pump" => EquipmentKind::Pump,
        "ahu" => EquipmentKind::Ahu,
        "valve" => EquipmentKind::Valve,
        _ => EquipmentKind::Fan,
    }
}

#[allow(dead_code)]
fn suppress_unused(label: &str, ids: &[&str], values: [f32; 3], targets: &[BindingTarget]) {
    let _ = (label, ids, values, targets, format_hmi_error);
}
