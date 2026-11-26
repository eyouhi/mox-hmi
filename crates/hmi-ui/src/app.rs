use crate::bridge::{self, EngineStatus, NoticeLevel, WorkspaceMode};
use crate::ui::{
    ai_control::AiControlPanel,
    axis_gizmo::AxisGizmo3D,
    controller_tuning::ControllerTuningHud,
    device_details::DeviceDetails,
    gamepad_menu::GamepadMenuHud,
    guide::OperationGuide,
    inspector::Inspector,
    left_panel::LeftPanel,
    runtime::{DeviceLabels3D, RuntimeAccessibleData, RuntimeTopbar},
    status_bar::StatusBar,
    topbar::Topbar,
};
use dioxus::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;

#[cfg(target_arch = "wasm32")]
const MAIN_CSS: Asset = asset!("/assets/main.css");
#[cfg(target_arch = "wasm32")]
const WEB_FONT_CSS: Asset = asset!("/assets/web-fonts.css");
#[cfg(target_arch = "wasm32")]
const FAVICON: Asset = asset!("/assets/favicon.svg");
#[cfg(not(target_arch = "wasm32"))]
const NATIVE_CSS: &str = include_str!("../assets/main.css");
const SCREEN_TRANSITION_MS: u32 = 460;

#[component]
pub fn App() -> Element {
    use_hook(|| {
        bridge::pin_ui_runtime();
        if let Err(error) = bridge::initialize() {
            bridge::set_notice(NoticeLevel::Error, error);
        }
        install_wheel_zoom_bridge();
        crate::control::start();
        crate::webmcp::start();
    });

    #[cfg(target_arch = "wasm32")]
    let mut webgpu = use_signal(|| None::<bool>);
    #[cfg(not(target_arch = "wasm32"))]
    let webgpu = use_signal(|| None::<bool>);
    let engine_starting = bridge::ENGINE_STATUS() == EngineStatus::Starting;
    let mode = bridge::WORKSPACE_MODE();
    let ai_connected = matches!(
        crate::control::AI_CONNECTION(),
        crate::control::AiConnection::Connected(_)
    );
    let runtime_only = bridge::runtime_only();
    let _ = bridge::NOTICE_REVISION();
    let notice = bridge::notice();
    let field_error = bridge::FIELD_ERROR_ANNOUNCEMENT();
    let autosave_token = use_hook(|| Rc::new(Cell::new(0u64)));
    let _ = bridge::ACTIVE_SCREEN();
    let _ = bridge::DOCUMENT_REVISION();
    let active_screen = bridge::active_screen_snapshot();
    let _ = bridge::GAMEPAD_STATUS();
    let gamepad_connected = bridge::gamepad_status().connected;
    let is_three_d = active_screen
        .as_ref()
        .is_some_and(|screen| !screen.is_two_d());
    let loading_screen_name = active_screen
        .as_ref()
        .map(|screen| screen.name().to_string())
        .unwrap_or_default();
    let loading_title = if engine_starting {
        if cfg!(target_arch = "wasm32") {
            "正在初始化 WebGPU 场景"
        } else {
            "正在初始化 Bevy 渲染场景"
        }
    } else {
        "正在切换画面"
    };
    let loading_detail = if engine_starting {
        "正在加载渲染器与设备模型"
    } else {
        loading_screen_name.as_str()
    };
    let mut screen_loading = use_signal(|| false);
    let mut screen_loading_token = use_signal(|| 0u64);
    let previous_screen = use_hook(|| Rc::new(RefCell::new(bridge::active_screen())));
    let edit_hint = if is_three_d {
        if gamepad_connected {
            "左摇杆地面移动 · 右摇杆视角 · 方向键 X/高度 · Options 菜单"
        } else {
            "拖设备移动 · 空白拖旋转 · Option+拖框选 · 方向键微调 · Q/E 升降"
        }
    } else if gamepad_connected {
        "左摇杆移动 · 右摇杆平移 · Option+拖框选 · Options 菜单"
    } else {
        "空白拖平移 · Option+拖框选 · ⇧/⌘ 点击多选 · ⌘C/V/X · F 聚焦"
    };

    #[cfg(target_arch = "wasm32")]
    use_future(move || async move {
        let available = detect_webgpu().await;
        webgpu.set(Some(available));
        *bridge::WEBGPU_READY.write() = Some(available);
        if available {
            web_sys::console::log_1(&JsValue::from_str("MOX HMI renderer: BrowserWebGpu"));
            if let Err(error) = crate::start_engine() {
                bridge::mark_engine_failed(error);
                bridge::set_notice(NoticeLevel::Error, error);
            }
            spawn(async move {
                crate::timer::sleep_ms(30_000).await;
                if bridge::ENGINE_STATUS() == EngineStatus::Starting {
                    bridge::mark_engine_failed("WebGPU 初始化超时（30 秒）");
                }
            });
        } else {
            let message = "当前浏览器未启用 WebGPU，请使用 Chrome/Edge 120+";
            bridge::mark_engine_failed(message);
            bridge::set_notice(NoticeLevel::Error, message);
        }
    });

    use_effect(move || {
        let current = bridge::ACTIVE_SCREEN();
        let changed = {
            let mut previous = previous_screen.borrow_mut();
            let changed = previous.is_some() && *previous != current;
            *previous = current;
            changed
        };
        if changed {
            let token = screen_loading_token().wrapping_add(1);
            screen_loading_token.set(token);
            screen_loading.set(true);
            spawn(async move {
                crate::timer::sleep_ms(SCREEN_TRANSITION_MS as u64).await;
                if screen_loading_token() == token {
                    screen_loading.set(false);
                }
            });
        }
    });

    use_effect(move || {
        let revision = bridge::DOCUMENT_REVISION();
        let token = autosave_token.get().wrapping_add(1);
        autosave_token.set(token);
        let token_cell = autosave_token.clone();
        spawn(async move {
            crate::timer::sleep_ms(500).await;
            if token_cell.get() == token && bridge::document_revision() == revision {
                let _ = bridge::persist_now();
            }
        });
    });

    let run_mode = mode == WorkspaceMode::Run;
    let shell_class = if run_mode {
        "mox-shell is-run"
    } else {
        "mox-shell"
    };

    rsx! {
        {document_styles()}
        a { class: "skip-link", href: "#main-content", "跳到主画布" }
        div {
            class: if ai_connected { "ai-connection-frame is-visible" } else { "ai-connection-frame" },
            "data-testid": "ai-connection-frame",
            aria_hidden: "true",
        }
        div {
            class: "sr-only",
            "data-testid": "field-error-live",
            role: "status",
            aria_live: "polite",
            aria_atomic: "true",
            if let Some(message) = field_error { "{message}" }
        }
        div {
            class: "{shell_class}",
            "data-testid": "app-shell",
            tabindex: "0",
            onkeydown: move |event| {
                let modifiers = event.modifiers();
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = (&event, &modifiers);
                }
                #[cfg(target_arch = "wasm32")]
                {
                    if bridge::text_entry_focused() {
                        return;
                    }
                    let key = match event.key() {
                        Key::Character(value) => {
                            crate::shortcuts::EditorKey::Character(value.to_string())
                        }
                        Key::Named(NamedKey::F1) => crate::shortcuts::EditorKey::F1,
                        Key::Named(NamedKey::Escape) => crate::shortcuts::EditorKey::Escape,
                        Key::Named(NamedKey::Delete) => crate::shortcuts::EditorKey::Delete,
                        Key::Named(NamedKey::Backspace) => crate::shortcuts::EditorKey::Backspace,
                        _ => crate::shortcuts::EditorKey::Other,
                    };
                    if crate::shortcuts::handle_editor_shortcut(
                        key,
                        modifiers.ctrl() || modifiers.meta(),
                        modifiers.shift(),
                    ) {
                        event.prevent_default();
                    }
                }
            },

            div { class: "topbar-slot",
                if run_mode {
                    RuntimeTopbar {}
                } else {
                    Topbar {}
                }
            }

            div { class: if run_mode { "left-slot is-empty" } else { "left-slot" },
                if !run_mode {
                    LeftPanel {}
                }
            }

            main {
                id: "main-content",
                class: "viewport-shell",
                tabindex: "-1",
                aria_label: "主画布",
                div { class: "canvas-stage", "data-testid": "canvas-stage",
                    {canvas_element(screen_loading() || engine_starting)}
                    p { id: "canvas-help", class: "sr-only",
                        if cfg!(target_arch = "wasm32") {
                            "画布内容由 WebGPU 渲染。点击或 Shift 点击多选；2D 空白处拖动平移、Option 加拖动框选，3D 空白处拖动旋转视角、Option 加拖动框选；⌘A 全选，⌘C/⌘V/⌘X 复制、粘贴、剪切，⌘D 原地复制，Delete 删除，⌘Z 撤销。2D 方向键轻推，Q/E 旋转，F 聚焦；3D 方向键轻推，Q/E 调整高度，Option 加方向键旋转视角，WASD 平移观察中心，Shift 加速。DualSense 左摇杆移动，右摇杆观察，Options 打开菜单，Create 打开指引。"
                        } else {
                            "画布内容由 Bevy wgpu 渲染。点击或 Shift 点击多选；2D 空白处拖动平移、Option 加拖动框选，3D 空白处拖动旋转视角、Option 加拖动框选；⌘A 全选，⌘C/⌘V/⌘X 复制、粘贴、剪切，⌘D 原地复制，Delete 删除，⌘Z 撤销。2D 方向键轻推，Q/E 旋转，F 聚焦；3D 方向键轻推，Q/E 调整高度，Option 加方向键旋转视角，WASD 平移观察中心，Shift 加速。DualSense 左摇杆移动，右摇杆观察，Options 打开菜单，Create 打开指引。"
                        }
                    }
                    if screen_loading() || engine_starting {
                        div {
                            class: "screen-loading",
                            "data-testid": "screen-loading",
                            "data-kind": if engine_starting { "startup" } else { "screen" },
                            role: "status",
                            aria_live: "polite",
                            aria_atomic: "true",
                            div { class: "screen-loading-card",
                                span { class: "screen-spinner", aria_hidden: "true" }
                                div {
                                    strong { "{loading_title}" }
                                    if !loading_detail.is_empty() {
                                        span { "{loading_detail}" }
                                    }
                                }
                            }
                        }
                    }
                    if !run_mode {
                        if is_three_d {
                            AxisGizmo3D {}
                        }
                        div { class: "canvas-hint-bar", "data-testid": "canvas-hints",
                            span { class: "canvas-hint-copy", "{edit_hint}" }
                            button {
                                class: "canvas-guide-button",
                                "data-testid": "canvas-guide-button",
                                title: "打开操作指引 F1",
                                onclick: move |_| bridge::set_guide_open(true),
                                "操作指引 · F1"
                            }
                        }
                    }
                    if run_mode {
                        RuntimeAccessibleData {}
                    }
                    DeviceLabels3D {}
                    if run_mode {
                        DeviceDetails {}
                    }
                    if webgpu() == Some(false) {
                        div {
                            class: "blocking-overlay",
                            "data-testid": "webgpu-blocked",
                            role: "alert",
                            aria_live: "assertive",
                            aria_atomic: "true",
                            div { class: "blocking-card",
                                div { class: "blocking-mark", aria_hidden: "true", "!" }
                                h2 { "WebGPU 不可用" }
                                p { "MOX HMI 仅使用 WebGPU 渲染，不提供 WebGL2 回退。请使用 Chrome/Edge 120+，并确认浏览器未禁用硬件加速。" }
                            }
                        }
                    }
                }
            }

            div { class: if run_mode { "right-slot is-empty" } else { "right-slot" },
                if !run_mode {
                    Inspector {}
                }
            }

            div { class: if run_mode { "footer-slot is-empty" } else { "footer-slot" },
                if !run_mode {
                    StatusBar {}
                }
            }

            if let Some(notice) = notice {
                div {
                    class: "notice",
                    "data-testid": "notice",
                    "data-level": notice.level.as_str(),
                    role: if notice.level == NoticeLevel::Error { "alert" } else { "status" },
                    aria_live: if notice.level == NoticeLevel::Error { "assertive" } else { "polite" },
                    aria_atomic: "true",
                    div { class: "notice-mark", aria_hidden: "true" }
                    span { class: "notice-copy", "{notice.message}" }
                    button {
                        class: "notice-close",
                        aria_label: "关闭提示",
                        onclick: move |_| {
                            bridge::clear_notice();
                            #[cfg(target_arch = "wasm32")]
                            let _ = dioxus::document::eval(
                                r#"document.querySelector('[data-testid="app-shell"]')?.focus()"#,
                            );
                        },
                        "×"
                    }
                }
            }

            OperationGuide {}

            if !runtime_only {
                AiControlPanel {}
            }

            ControllerTuningHud {}
            GamepadMenuHud {}
        }
    }
}

