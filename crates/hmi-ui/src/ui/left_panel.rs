use super::common::{
    execute_command, focus_canvas, format_hmi_error, report_error, sanitize_test_id,
};
use crate::bridge;
use dioxus::prelude::*;
use hmi_core::{
    Binding, BindingMapping, BindingTarget, EditorCommand, ProjectDocument, Screen, SimulatorKind,
    TagDefinition, TagValue, TagValueType,
};

#[component]
pub fn LeftPanel() -> Element {
    let _ = bridge::DOCUMENT_REVISION();
    let _ = bridge::TAG_REVISION();
    let _ = bridge::ACTIVE_SCREEN();
    let _ = bridge::SELECTED_NODE();
    rsx! {
        aside { class: "left-panel", "data-testid": "left-panel", aria_label: "工程结构",
            ScreenTree {}
            ComponentPalette {}
            TagPanel {}
        }
    }
}

#[component]
fn ScreenTree() -> Element {
    let _ = bridge::DOCUMENT_REVISION();
    let project = bridge::project();
    let active_id = bridge::active_screen().unwrap_or_else(|| project.startup_screen.clone());
    let selected = bridge::selected_nodes();
    let mut renaming = use_signal(|| false);
    let mut name_draft = use_signal(String::new);
    let mut creating_screen = use_signal(|| false);
    let active_screen = project.screen(&active_id).cloned();
    let active_name = active_screen
        .as_ref()
        .map(|screen| screen.name().to_string())
        .unwrap_or_default();
    let active_nodes: Vec<(String, String, bool)> = match active_screen.as_ref() {
        Some(Screen::TwoD { nodes, .. }) => nodes
            .iter()
            .map(|node| (node.id.clone(), node.name.clone(), node.locked))
            .collect(),
        Some(Screen::ThreeD { nodes, .. }) => nodes
            .iter()
            .map(|node| (node.id.clone(), node.name.clone(), node.locked))
            .collect(),
        None => Vec::new(),
    };

    rsx! {
        section { class: "panel-section screen-section", "data-testid": "screen-tree",
            div { class: "section-heading",
                div {
                    div { class: "eyebrow", "工程结构" }
                    h2 { class: "section-title", "画面" }
                }
                div { class: "section-actions",
                    button {
                        class: "icon-button",
                        title: "新增画面",
                        aria_label: "新增画面",
                        "data-testid": "add-screen",
                        aria_expanded: if creating_screen() { "true" } else { "false" },
                        onclick: move |_| creating_screen.set(!creating_screen()),
                        "+"
                    }
                    button {
                        class: "icon-button",
                        title: "重命名画面",
                        aria_label: "重命名画面",
                        onclick: move |_| {
                            name_draft.set(active_name.clone());
                            renaming.set(true);
                        },
                        "改"
                    }
                    button {
                        class: "icon-button danger",
                        title: "删除画面",
                        aria_label: "删除画面",
                        onclick: move |_| delete_screen(&active_id),
                        "删"
                    }
                }
            }

            if creating_screen() {
                div {
                    class: "screen-create-menu",
                    role: "group",
                    aria_label: "选择画面类型",
                    button {
                        class: "screen-create-option",
                        "data-testid": "add-screen-2d",
                        onclick: move |_| {
                            creating_screen.set(false);
                            add_screen(false);
                        },
                        span { class: "screen-create-mark", "2D" }
                        span {
                            strong { "空白 2D 画面" }
                            small { "工艺图、仪表、按钮" }
                        }
                    }
                    button {
                        class: "screen-create-option",
                        "data-testid": "add-screen-3d",
                        onclick: move |_| {
                            creating_screen.set(false);
                            add_screen(true);
                        },
                        span { class: "screen-create-mark", "3D" }
                        span {
                            strong { "3D 机房画面" }
                            small { "设备、管线、数字孪生" }
                        }
                    }
                }
            }

            if renaming() {
                div { class: "inline-editor",
                    input {
                        id: "screen-name-input",
                        class: "text-input",
                        aria_label: "画面名称",
                        value: "{name_draft}",
                        autofocus: true,
                        oninput: move |event| name_draft.set(event.value()),
                        onkeydown: {
                            let active_id = active_id.clone();
                            move |event| {
                                event.stop_propagation();
                                if event.key() == Key::Named(NamedKey::Enter) {
                                    event.prevent_default();
                                    if event.is_composing() {
                                        return;
                                    }
                                    if rename_screen(&active_id, name_draft().trim()) {
                                        renaming.set(false);
                                    }
                                } else if event.key() == Key::Named(NamedKey::Escape) {
                                    renaming.set(false);
                                }
                            }
                        },
                    }
                    button {
                        class: "mini-button",
                        onclick: {
                            let active_id = active_id.clone();
                            move |_| {
                                if rename_screen(&active_id, name_draft().trim()) {
                                    renaming.set(false);
                                }
                            }
                        },
                        "确定"
                    }
                    button {
                        class: "mini-button ghost",
                        onclick: move |_| renaming.set(false),
                        "取消"
                    }
                }
            }

            nav { class: "screen-list", aria_label: "画面切换",
                for screen in project.screens.iter() {
                    {
                        let screen_id = screen.id().to_string();
                        let test_id = format!("screen-tab-{}", sanitize_test_id(&screen_id));
                        let is_active = screen_id == active_id;
                        let kind = if screen.is_two_d() { "2D" } else { "3D" };
                        rsx! {
                            button {
                                key: "{screen_id}",
                                class: if is_active { "screen-tab is-active" } else { "screen-tab" },
                                aria_current: if is_active { "page" } else { "false" },
                                "data-testid": "{test_id}",
                                onclick: {
                                    let screen_id = screen_id.clone();
                                    move |_| bridge::select_screen(&screen_id)
                                },
                                span { class: "screen-kind", "{kind}" }
                                span { class: "screen-name", "{screen.name()}" }
                            }
                        }
                    }
                }
            }

            ul { class: "node-list", "data-testid": "node-list",
                if active_nodes.is_empty() {
                    li { class: "empty-copy", "当前画面没有节点" }
                }
                for (node_id, name, locked) in active_nodes {
                    {
                        let test_id = format!("node-row-{}", sanitize_test_id(&node_id));
                        let is_selected = selected.contains(&node_id);
                        rsx! {
                            li {
                                key: "{node_id}",
                                button {
                                    class: if is_selected { "node-row is-active" } else { "node-row" },
                                    "data-testid": "{test_id}",
                                    onclick: {
                                        let node_id = node_id.clone();
                                        move |event| {
                                            let modifiers = event.modifiers();
                                            if modifiers.shift() || modifiers.ctrl() || modifiers.meta() {
                                                bridge::toggle_node_selection(&node_id);
                                            } else {
                                                bridge::select_node(Some(node_id.clone()));
                                            }
                                            focus_canvas();
                                        }
                                    },
                                    span { class: "node-dot", aria_hidden: "true" }
                                    span { class: "node-label", "{name}" }
                                    if locked {
                                        span { class: "lock-mark", "锁" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn rename_screen(active_id: &str, name: &str) -> bool {
    if name.is_empty() {
        bridge::set_notice(bridge::NoticeLevel::Warning, "画面名称不能为空");
        return false;
    }
    match bridge::execute(EditorCommand::RenameScreen {
        screen_id: active_id.to_string(),
        name: name.to_string(),
    }) {
        Ok(_) => {
            bridge::set_notice(bridge::NoticeLevel::Info, "画面已重命名");
            true
        }
        Err(error) => {
            report_error(error);
            false
        }
    }
}

fn add_screen(three_d: bool) {
    match bridge::create_screen(three_d) {
        Ok(_) => focus_canvas(),
        Err(error) => bridge::set_notice(bridge::NoticeLevel::Error, error),
    }
}

fn delete_screen(active_id: &str) {
    let project = bridge::project();
    if project.screens.len() <= 1 {
        bridge::set_notice(bridge::NoticeLevel::Warning, "工程至少需要保留一个画面");
        return;
    }
    let next = project
        .screens
        .iter()
        .find(|screen| screen.id() != active_id)
        .map(|screen| screen.id().to_string());
    match bridge::execute(EditorCommand::DeleteScreen {
        screen_id: active_id.to_string(),
        new_startup: next.clone(),
    }) {
        Ok(_) => {
            if let Some(next) = next {
                bridge::select_screen(&next);
            }
        }
        Err(error) => report_error(error),
    }
}

#[component]
fn ComponentPalette() -> Element {
    let _ = bridge::DOCUMENT_REVISION();
    let _ = bridge::ACTIVE_SCREEN();
    let _ = bridge::GAMEPAD_STATUS();
    let project = bridge::project();
    let active_id = bridge::active_screen().unwrap_or_else(|| project.startup_screen.clone());
    let is_two_d = project
        .screen(&active_id)
        .is_some_and(|screen| screen.is_two_d());
    let gamepad_connected = bridge::gamepad_status().connected;

    rsx! {
        section { class: "panel-section palette-section", "data-testid": "component-palette",
            div { class: "section-heading compact",
                div {
                    div { class: "eyebrow", "组件库" }
                    h2 { class: "section-title", if is_two_d { "2D 图元" } else { "3D 设备" } }
                }
            }
            div { class: "palette-grid",
                if is_two_d {
                    PaletteButton { test_id: "add-panel", label: "面板", glyph: "▦", kind: "panel" }
                    PaletteButton { test_id: "add-label", label: "文本", glyph: "T", kind: "label" }
                    PaletteButton { test_id: "add-value-card", label: "数值卡", glyph: "123", kind: "value-card" }
                    PaletteButton { test_id: "add-status-lamp", label: "状态灯", glyph: "●", kind: "status-lamp" }
                    PaletteButton { test_id: "add-bar-gauge", label: "仪表条", glyph: "▤", kind: "bar-gauge" }
                    PaletteButton { test_id: "add-button", label: "按钮", glyph: "◉", kind: "button" }
                    PaletteButton { test_id: "add-pipe", label: "管线", glyph: "┄", kind: "pipe" }
                    PaletteButton { test_id: "add-equipment-symbol", label: "设备", glyph: "◇", kind: "equipment" }
                } else {
                    PaletteButton { test_id: "add-ahu", label: "AHU", glyph: "▣", kind: "ahu" }
                    PaletteButton { test_id: "add-fan", label: "风机", glyph: "◌", kind: "fan" }
                    PaletteButton { test_id: "add-pump", label: "水泵", glyph: "⊙", kind: "pump" }
                    PaletteButton { test_id: "add-duct", label: "风管", glyph: "▤", kind: "duct" }
                    PaletteButton { test_id: "add-pipe-3d", label: "水管", glyph: "━", kind: "pipe-3d" }
                    PaletteButton { test_id: "add-valve", label: "阀门", glyph: "◎", kind: "valve" }
                }
            }
            if gamepad_connected {
                p { class: "panel-gamepad-hint", "手柄：L3+Create → 组件库，↑↓ 选择，× 添加" }
            }
        }
    }
}

#[component]
fn PaletteButton(
    test_id: &'static str,
    label: &'static str,
    glyph: &'static str,
    kind: &'static str,
) -> Element {
    rsx! {
        button {
            class: "palette-button",
            "data-testid": "{test_id}",
            title: "添加{label}",
            onclick: move |_| add_node(kind),
            span { class: "palette-glyph", aria_hidden: "true", "{glyph}" }
            span { "{label}" }
        }
    }
}

fn add_node(kind: &str) {
    match bridge::add_palette_node(kind) {
        Ok(_) => focus_canvas(),
        Err(error) => bridge::set_notice(bridge::NoticeLevel::Error, error),
    }
}

#[component]
fn TagPanel() -> Element {
    let _ = bridge::DOCUMENT_REVISION();
    let _ = bridge::TAG_REVISION();
    let _ = bridge::GAMEPAD_STATUS();
    let project = bridge::project();
    let snapshot = bridge::tag_snapshot();
    let selected_tag = bridge::SELECTED_TAG();
    let gamepad_connected = bridge::gamepad_status().connected;

    rsx! {
        section { class: "panel-section tag-section", "data-testid": "tag-list",
            div { class: "section-heading compact",
                div {
                    div { class: "eyebrow", "数据字典" }
                    h2 { class: "section-title", "Tags · {project.tags.len()}" }
                }
                button {
                    class: "mini-button",
                    "data-testid": "add-tag",
                    onclick: move |_| {
                        let id = next_tag_id(&project);
                        let tag = TagDefinition {
                            id: id.clone(),
                            name: "新 Tag".to_string(),
                            value_type: TagValueType::Number,
                            unit: String::new(),
                            min: Some(0.0),
                            max: Some(100.0),
                            precision: 0,
                            initial: TagValue::Number(0.0),
                            writable: false,
                            simulator: SimulatorKind::Constant,
                        };
                        match bridge::execute(EditorCommand::AddTag { tag }) {
                            Ok(_) => *bridge::SELECTED_TAG.write() = Some(id),
                            Err(error) => report_error(error),
                        }
                    },
                    "+ Tag"
                }
            }
            if gamepad_connected {
                p { class: "panel-gamepad-hint", "手柄：L3+Create → 数据字典，可浏览并选中 Tag" }
            }
            ul { class: "tag-rows",
                for tag in project.tags.iter() {
                    {
                        let tag_id = tag.id.clone();
                        let row_id = format!("tag-row-{}", sanitize_test_id(&tag_id));
                        let is_selected = selected_tag.as_deref() == Some(tag_id.as_str());
                        let value = snapshot
                            .get(&tag_id)
                            .map(|value| value.display_text())
                            .unwrap_or_else(|| "—".to_string());
                        rsx! {
                            li {
                                key: "{tag_id}",
                                button {
                                    class: if is_selected { "tag-row is-active" } else { "tag-row" },
                                    "data-testid": "{row_id}",
                                    onclick: {
                                        let tag_id = tag_id.clone();
                                        move |_| *bridge::SELECTED_TAG.write() = Some(tag_id.clone())
                                    },
                                    span { class: "tag-name", "{tag.name}" }
                                    span { class: "tag-value", "{value}" }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(tag_id) = selected_tag {
                if let Some(tag) = project.tag(&tag_id).cloned() {
                    TagEditor {
                        key: "{tag.id}-{bridge::DOCUMENT_REVISION()}",
                        tag,
                        on_deleted: move |_| *bridge::SELECTED_TAG.write() = None,
                    }
                }
            }
        }
    }
}

fn next_tag_id(project: &ProjectDocument) -> String {
    for index in 1..=10_000 {
        let candidate = format!("bas.custom.{index}");
        if project.tags.iter().all(|tag| tag.id != candidate) {
            return candidate;
        }
    }
    "bas.custom.new".to_string()
}

#[component]
fn TagEditor(tag: TagDefinition, on_deleted: EventHandler<()>) -> Element {
    let mut name = use_signal(|| tag.name.clone());
    let mut unit = use_signal(|| tag.unit.clone());
    let mut min = use_signal(|| tag.min.map(|value| value.to_string()).unwrap_or_default());
    let mut max = use_signal(|| tag.max.map(|value| value.to_string()).unwrap_or_default());
    let mut precision = use_signal(|| tag.precision.to_string());
    let mut writable = use_signal(|| tag.writable);
    let mut simulator = use_signal(|| tag.simulator);
    let mut initial = use_signal(|| match &tag.initial {
        TagValue::Number(value) => value.to_string(),
        TagValue::Bool(value) => value.to_string(),
        TagValue::Enum(value) => value.clone(),
    });
    let tag_id = tag.id.clone();
    let tag_type = tag.value_type;

    let apply = {
        let tag_id = tag_id.clone();
        move |_| {
            let min_value = if tag_type == TagValueType::Number || !min().trim().is_empty() {
                match min().trim().parse::<f32>() {
                    Ok(value) if value.is_finite() => Some(value),
                    _ => {
                        bridge::set_notice(bridge::NoticeLevel::Error, "Tag min 必须是有限数值");
                        return;
                    }
                }
            } else {
                None
            };
            let max_value = if tag_type == TagValueType::Number || !max().trim().is_empty() {
                match max().trim().parse::<f32>() {
                    Ok(value) if value.is_finite() => Some(value),
                    _ => {
                        bridge::set_notice(bridge::NoticeLevel::Error, "Tag max 必须是有限数值");
                        return;
                    }
                }
            } else {
                None
            };
            let Ok(precision_value) = precision().parse::<u8>() else {
                bridge::set_notice(
                    bridge::NoticeLevel::Error,
                    "Tag precision 必须是 0–6 的整数",
                );
                return;
            };
            let initial_value = match tag_type {
                TagValueType::Number => match initial().trim().parse::<f32>() {
                    Ok(value) if value.is_finite() => TagValue::Number(value),
                    _ => {
                        bridge::set_notice(bridge::NoticeLevel::Error, "Tag 初始值必须是有限数值");
                        return;
                    }
                },
                TagValueType::Bool => match initial().trim() {
                    "true" | "运行" => TagValue::Bool(true),
                    "false" | "停止" => TagValue::Bool(false),
                    _ => {
                        bridge::set_notice(
                            bridge::NoticeLevel::Error,
                            "Bool 初始值使用 true/false 或 运行/停止",
                        );
                        return;
                    }
                },
                TagValueType::Enum => TagValue::Enum(initial()),
            };
            let updated = TagDefinition {
                id: tag_id.clone(),
                name: name().trim().to_string(),
                value_type: tag_type,
                unit: unit().trim().to_string(),
                min: min_value,
                max: max_value,
                precision: precision_value,
                initial: initial_value,
                writable: writable(),
                simulator: simulator(),
            };
            execute_command(
                EditorCommand::UpdateTag { tag: updated },
                Some("Tag 已更新"),
            );
        }
    };

    let delete = {
        let tag_id = tag_id.clone();
        move |_| match bridge::execute(EditorCommand::DeleteTag {
            tag_id: tag_id.clone(),
        }) {
            Ok(_) => on_deleted.call(()),
            Err(error) => bridge::set_notice(bridge::NoticeLevel::Error, format_hmi_error(&error)),
        }
    };

    let simulators: &[SimulatorKind] = match tag_type {
        TagValueType::Number => &[
            SimulatorKind::Constant,
            SimulatorKind::Sine,
            SimulatorKind::Step,
            SimulatorKind::RandomWalk,
            SimulatorKind::Manual,
        ],
        TagValueType::Bool => &[
            SimulatorKind::Constant,
            SimulatorKind::Pulse,
            SimulatorKind::Manual,
        ],
        TagValueType::Enum => &[SimulatorKind::Constant, SimulatorKind::Manual],
    };

    rsx! {
        div {
            class: "sub-editor tag-editor",
            onkeydown: move |event| event.stop_propagation(),
            div { class: "sub-editor-head",
                div {
                    div { class: "eyebrow", "Tag 编辑器" }
                    code { class: "id-chip", "{tag_id}" }
                }
            }
            label { class: "field", r#for: "tag-name-input",
                span { "名称" }
                input {
                    id: "tag-name-input",
                    class: "text-input",
                    "data-testid": "tag-name",
                    value: "{name}",
                    oninput: move |event| name.set(event.value()),
                }
            }
            div { class: "field-grid two",
                label { class: "field", r#for: "tag-unit-input",
                    span { "单位" }
                    input {
                        id: "tag-unit-input",
                        class: "text-input",
                        "data-testid": "tag-unit",
                        value: "{unit}",
                        oninput: move |event| unit.set(event.value()),
                    }
                }
                label { class: "field", r#for: "tag-precision-input",
                    span { "精度" }
                    input {
                        id: "tag-precision-input",
                        class: "text-input numeric",
                        r#type: "number",
                        min: "0",
                        max: "6",
                        "data-testid": "tag-precision",
                        value: "{precision}",
                        oninput: move |event| precision.set(event.value()),
                    }
                }
                label { class: "field", r#for: "tag-min-input",
                    span { "最小值" }
                    input {
                        id: "tag-min-input",
                        class: "text-input numeric",
                        "data-testid": "tag-min",
                        value: "{min}",
                        oninput: move |event| min.set(event.value()),
                    }
                }
                label { class: "field", r#for: "tag-max-input",
                    span { "最大值" }
                    input {
                        id: "tag-max-input",
                        class: "text-input numeric",
                        "data-testid": "tag-max",
                        value: "{max}",
                        oninput: move |event| max.set(event.value()),
                    }
                }
            }
            label { class: "field", r#for: "tag-initial-input",
                span { "初始值" }
                input {
                    id: "tag-initial-input",
                    class: "text-input",
                    "data-testid": "tag-initial",
                    value: "{initial}",
                    oninput: move |event| initial.set(event.value()),
                }
            }
            label { class: "field", r#for: "tag-simulator-input",
                span { "仿真器" }
                select {
                    id: "tag-simulator-input",
                    class: "select-input",
                    "data-testid": "tag-simulator",
                    value: simulator_value(simulator()),
                    onchange: move |event| simulator.set(parse_simulator(&event.value())),
                    for option in simulators {
                        option {
                            value: simulator_value(*option),
                            selected: simulator() == *option,
                            "{simulator_label(*option)}"
                        }
                    }
                }
            }
            label { class: "check-field", r#for: "tag-writable-input",
                input {
                    id: "tag-writable-input",
                    r#type: "checkbox",
                    "data-testid": "tag-writable",
                    checked: writable(),
                    onchange: move |event| writable.set(event.checked()),
                }
                span { "允许运行态写入" }
            }
            div { class: "sub-editor-actions",
                button { class: "mini-button primary", "data-testid": "apply-tag", onclick: apply, "应用 Tag" }
                button { class: "mini-button danger", "data-testid": "delete-tag", onclick: delete, "删除 Tag" }
            }
        }
    }
}

fn simulator_value(simulator: SimulatorKind) -> &'static str {
    match simulator {
        SimulatorKind::Constant => "constant",
        SimulatorKind::Sine => "sine",
        SimulatorKind::Pulse => "pulse",
        SimulatorKind::Step => "step",
        SimulatorKind::RandomWalk => "random_walk",
        SimulatorKind::Manual => "manual",
    }
}

fn simulator_label(simulator: SimulatorKind) -> &'static str {
    match simulator {
        SimulatorKind::Constant => "常量",
        SimulatorKind::Sine => "正弦",
        SimulatorKind::Pulse => "脉冲",
        SimulatorKind::Step => "阶跃",
        SimulatorKind::RandomWalk => "随机游走",
        SimulatorKind::Manual => "手动",
    }
}

fn parse_simulator(value: &str) -> SimulatorKind {
    match value {
        "sine" => SimulatorKind::Sine,
        "pulse" => SimulatorKind::Pulse,
        "step" => SimulatorKind::Step,
        "random_walk" => SimulatorKind::RandomWalk,
        "manual" => SimulatorKind::Manual,
        _ => SimulatorKind::Constant,
    }
}

#[allow(dead_code)]
fn unused_binding_types() -> (
    Option<Binding>,
    Option<BindingMapping>,
    Option<BindingTarget>,
) {
    (None, None, None)
}
