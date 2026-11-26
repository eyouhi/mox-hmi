use super::common::sanitize_test_id;
use crate::bridge::{self, EngineStatus, WorkspaceMode};
use dioxus::prelude::*;
use hmi_core::{ButtonAction, TagValue, format_tag_value};

#[component]
pub fn DeviceLabels3D() -> Element {
    let labels = bridge::VIEW_LABELS_3D();
    rsx! {
        div { class: "device-label-layer",
            for label in labels {
                div {
                    key: "{label.node_id}",
                    class: if label.visible { "device-label" } else { "device-label is-hidden" },
                    "data-testid": "device-label-{sanitize_test_id(&label.node_id)}",
                    "data-point-x": "{label.point_left_pct:.4}",
                    "data-point-y": "{label.point_top_pct:.4}",
                    aria_hidden: if label.visible { "false" } else { "true" },
                    style: "--label-x: {label.left_pct:.2}%; --label-y: {label.top_pct:.2}%;",
                    strong { "{label.title}" }
                    if !label.value.is_empty() {
                        span { "{label.value}" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn RuntimeTopbar() -> Element {
    let _ = bridge::TAG_REVISION();
    let _ = bridge::ACTIVE_SCREEN();
    let project = bridge::project();
    let snapshot = bridge::tag_snapshot();
    let mut clock = use_signal(current_clock);
    use_future(move || async move {
        loop {
            crate::timer::sleep_ms(1_000).await;
            clock.set(current_clock());
        }
    });

    let active_id = bridge::active_screen().unwrap_or_else(|| project.startup_screen.clone());
    let screen_2d = project.screens.iter().find(|screen| screen.is_two_d());
    let screen_3d = project.screens.iter().find(|screen| !screen.is_two_d());
    let fan_running = snapshot
        .get("bas.supply_fan.run")
        .and_then(TagValue::as_bool)
        .unwrap_or(false);
    let supply_temp = format_tag_value(
        snapshot
            .get("bas.ahu.supply_temp")
            .unwrap_or(&TagValue::Number(0.0)),
        1,
        "°C",
    );
    let fan_freq = format_tag_value(
        snapshot
            .get("bas.supply_fan.freq")
            .unwrap_or(&TagValue::Number(0.0)),
        1,
        "Hz",
    );
    let engine = bridge::ENGINE_STATUS();
    let runtime_only = bridge::runtime_only();
    let remote_runner = bridge::remote_runner_active();
    let remote_connected = bridge::remote_runner_connected();

    rsx! {
        header { class: "runtime-topbar", "data-testid": "run-shell",
            div { class: "runtime-brand",
                div { class: "brand-mark", aria_hidden: "true", "M" }
                div {
                    h1 { class: "brand-title", "MOX HMI Runtime" }
                    div { class: "brand-subtitle", "{project.name}" }
                }
            }
            div { class: "runtime-clock", aria_label: "当前时间", "{clock()}" }
            nav { class: "runtime-nav", aria_label: "运行画面切换",
                if let Some(screen) = screen_2d {
                    button {
                        class: if screen.id() == active_id { "runtime-tab is-active" } else { "runtime-tab" },
                        aria_current: if screen.id() == active_id { "page" } else { "false" },
                        "data-testid": "run-screen-2d",
                        onclick: {
                            let id = screen.id().to_string();
                            move |_| {
                                bridge::select_screen(&id);
                                bridge::select_node(None);
                            }
                        },
                        "2D 工艺图"
                    }
                }
                if let Some(screen) = screen_3d {
                    button {
                        class: if screen.id() == active_id { "runtime-tab is-active" } else { "runtime-tab" },
                        aria_current: if screen.id() == active_id { "page" } else { "false" },
                        "data-testid": "run-screen-3d",
                        onclick: {
                            let id = screen.id().to_string();
                            move |_| {
                                bridge::select_screen(&id);
                                bridge::select_node(None);
                            }
                        },
                        "3D 机房"
                    }
                }
            }
            div { class: "runtime-spacer" }
            span {
                class: if remote_runner && !remote_connected {
                    "status-pill is-waiting"
                } else {
                    "status-pill is-ok"
                },
                if remote_runner {
                    if remote_connected {
                        "服务端实时 4 Hz"
                    } else {
                        "服务端重连中"
                    }
                } else {
                    "本地仿真 4 Hz"
                }
            }
            span {
                class: "runtime-value",
                "data-testid": "supply-fan-state",
                if fan_running { "送风机：运行" } else { "送风机：停止" }
            }
            span {
                class: "runtime-value",
                "data-testid": "runtime-tag-bas-ahu-supply-temp",
                "{supply_temp}"
            }
            span {
                class: "runtime-value",
                "data-testid": "runtime-tag-bas-supply-fan-freq",
                "{fan_freq}"
            }
            button {
                class: "toolbar-button",
                "data-testid": "button-supply-fan",
                onclick: move |_| {
                    let action = ButtonAction::ToggleTag {
                        tag_id: "bas.supply_fan.run".to_string(),
                    };
                    match bridge::invoke_button_action(&action) {
                        Ok(()) => {
                            bridge::select_node(Some("node.button.fan".to_string()));
                            #[cfg(target_arch = "wasm32")]
                            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(
                                "MOX HMI button: ToggleTag { tag_id: \"bas.supply_fan.run\" }",
                            ));
                        }
                        Err(error) => bridge::set_notice(bridge::NoticeLevel::Error, error.to_string()),
                    }
                },
                "送风机启停"
            }
            if !runtime_only {
                button {
                    class: "toolbar-button toolbar-button-danger",
                    "data-testid": "run-exit",
                    onclick: move |_| bridge::set_workspace_mode(WorkspaceMode::Edit),
                    "退出运行"
                }
            }
            if let EngineStatus::Failed(error) = engine {
                div { class: "engine-strip", role: "alert", "渲染引擎异常：{error}" }
            }
        }
    }
}

#[component]
pub fn RuntimeAccessibleData() -> Element {
    let _ = bridge::TAG_REVISION();
    let project = bridge::project();
    let snapshot = bridge::tag_snapshot();

    rsx! {
        div { class: "sr-only",
            h2 { "运行数据" }
            ul {
                for tag in project.tags.iter() {
                    {
                        let value = snapshot
                            .get(&tag.id)
                            .map(|value| format_tag_value(value, tag.precision, &tag.unit))
                            .unwrap_or_else(|| "—".to_string());
                        rsx! {
                            li { key: "{tag.id}",
                                span { "{tag.name}" }
                                span { "（{tag.id}）：{value}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn current_clock() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let text = js_sys::Date::new_0().to_locale_time_string("zh-CN");
        text.as_string().unwrap_or_else(|| "--:--:--".to_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let seconds_of_day = seconds % 86_400;
        format!(
            "{:02}:{:02}:{:02}",
            seconds_of_day / 3_600,
            seconds_of_day % 3_600 / 60,
            seconds_of_day % 60
        )
    }
}
