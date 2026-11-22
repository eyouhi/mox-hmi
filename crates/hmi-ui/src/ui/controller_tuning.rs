use crate::bridge;
use dioxus::prelude::*;

#[component]
pub fn ControllerTuningHud() -> Element {
    let Some(hud) = bridge::controller_tuning_hud() else {
        return rsx! {};
    };
    let decimals = if hud.step >= 1.0 {
        0
    } else if hud.step >= 0.1 {
        1
    } else {
        2
    };
    let value = format!("{value:.decimals$}", value = hud.value);

    rsx! {
        aside {
            class: "controller-tuning",
            "data-testid": "controller-tuning",
            role: "status",
            aria_live: "polite",
            aria_atomic: "true",
            div { class: "controller-tuning-head",
                span { class: "controller-tuning-mark", aria_hidden: "true", "PS" }
                div {
                    span { class: "eyebrow", "手柄参数模式" }
                    strong { "{hud.label}" }
                }
                span { class: "controller-tuning-index", "{hud.index + 1} / {hud.count}" }
            }
            div { class: "controller-tuning-value", "data-testid": "controller-tuning-value", "{value}" }
            p { class: "controller-tuning-help",
                "方向键 ↑↓ 切换参数 · ←→ 调整 · L1/R1 粗调 ×10 · ○ 退出"
            }
        }
    }
}