fn document_styles() -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        rsx! {
            document::Stylesheet { href: MAIN_CSS }
            document::Stylesheet { href: WEB_FONT_CSS }
            document::Link { rel: "icon", r#type: "image/svg+xml", href: FAVICON }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        rsx! {
            document::Style { "{NATIVE_CSS}" }
        }
    }
}

fn install_wheel_zoom_bridge() {
    #[cfg(not(target_arch = "wasm32"))]
    return;
    #[cfg(target_arch = "wasm32")]
    let _ = dioxus::document::eval(
        r##"
        (() => {
          if (window.__moxHmiWheelZoomBridge) return;
          window.__moxHmiWheelZoomBridge = true;
          let timer = 0;
          let mouseTimer = 0;
          const markWheelZoomActive = () => {
            window.__moxHmiWheelZoomActive = true;
            window.clearTimeout(timer);
            timer = window.setTimeout(() => {
              window.__moxHmiWheelZoomActive = false;
            }, 900);
          };
          const markMouseWheelActive = () => {
            window.__moxHmiWheelMouseActive = true;
            window.clearTimeout(mouseTimer);
            mouseTimer = window.setTimeout(() => {
              window.__moxHmiWheelMouseActive = false;
            }, 350);
          };
          const classifyWheel = (event) => {
            if (event.ctrlKey || event.metaKey) {
              markWheelZoomActive();
              return;
            }
            if (event.deltaMode === WheelEvent.DOM_DELTA_LINE) {
              markMouseWheelActive();
              return;
            }
            const legacy = Number(event.wheelDeltaY);
            const looksLikeMouseWheel = Number.isFinite(legacy)
              ? Math.abs(legacy) >= 120 && Math.abs(legacy) % 120 === 0
              : Math.abs(event.deltaY) >= 100 && event.deltaX === 0 && Number.isInteger(event.deltaY);
            if (looksLikeMouseWheel) markMouseWheelActive();
          };

          const attachGestureBridge = () => {
            const canvas = document.querySelector("#bevy-canvas");
            if (!canvas) {
              window.requestAnimationFrame(attachGestureBridge);
              return;
            }
            if (canvas.__moxHmiGestureBridge) return;
            canvas.__moxHmiGestureBridge = true;
            canvas.addEventListener("wheel", (event) => {
              classifyWheel(event);
              if (event.cancelable) event.preventDefault();
            }, { passive: false });
            canvas.addEventListener("contextmenu", (event) => event.preventDefault(), { passive: false });
            let gestureScale = 1;
            canvas.addEventListener("gesturestart", (event) => {
              gestureScale = Number(event.scale) || 1;
              event.preventDefault();
            }, { passive: false });
            canvas.addEventListener("gesturechange", (event) => {
              event.preventDefault();
              const next = Number(event.scale) || gestureScale;
              const ratio = gestureScale > 0 ? next / gestureScale : 1;
              gestureScale = next;
              if (!Number.isFinite(ratio) || Math.abs(ratio - 1) < 0.001) return;
              markWheelZoomActive();
              canvas.dispatchEvent(new WheelEvent("wheel", {
                deltaY: Math.log(ratio) * 120,
                deltaMode: WheelEvent.DOM_DELTA_PIXEL,
                ctrlKey: true,
                bubbles: true,
                cancelable: true,
              }));
            }, { passive: false });
            canvas.addEventListener("gestureend", (event) => {
              gestureScale = 1;
              event.preventDefault();
            }, { passive: false });
          };
          window.requestAnimationFrame(attachGestureBridge);
        })();
        "##,
    );
}

