use super::common::report_error;
#[cfg(not(target_arch = "wasm32"))]
use super::file_io::pick_json_text;
use super::file_io::{download_json, read_uploaded_text};
use crate::bridge::{self, EngineStatus, NoticeLevel, SaveStatus, WorkspaceMode};
use dioxus::prelude::*;

#[component]
pub fn Topbar() -> Element {
    let _ = bridge::DOCUMENT_REVISION();
    let webgpu = bridge::WEBGPU_READY();
    let engine = bridge::ENGINE_STATUS();
    let save_status = bridge::SAVE_STATUS();
    let project = bridge::project();
    let can_undo = bridge::can_undo();
    let can_redo = bridge::can_redo();
    let gamepad = bridge::gamepad_status();

    let engine_text = match (webgpu, &engine) {
        (Some(false), _) if cfg!(target_arch = "wasm32") => "WebGPU 不可用",
        (_, EngineStatus::Ready) if cfg!(target_arch = "wasm32") => "WebGPU ready",
        (_, EngineStatus::Failed(_)) if cfg!(target_arch = "wasm32") => "WebGPU 失败",
        (_, EngineStatus::Ready) => "Bevy 就绪",
        (_, EngineStatus::Failed(_)) => "Bevy 失败",
        _ => "Bevy 启动中",
    };
    let webgpu_class = match &engine {
        EngineStatus::Ready => "status-pill is-ok",
        EngineStatus::Failed(_) => "status-pill is-error",
        EngineStatus::Starting => "status-pill is-waiting",
    };
    let save_text = match save_status {
        SaveStatus::Saved => "已保存",
        SaveStatus::Pending => "保存中",
        SaveStatus::Failed => "保存失败",
    };
    let save_class = match save_status {
        SaveStatus::Saved => "save-pill is-ok",
        SaveStatus::Pending => "save-pill is-waiting",
        SaveStatus::Failed => "save-pill is-error",
    };
    let gamepad_text = if gamepad.connected {
        if gamepad.is_dualsense() {
            "DualSense"
        } else {
            "手柄在线"
        }
    } else {
        "手柄未连接"
    };

    rsx! {
        header { class: "topbar", "data-testid": "topbar",
            div { class: "brand-block",
                div { class: "brand-mark", aria_hidden: "true", "M" }
                div { class: "brand-copy",
                    h1 { class: "brand-title", "MOX HMI" }
                    div { class: "brand-subtitle", "车站 BAS 工业组态" }
                }
            }

            div { class: "project-block",
                div { class: "eyebrow", "当前工程" }
                div { class: "project-name", "data-testid": "project-name", "{project.name}" }
            }

            div { class: "topbar-spacer" }

            div { class: "engine-cluster",
                span {
                    class: webgpu_class,
                    "data-testid": "webgpu-status",
                    role: "status",
                    aria_live: "polite",
                    aria_atomic: "true",
                    "{engine_text}"
                }
                span {
                    class: save_class,
                    "data-testid": "save-status",
                    role: "status",
                    aria_live: "polite",
                    aria_atomic: "true",
                    "{save_text}"
                }
                span {
                    class: if gamepad.connected { "controller-pill is-connected" } else { "controller-pill" },
                    "data-testid": "gamepad-status",
                    role: "status",
                    aria_live: "polite",
                    title: if gamepad.connected { gamepad.id.clone() } else { "连接 DualSense 后按任意键唤醒".to_string() },
                    "{gamepad_text}"
                }
            }

            div { class: "toolbar-group",
                button {
                    class: "toolbar-button toolbar-button-quiet",
                    "data-testid": "guide-button",
                    title: "操作指引 F1",
                    aria_keyshortcuts: "F1",
                    onclick: move |_| bridge::set_guide_open(true),
                    "操作指引"
                }
                button {
                    class: "toolbar-button",
                    "data-testid": "undo-button",
                    disabled: !can_undo,
                    title: "撤销 Ctrl+Z",
                    onclick: move |_| {
                        bridge::undo();
                    },
                    "撤销"
                }
                button {
                    class: "toolbar-button",
                    "data-testid": "redo-button",
                    disabled: !can_redo,
                    title: "重做 Ctrl+Shift+Z",
                    onclick: move |_| {
                        bridge::redo();
                    },
                    "重做"
                }
                button {
                    class: "toolbar-button",
                    "data-testid": "import-button",
                    onclick: move |_| request_import(),
                    "导入"
                }
                button {
                    class: "toolbar-button",
                    "data-testid": "export-button",
                    onclick: move |_| {
                        let result = bridge::export_json().and_then(|json| {
                            download_json(&bridge::export_download_name(), &json)
                        });
                        match result {
                            Ok(()) => bridge::set_notice(NoticeLevel::Info, "工程已导出"),
                            Err(error) => report_error(error),
                        }
                    },
                    "导出"
                }
            }

            input {
                r#type: "file",
                accept: ".json,.moxhmi.json,application/json",
                "data-testid": "import-input",
                hidden: true,
                aria_label: "导入工程 JSON 文件",
                onchange: move |event| {
                    if let Some(file) = event.files().into_iter().next() {
                        spawn(async move {
                            match read_uploaded_text(file).await {
                                Ok(text) => {
                                    if let Err(error) = bridge::import_json(&text) {
                                        bridge::set_notice(NoticeLevel::Error, error);
                                    }
                                }
                                Err(error) => bridge::set_notice(NoticeLevel::Error, error),
                            }
                            #[cfg(target_arch = "wasm32")]
                            let _ = dioxus::document::eval(
                                "const el=document.querySelector('[data-testid=import-input]'); if(el){el.value=''}",
                            );
                        });
                    }
                },
            }

            button {
                class: "toolbar-button toolbar-button-primary",
                "data-testid": "run-button",
                onclick: move |_| {
                    bridge::select_node(None);
                    bridge::set_workspace_mode(WorkspaceMode::Run);
                },
                "运行"
            }
        }
    }
}

fn request_import() {
    #[cfg(target_arch = "wasm32")]
    {
        let _ =
            dioxus::document::eval("document.querySelector('[data-testid=import-input]')?.click()");
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        match pick_json_text() {
            Ok(Some(text)) => {
                if let Err(error) = bridge::import_json(&text) {
                    bridge::set_notice(NoticeLevel::Error, error);
                }
            }
            Ok(None) => {}
            Err(error) => bridge::set_notice(NoticeLevel::Error, error),
        }
    }
}
