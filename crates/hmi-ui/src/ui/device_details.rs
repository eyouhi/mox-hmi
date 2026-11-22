use super::common::{format_number, node_kind_label, sanitize_test_id};
use crate::bridge;
use dioxus::prelude::*;
use hmi_core::{Binding, ButtonAction, Screen, TagSnapshot, TagValue, format_tag_value};

type RuntimeNodeInfo = (String, Option<String>, Vec<Binding>, Option<String>);

#[component]
pub fn DeviceDetails() -> Element {
    let _ = bridge::TAG_REVISION();
    let _ = bridge::SELECTED_NODE();
    let project = bridge::project();
    let Some(selected) = bridge::selected_node() else {
        return rsx! {};
    };
    let Some((name, label, bindings, action_tag)) = runtime_node(&project, &selected) else {
        return rsx! {};
    };
    let snapshot = bridge::tag_snapshot();
    let status = runtime_status(&snapshot, &bindings, action_tag.as_deref());
    let kind = node_kind_label(&project, &selected);

    rsx! {
        div { class: "device-details", "data-testid": "device-details",
            div { class: "device-details-head",
                div {
                    div { class: "eyebrow", "{kind}" }
                    h2 { class: "device-title", "data-testid": "device-details-title", "{name}" }
                    if let Some(label) = label {
                        div { class: "device-label-text", "{label}" }
                    }
                }
                button {
                    class: "icon-button",
                    "data-testid": "device-details-close",
                    aria_label: "关闭设备详情",
                    onclick: move |_| bridge::select_node(None),
                    "×"
                }
            }
            if let Some((text, class)) = status {
                div {
                    class: "device-status {class}",
                    "data-testid": "device-details-status",
                    span { class: "status-dot", aria_hidden: "true" }
                    "{text}"
                }
            }
            ul { class: "device-binding-list",
                if bindings.is_empty() {
                    li { class: "empty-copy", "此设备未配置 Tag 绑定" }
                }
                for binding in bindings {
                    {
                        let tag = project.tag(&binding.tag_id);
                        let value = snapshot
                            .get(&binding.tag_id)
                            .map(|value| match tag {
                                Some(tag) => format_tag_value(value, tag.precision, &tag.unit),
                                None => value.display_text(),
                            })
                            .unwrap_or_else(|| "—".to_string());
                        rsx! {
                            li { key: "{binding.tag_id}", class: "device-tag-row",
                                div {
                                    div { class: "device-tag-name", {tag.map(|tag| tag.name.clone()).unwrap_or_else(|| binding.tag_id.clone())} }
                                    code { class: "device-tag-id", "{binding.tag_id}" }
                                }
                                strong {
                                    class: "device-tag-value",
                                    "data-testid": "device-tag-{sanitize_test_id(&binding.tag_id)}",
                                    "{value}"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "device-hint", "运行态写入不会进入工程、撤销栈或自动保存。" }
        }
    }
}

fn runtime_node(project: &hmi_core::ProjectDocument, node_id: &str) -> Option<RuntimeNodeInfo> {
    for screen in &project.screens {
        match screen {
            Screen::TwoD { nodes, .. } => {
                if let Some(node) = nodes.iter().find(|node| node.id == node_id) {
                    let action_tag = match &node.widget {
                        hmi_core::Widget2D::Button { action, .. } => match action {
                            ButtonAction::ToggleTag { tag_id }
                            | ButtonAction::SetTag { tag_id, .. } => Some(tag_id.clone()),
                        },
                        _ => None,
                    };
                    return Some((node.name.clone(), None, node.bindings.clone(), action_tag));
                }
            }
            Screen::ThreeD { nodes, .. } => {
                if let Some(node) = nodes.iter().find(|node| node.id == node_id) {
                    return Some((
                        node.name.clone(),
                        node.label.clone(),
                        node.bindings.clone(),
                        None,
                    ));
                }
            }
        }
    }
    None
}

fn runtime_status(
    snapshot: &TagSnapshot,
    bindings: &[Binding],
    action_tag: Option<&str>,
) -> Option<(String, &'static str)> {
    if bindings.is_empty() && action_tag.is_none() {
        return None;
    }
    let mut tags: Vec<&str> = bindings
        .iter()
        .map(|binding| binding.tag_id.as_str())
        .collect();
    if let Some(tag) = action_tag {
        tags.push(tag);
    }
    let fault = tags.iter().any(|tag| {
        tag.ends_with(".fault")
            && snapshot
                .get(tag)
                .and_then(TagValue::as_bool)
                .unwrap_or(false)
    });
    if fault {
        return Some(("故障".to_string(), "is-fault"));
    }
    let running = tags.iter().any(|tag| {
        tag.ends_with(".run")
            && snapshot
                .get(tag)
                .and_then(TagValue::as_bool)
                .unwrap_or(false)
    });
    if running {
        Some(("运行".to_string(), "is-running"))
    } else {
        Some(("停止".to_string(), "is-stopped"))
    }
}

#[allow(dead_code)]
fn format_value_for_badge(value: &TagValue) -> String {
    match value {
        TagValue::Number(value) => format_number(*value),
        other => other.display_text(),
    }
}