#[cfg(target_arch = "wasm32")]
fn canvas_element(busy: bool) -> Element {
    rsx! {
        canvas {
            id: "bevy-canvas",
            class: "bevy-canvas",
            tabindex: "0",
            role: "img",
            aria_label: "HMI 组态画布",
            aria_describedby: "canvas-help",
            "aria-busy": if busy { "true" } else { "false" },
            "aria-keyshortcuts": "Meta+S Control+S Meta+Z Control+Z Meta+Shift+Z Control+Shift+Z Control+Y Meta+C Control+C Meta+V Control+V Meta+X Control+X Meta+A Control+A Meta+D Control+D Delete Backspace ArrowLeft ArrowRight ArrowUp ArrowDown + - Home F1 F W A S D Q E",
            onkeydown: move |event| {
                if matches!(
                    event.key(),
                    Key::Named(NamedKey::ArrowLeft)
                        | Key::Named(NamedKey::ArrowRight)
                        | Key::Named(NamedKey::ArrowUp)
                        | Key::Named(NamedKey::ArrowDown)
                        | Key::Named(NamedKey::Home)
                ) || matches!(
                    event.key(),
                    Key::Character(value) if matches!(value.as_str(), "+" | "-" | "=")
                ) {
                    event.prevent_default();
                }
            },
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn canvas_element(_busy: bool) -> Element {
    crate::native_viewport().unwrap_or_else(|| {
        rsx! {
            div { class: "blocking-overlay",
                div { class: "blocking-card",
                    h2 { "原生视图未注册" }
                    p { "编辑器启动时没有安装 Bevy 画布提供者。" }
                }
            }
        }
    })
}

#[cfg(target_arch = "wasm32")]
async fn detect_webgpu() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let navigator: JsValue = window.navigator().into();
    let Ok(gpu) = js_sys::Reflect::get(&navigator, &JsValue::from_str("gpu")) else {
        return false;
    };
    if gpu.is_undefined() || gpu.is_null() {
        return false;
    }
    let Ok(request_adapter) = js_sys::Reflect::get(&gpu, &JsValue::from_str("requestAdapter"))
    else {
        return false;
    };
    let Ok(request_adapter) = request_adapter.dyn_into::<js_sys::Function>() else {
        return false;
    };
    let Ok(promise) = request_adapter.call0(&gpu) else {
        return false;
    };
    let Ok(promise) = promise.dyn_into::<js_sys::Promise>() else {
        return false;
    };
    match JsFuture::from(promise).await {
        Ok(adapter) => !adapter.is_null() && !adapter.is_undefined(),
        Err(_) => false,
    }
}
