use crate::bridge;
use dioxus::prelude::*;

#[component]
pub fn GamepadMenuHud() -> Element {
    let Some(hud) = bridge::gamepad_menu_hud() else {
        return rsx! {};
    };

    rsx! {
        aside {
            class: "gamepad-menu",
            "data-testid": "gamepad-menu",
            role: "status",
            aria_live: "polite",
            aria_atomic: "true",
            div { class: "gamepad-menu-head",
                span { class: "gamepad-menu-mark", aria_hidden: "true", "PS" }
                div {
                    span { class: "eyebrow", "手柄快捷菜单" }
                    strong { "{hud.title}" }
                }
                span { class: "gamepad-menu-index", "{hud.index + 1} / {hud.items.len()}" }
            }
            div { class: "gamepad-menu-list", role: "list",
                for (index, item) in hud.items.iter().enumerate() {
                    div {
                        class: if index == hud.index { "gamepad-menu-item is-active" } else { "gamepad-menu-item" },
                        role: "listitem",
                        aria_current: if index == hud.index { "true" } else { "false" },
                        span { class: "gamepad-menu-item-label", "{item.label}" }
                        span { class: "gamepad-menu-item-detail", "{item.detail}" }
                    }
                }
            }
            p { class: "gamepad-menu-help", "{hud.hint}" }
        }
    }
}
