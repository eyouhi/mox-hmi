use crate::bridge::{self, EngineStatus, SaveStatus};
use dioxus::prelude::*;

#[component]
pub fn StatusBar() -> Element {
    let _ = bridge::DOCUMENT_REVISION();
    let _ = bridge::TAG_REVISION();
    let _ = bridge::ACTIVE_SCREEN();
    let _ = bridge::SELECTED_NODE();
    let project = bridge::project();
    let active_id = bridge::active_screen().unwrap_or_else(|| project.startup_screen.clone());
    let screen = project.screen(&active_id);
    let kind = screen
        .map(|screen| if screen.is_two_d() { "2D" } else { "3D" })
        .unwrap_or("—");
    let screen_name = screen.map(|screen| screen.name()).unwrap_or("未选择");
    let selected_nodes = bridge::selected_nodes();
    let selected = if selected_nodes.len() > 1 {
        format!(
            "{} 个 · {}",
            selected_nodes.len(),
            selected_nodes.last().cloned().unwrap_or_default()
        )
    } else {
        selected_nodes
            .first()
            .cloned()
            .unwrap_or_else(|| "未选择".to_string())
    };
    let zoom = bridge::zoom_2d() * 100.0;
    let save = match bridge::SAVE_STATUS() {
        SaveStatus::Saved => "已保存",
        SaveStatus::Pending => "保存中",
        SaveStatus::Failed => "保存失败",
    };
    let engine_error = match bridge::ENGINE_STATUS() {
        EngineStatus::Failed(error) => Some(error),
        _ => None,
    };

    rsx! {
        footer {
            class: "status-bar",
            "data-testid": "status-bar",
            aria_label: "状态栏",
            div { class: "status-item",
                span { class: "status-label", "节点" }
                code { class: "status-value", "data-testid": "selection-status", "{selected}" }
            }
            div { class: "status-item",
                span { class: "status-label", "画面" }
                strong { class: "status-value", "{screen_name}" }
                span { class: "mode-chip", "data-testid": "scene-mode", "{kind}" }
            }
            div { class: "status-spacer" }
            div { class: "status-item",
                span { class: "status-label", "缩放" }
                span { class: "status-value numeric", "data-testid": "zoom-value", "{zoom:.0}%" }
            }
            div { class: "status-item",
                span { class: "status-label", "Tag" }
                span { class: "status-value", "仿真 4 Hz" }
            }
            div { class: "status-item",
                span { class: "status-label", "保存" }
                span { class: "status-value", role: "status", aria_live: "polite", "{save}" }
            }
            if let Some(error) = engine_error {
                div { class: "status-item is-error", role: "alert",
                    span { class: "status-label", "引擎异常" }
                    span { class: "status-value", "{error}" }
                }
            }
        }
    }
}
