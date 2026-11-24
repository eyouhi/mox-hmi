use crate::bridge::{self, WorkspaceMode};
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use hmi_control_protocol::{
    BrowserMessage, DEFAULT_CONTROL_PATH, DEFAULT_CONTROL_PORT, PROTOCOL_VERSION, ServerMessage,
};
use hmi_control_protocol::{ControlState, ScreenSummary};
use hmi_core::{EditorCommand, TagValue};
use serde_json::{Value, json};
#[cfg(target_arch = "wasm32")]
use std::cell::{Cell, RefCell};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure};
#[cfg(target_arch = "wasm32")]
use web_sys::{CloseEvent, Event, MessageEvent, WebSocket};

#[cfg(target_arch = "wasm32")]
const RECONNECT_DELAY_MS: i32 = 3_000;
const MAX_ACTIVITIES: usize = 20;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU64, Ordering},
};
#[cfg(not(target_arch = "wasm32"))]
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiConnection {
    Disabled,
    Connecting(String),
    Connected(String),
    Disconnected(String, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiStatus {
    Running,
    Ok,
    Error,
}

impl AiStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiActivity {
    pub id: u64,
    pub time: String,
    pub label: String,
    pub detail: String,
    pub status: AiStatus,
}

pub static AI_CONNECTION: GlobalSignal<AiConnection> = Signal::global(|| AiConnection::Disabled);
pub static AI_ACTIVITY: GlobalSignal<Vec<AiActivity>> = Signal::global(Vec::new);
pub static WEBMCP_AVAILABLE: GlobalSignal<bool> = Signal::global(|| false);
pub static MCP_SETTINGS_AVAILABLE: GlobalSignal<bool> = Signal::global(|| false);
pub static MCP_ENABLED: GlobalSignal<bool> = Signal::global(|| false);
pub static MCP_ENDPOINT: GlobalSignal<String> = Signal::global(String::new);

pub fn mark_webmcp_available() {
    let mut available = WEBMCP_AVAILABLE.write();
    if *available {
        return;
    }
    *available = true;
}

#[cfg(not(target_arch = "wasm32"))]
static NEXT_NATIVE_ACTIVITY: AtomicU64 = AtomicU64::new(1);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct NativeMcpBridge {
    enabled: Arc<dyn Fn() -> bool + Send + Sync>,
    set_enabled: Arc<dyn Fn(bool) -> Result<(), String> + Send + Sync>,
    endpoint: Arc<dyn Fn() -> String + Send + Sync>,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeMcpBridge {
    pub fn new(
        enabled: Arc<dyn Fn() -> bool + Send + Sync>,
        set_enabled: Arc<dyn Fn(bool) -> Result<(), String> + Send + Sync>,
        endpoint: Arc<dyn Fn() -> String + Send + Sync>,
    ) -> Self {
        Self {
            enabled,
            set_enabled,
            endpoint,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
static NATIVE_MCP_BRIDGE: OnceLock<NativeMcpBridge> = OnceLock::new();

#[cfg(not(target_arch = "wasm32"))]
pub fn install_native_mcp_bridge(bridge: NativeMcpBridge) -> Result<(), String> {
    NATIVE_MCP_BRIDGE
        .set(bridge)
        .map_err(|_| "原生 MCP 设置桥接已安装".to_string())
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
struct ControlConfig {
    url: String,
    token: Option<String>,
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static CONFIG: RefCell<Option<ControlConfig>> = const { RefCell::new(None) };
    static SOCKET: RefCell<Option<WebSocket>> = const { RefCell::new(None) };
    static CONNECTION_GENERATION: Cell<u64> = const { Cell::new(0) };
    static NEXT_ACTIVITY: Cell<u64> = const { Cell::new(1) };
    static RECONNECT_PENDING: Cell<bool> = const { Cell::new(false) };
}

pub fn start() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(bridge) = NATIVE_MCP_BRIDGE.get() {
            *MCP_ENABLED.write() = (bridge.enabled)();
            *MCP_ENDPOINT.write() = (bridge.endpoint)();
            *MCP_SETTINGS_AVAILABLE.write() = true;
        }
        *AI_CONNECTION.write() = AiConnection::Disabled;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let Some(config) = read_config() else {
            *AI_CONNECTION.write() = AiConnection::Disabled;
            return;
        };
        CONFIG.with(|slot| *slot.borrow_mut() = Some(config.clone()));
        *AI_CONNECTION.write() = AiConnection::Connecting(config.url);
        connect();
    }
}

pub fn reconnect() {
    #[cfg(not(target_arch = "wasm32"))]
    return;
    #[cfg(target_arch = "wasm32")]
    {
        let Some(config) = CONFIG.with(|slot| slot.borrow().clone()) else {
            start();
            return;
        };
        if let Some(socket) = SOCKET.with(|slot| slot.borrow_mut().take()) {
            let _ = socket.close();
        }
        RECONNECT_PENDING.with(|pending| pending.set(false));
        *AI_CONNECTION.write() = AiConnection::Connecting(config.url);
        connect();
    }
}

pub fn clear_activities() {
    AI_ACTIVITY.write().clear();
}

pub fn set_mcp_enabled(enabled: bool) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let bridge = NATIVE_MCP_BRIDGE
            .get()
            .ok_or_else(|| "MCP 设置桥接尚未安装".to_string())?;
        (bridge.set_enabled)(enabled)?;
        *MCP_ENABLED.write() = enabled;
        *MCP_ENDPOINT.write() = (bridge.endpoint)();
        *AI_CONNECTION.write() = AiConnection::Disabled;
        Ok(())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = enabled;
        Err("MCP 开关仅在原生编辑器中可用".to_string())
    }
}

pub fn codex_mcp_command() -> String {
    let endpoint = MCP_ENDPOINT.peek().clone();
    codex_mcp_command_for(&endpoint)
}

pub async fn copy_mcp_command(command: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen_futures::JsFuture;

        let window = web_sys::window().ok_or_else(|| "window 不可用".to_string())?;
        let promise = window.navigator().clipboard().write_text(command);
        JsFuture::from(promise)
            .await
            .map(|_| ())
            .map_err(|error| format!("浏览器剪贴板不可用: {error:?}"))
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::NSString;

        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let text = NSString::from_str(command);
        let data_type = unsafe { NSPasteboardTypeString };
        if pasteboard.setString_forType(&text, data_type) {
            Ok(())
        } else {
            Err("无法写入 macOS 剪贴板".to_string())
        }
    }
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "macos")))]
    {
        let _ = command;
        Err("当前平台暂不支持复制 Codex 命令".to_string())
    }
}

fn codex_mcp_command_for(endpoint: &str) -> String {
    if endpoint.is_empty() {
        return String::new();
    }
    let endpoint = endpoint.replace('\'', "'\\''");
    format!("codex mcp add mox-hmi --url '{endpoint}'")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn mark_native_ai_connected() {
    if !MCP_ENABLED() {
        return;
    }
    if matches!(
        &*AI_CONNECTION.peek(),
        AiConnection::Disabled | AiConnection::Disconnected(_, _)
    ) {
        *AI_CONNECTION.write() = AiConnection::Connected("native://mcp".to_string());
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn begin_native_activity(label: &str, detail: &str, status: AiStatus) -> u64 {
    let id = NEXT_NATIVE_ACTIVITY.fetch_add(1, Ordering::Relaxed);
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let time = format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());
    let mut activities = AI_ACTIVITY.write();
    activities.insert(
        0,
        AiActivity {
            id,
            time,
            label: label.to_string(),
            detail: detail.to_string(),
            status,
        },
    );
    activities.truncate(MAX_ACTIVITIES);
    id
}

#[cfg(not(target_arch = "wasm32"))]
pub fn finish_native_activity(id: u64, status: AiStatus, detail: &str) {
    let mut activities = AI_ACTIVITY.write();
    if let Some(activity) = activities.iter_mut().find(|activity| activity.id == id) {
        activity.status = status;
        activity.detail = detail.to_string();
    }
}

#[cfg(target_arch = "wasm32")]
fn connect() {
    let Some(config) = CONFIG.with(|slot| slot.borrow().clone()) else {
        return;
    };
    if SOCKET.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|socket| socket.ready_state() <= WebSocket::OPEN)
    }) {
        return;
    }

    let generation = CONNECTION_GENERATION.with(|generation| {
        let next = generation.get().wrapping_add(1);
        generation.set(next);
        next
    });
    let Ok(socket) = WebSocket::new(&config.url) else {
        *AI_CONNECTION.write() =
            AiConnection::Disconnected("无法创建 WebSocket".to_string(), config.url.clone());
        schedule_reconnect();
        return;
    };

    let onopen = Closure::<dyn FnMut(Event)>::new(move |_| {
        RECONNECT_PENDING.with(|pending| pending.set(false));
        let url = config.url.clone();
        *AI_CONNECTION.write() = AiConnection::Connected(url.clone());
        push_activity("AI 控制隧道已连接", &url, AiStatus::Ok);
        send(BrowserMessage::Hello {
            protocol: PROTOCOL_VERSION,
            token: config.token.clone(),
            app: "MOX HMI Studio".to_string(),
            state: current_state(),
        });
    });
    socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        if let Some(text) = event.data().as_string() {
            handle_server_text(&text);
        }
    });
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    let onerror = Closure::<dyn FnMut(Event)>::new(move |_| {
        let url = CONFIG
            .with(|slot| slot.borrow().as_ref().map(|config| config.url.clone()))
            .unwrap_or_default();
        *AI_CONNECTION.write() = AiConnection::Disconnected("WebSocket 连接错误".to_string(), url);
    });
    socket.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();

    let onclose = Closure::<dyn FnMut(CloseEvent)>::new(move |event: CloseEvent| {
        if CONNECTION_GENERATION.with(Cell::get) != generation {
            return;
        }
        SOCKET.with(|slot| {
            slot.borrow_mut().take();
        });
        let url = CONFIG
            .with(|slot| slot.borrow().as_ref().map(|config| config.url.clone()))
            .unwrap_or_default();
        let reason = if event.reason().is_empty() {
            format!("连接已关闭（{}）", event.code())
        } else {
            event.reason()
        };
        *AI_CONNECTION.write() = AiConnection::Disconnected(reason, url);
        schedule_reconnect();
    });
    socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();

    SOCKET.with(|slot| *slot.borrow_mut() = Some(socket));
}

#[cfg(target_arch = "wasm32")]
fn schedule_reconnect() {
    if CONFIG.with(|slot| slot.borrow().is_none())
        || RECONNECT_PENDING.with(|pending| {
            let already_pending = pending.get();
            pending.set(true);
            already_pending
        })
    {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::once_into_js(move || {
        RECONNECT_PENDING.with(|pending| pending.set(false));
        connect();
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        callback.unchecked_ref(),
        RECONNECT_DELAY_MS,
    );
}

#[cfg(target_arch = "wasm32")]
fn handle_server_text(text: &str) {
    let message = match serde_json::from_str::<ServerMessage>(text) {
        Ok(message) => message,
        Err(error) => {
            push_activity("收到无效控制消息", &error.to_string(), AiStatus::Error);
            return;
        }
    };
    match message {
        ServerMessage::Welcome { session_id, .. } => {
            let url = current_url();
            *AI_CONNECTION.write() = AiConnection::Connected(url);
            push_activity(
                "AI 会话就绪",
                &format!("session {session_id}"),
                AiStatus::Ok,
            );
        }
        ServerMessage::Reject { reason } => {
            push_activity("控制隧道拒绝连接", &reason, AiStatus::Error);
            *AI_CONNECTION.write() = AiConnection::Disconnected(reason, current_url());
        }
        ServerMessage::Request {
            id,
            action,
            label,
            params,
        } => execute_request(id, &action, &label, &params),
    }
}

#[cfg(target_arch = "wasm32")]
fn execute_request(id: u64, action: &str, label: &str, params: &Value) {
    let (result, duration_ms) = track_action(action, label, params);
    let state = current_state();

    match result {
        Ok(value) => {
            send(BrowserMessage::Response {
                id,
                ok: true,
                result: value,
                error: None,
                state,
                duration_ms,
            });
        }
        Err(error) => {
            send(BrowserMessage::Response {
                id,
                ok: false,
                result: Value::Null,
                error: Some(error),
                state,
                duration_ms,
            });
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn track_action(action: &str, label: &str, params: &Value) -> (Result<Value, String>, u64) {
    let started = js_sys::Date::now();
    let detail = action_detail(action, params);
    let activity_id = push_activity(label, &detail, AiStatus::Running);
    let result = run_action(action, params);
    let duration_ms = (js_sys::Date::now() - started).max(0.0) as u64;

    match &result {
        Ok(value) => {
            let detail = value
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or(&detail)
                .to_string();
            finish_activity(activity_id, AiStatus::Ok, &detail);
        }
        Err(error) => {
            finish_activity(activity_id, AiStatus::Error, &error);
            bridge::set_notice(bridge::NoticeLevel::Error, format!("AI 操作失败：{error}"));
        }
    }

    (result, duration_ms)
}

pub fn run_action(action: &str, params: &Value) -> Result<Value, String> {
    match action {
        "inspect_project" => inspect_project(params),
        "select_screen" => {
            let screen_id = required_str(params, "screen_id")?;
            bridge::select_screen(screen_id);
            if bridge::active_screen().as_deref() != Some(screen_id) {
                return Err(format!("画面 `{screen_id}` 不存在"));
            }
            Ok(json!({ "summary": format!("切换到画面 {screen_id}"), "screen_id": screen_id }))
        }
        "select_node" => {
            let node_id = params
                .get("node_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(node_id) = &node_id {
                if !bridge::project_contains_node(&bridge::project(), node_id) {
                    return Err(format!("节点 `{node_id}` 不存在"));
                }
                bridge::select_node(Some(node_id.clone()));
            } else {
                bridge::select_node(None);
            }
            Ok(
                json!({ "summary": node_id.map_or_else(|| "清除选择".to_string(), |id| format!("选中 {id}")), "selected_node": bridge::selected_node() }),
            )
        }
        "set_mode" => {
            let mode = required_str(params, "mode")?;
            let workspace = match mode {
                "edit" => WorkspaceMode::Edit,
                "run" => WorkspaceMode::Run,
                _ => return Err("mode 必须是 edit 或 run".to_string()),
            };
            bridge::set_workspace_mode(workspace);
            Ok(json!({ "summary": format!("切换到 {mode} 模式"), "mode": mode }))
        }
        "execute_editor_command" => execute_editor_command(params),
        "write_tag" => write_tag(params),
        "undo" => {
            let changed = bridge::undo();
            Ok(
                json!({ "summary": if changed { "已撤销" } else { "没有可撤销操作" }, "changed": changed }),
            )
        }
        "redo" => {
            let changed = bridge::redo();
            Ok(
                json!({ "summary": if changed { "已重做" } else { "没有可重做操作" }, "changed": changed }),
            )
        }
        "save" => {
            bridge::persist_now()?;
            Ok(json!({ "summary": "工程已保存" }))
        }
        "reset_camera_3d" => {
            bridge::reset_camera_3d();
            Ok(json!({ "summary": "3D 相机已重置" }))
        }
        other => Err(format!("未知控制动作 `{other}`")),
    }
}

fn inspect_project(params: &Value) -> Result<Value, String> {
    let project = bridge::project();
    if let Some(screen_id) = params.get("screen_id").and_then(Value::as_str) {
        let screen = project
            .screen(screen_id)
            .ok_or_else(|| format!("画面 `{screen_id}` 不存在"))?;
        Ok(json!({
            "summary": format!("读取画面 {screen_id}"),
            "project_name": project.name,
            "screen": screen,
            "tags": project.tags,
            "startup_screen": project.startup_screen,
        }))
    } else {
        Ok(json!({
            "summary": format!("读取工程 {}", project.name),
            "project": project,
        }))
    }
}

fn execute_editor_command(params: &Value) -> Result<Value, String> {
    let command_value = params
        .get("command")
        .ok_or_else(|| "缺少 command".to_string())?;
    let command: EditorCommand = serde_json::from_value(command_value.clone())
        .map_err(|error| format!("EditorCommand 无效: {error}"))?;
    let screen_id = command_screen_id(&command).map(str::to_string);
    let deletes_node = matches!(command, EditorCommand::DeleteNode { .. });
    if let Some(screen_id) = &screen_id
        && bridge::active_screen().as_deref() != Some(screen_id.as_str())
    {
        bridge::select_screen(screen_id);
    }
    let result = bridge::execute(command).map_err(|error| error.to_string())?;
    if !deletes_node
        && let Some(affected) = &result.affected_id
        && bridge::project_contains_node(&bridge::project(), affected)
    {
        bridge::select_node(Some(affected.clone()));
    }
    Ok(json!({
        "summary": format!("执行 {}，修订 {}", command_name(command_value), result.document_revision),
        "affected_id": result.affected_id,
        "document_revision": result.document_revision,
    }))
}

fn write_tag(params: &Value) -> Result<Value, String> {
    let tag_id = required_str(params, "tag_id")?;
    let value = params
        .get("value")
        .cloned()
        .ok_or_else(|| "缺少 value".to_string())?;
    let value: TagValue = serde_json::from_value(value).map_err(|error| error.to_string())?;
    bridge::write_tag(tag_id, value.clone()).map_err(|error| error.to_string())?;
    Ok(json!({
        "summary": format!("写入 {tag_id} = {}", value.display_text()),
        "tag_id": tag_id,
        "value": value,
    }))
}

pub fn current_state() -> ControlState {
    let project = bridge::project();
    ControlState {
        project_name: project.name.clone(),
        document_revision: bridge::document_revision(),
        tag_revision: bridge::tag_revision(),
        active_screen: bridge::active_screen(),
        selected_node: bridge::selected_node(),
        mode: match bridge::workspace_mode() {
            WorkspaceMode::Edit => "edit",
            WorkspaceMode::Run => "run",
        }
        .to_string(),
        can_undo: bridge::can_undo(),
        can_redo: bridge::can_redo(),
        screens: project
            .screens
            .iter()
            .map(|screen| ScreenSummary {
                id: screen.id().to_string(),
                name: screen.name().to_string(),
                kind: if screen.is_two_d() { "2d" } else { "3d" }.to_string(),
                nodes: match screen {
                    hmi_core::Screen::TwoD { nodes, .. } => nodes.len(),
                    hmi_core::Screen::ThreeD { nodes, .. } => nodes.len(),
                },
            })
            .collect(),
        tags: project.tags.len(),
    }
}

#[cfg(target_arch = "wasm32")]
fn send(message: BrowserMessage) {
    let Ok(text) = serde_json::to_string(&message) else {
        return;
    };
    SOCKET.with(|slot| {
        if let Some(socket) = slot.borrow().as_ref()
            && socket.ready_state() == WebSocket::OPEN
        {
            let _ = socket.send_with_str(&text);
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn current_url() -> String {
    CONFIG.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|config| config.url.clone())
            .unwrap_or_default()
    })
}

#[cfg(target_arch = "wasm32")]
fn push_activity(label: &str, detail: &str, status: AiStatus) -> u64 {
    let id = NEXT_ACTIVITY.with(|next| {
        let id = next.get();
        next.set(id.wrapping_add(1));
        id
    });
    let now = js_sys::Date::new_0();
    let time = format!(
        "{:02}:{:02}:{:02}",
        now.get_hours(),
        now.get_minutes(),
        now.get_seconds()
    );
    let mut activities = AI_ACTIVITY.write();
    activities.insert(
        0,
        AiActivity {
            id,
            time,
            label: label.to_string(),
            detail: detail.to_string(),
            status,
        },
    );
    activities.truncate(MAX_ACTIVITIES);
    id
}

#[cfg(target_arch = "wasm32")]
fn finish_activity(id: u64, status: AiStatus, detail: &str) {
    let mut activities = AI_ACTIVITY.write();
    if let Some(activity) = activities.iter_mut().find(|activity| activity.id == id) {
        activity.status = status;
        activity.detail = detail.to_string();
    }
}

#[cfg(target_arch = "wasm32")]
fn action_detail(action: &str, params: &Value) -> String {
    match action {
        "execute_editor_command" => params
            .get("command")
            .map(command_name)
            .unwrap_or_else(|| "EditorCommand".to_string()),
        "select_node" => params
            .get("node_id")
            .and_then(Value::as_str)
            .unwrap_or("清除选择")
            .to_string(),
        "select_screen" => params
            .get("screen_id")
            .and_then(Value::as_str)
            .unwrap_or("未指定画面")
            .to_string(),
        "write_tag" => format!(
            "{} = {}",
            params.get("tag_id").and_then(Value::as_str).unwrap_or("?"),
            params.get("value").cloned().unwrap_or(Value::Null)
        ),
        _ => action.to_string(),
    }
}

fn command_name(command: &Value) -> String {
    command
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown_command")
        .to_string()
}

fn command_screen_id(command: &EditorCommand) -> Option<&str> {
    match command {
        EditorCommand::AddScreen { screen } => Some(screen.id()),
        EditorCommand::DeleteScreen { screen_id, .. }
        | EditorCommand::RenameScreen { screen_id, .. }
        | EditorCommand::SetStartupScreen { screen_id }
        | EditorCommand::Add2dNode { screen_id, .. }
        | EditorCommand::Update2dNode { screen_id, .. }
        | EditorCommand::Add3dNode { screen_id, .. }
        | EditorCommand::Update3dNode { screen_id, .. }
        | EditorCommand::DeleteNode { screen_id, .. }
        | EditorCommand::DuplicateNode { screen_id, .. }
        | EditorCommand::SetBindings { screen_id, .. } => Some(screen_id),
        EditorCommand::AddTag { .. }
        | EditorCommand::UpdateTag { .. }
        | EditorCommand::DeleteTag { .. } => None,
    }
}

fn required_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("缺少字符串参数 {key}"))
}

#[cfg(target_arch = "wasm32")]
fn read_config() -> Option<ControlConfig> {
    let window = web_sys::window()?;
    let location = window.location();
    let search = location.search().unwrap_or_default();
    let params = parse_query(&search);
    if matches!(
        params.get("control").map(String::as_str),
        Some("off" | "0" | "false")
    ) {
        return None;
    }
    let control = params.get("control").map(String::as_str);
    if control.is_none() && !params.contains_key("control_token") {
        return None;
    }
    let url = match control {
        Some("1" | "true" | "on") | None => {
            format!("ws://127.0.0.1:{DEFAULT_CONTROL_PORT}{DEFAULT_CONTROL_PATH}")
        }
        Some(url) => url.to_string(),
    };
    Some(ControlConfig {
        url,
        token: params.get("control_token").cloned(),
    })
}

#[cfg(target_arch = "wasm32")]
fn parse_query(search: &str) -> std::collections::HashMap<String, String> {
    search
        .trim_start_matches('?')
        .split('&')
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            Some((decode_component(key), decode_component(value)))
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn decode_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    output.push(byte);
                    index += 3;
                } else {
                    output.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    use std::sync::atomic::AtomicBool;

    #[test]
    fn codex_command_contains_the_mcp_endpoint() {
        assert_eq!(
            codex_mcp_command_for("http://127.0.0.1:17880/mcp?token=token-123"),
            "codex mcp add mox-hmi --url 'http://127.0.0.1:17880/mcp?token=token-123'"
        );
    }

    #[test]
    fn codex_command_is_empty_without_an_endpoint() {
        assert!(codex_mcp_command_for("").is_empty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_mcp_bridge_updates_the_switch_state_and_command() {
        fn root() -> Element {
            rsx! {}
        }

        let mut dom = dioxus::core::VirtualDom::new(root);
        dom.rebuild_in_place();
        let _guard = dioxus::core::RuntimeGuard::new(dom.runtime());
        let enabled = Arc::new(AtomicBool::new(false));
        let read_enabled = enabled.clone();
        let write_enabled = enabled.clone();
        install_native_mcp_bridge(NativeMcpBridge::new(
            Arc::new(move || read_enabled.load(Ordering::Acquire)),
            Arc::new(move |next| {
                write_enabled.store(next, Ordering::Release);
                Ok(())
            }),
            Arc::new(|| "http://127.0.0.1:17880/mcp".to_string()),
        ))
        .unwrap();

        start();
        assert!(!MCP_ENABLED());

        set_mcp_enabled(true).unwrap();
        assert!(MCP_ENABLED());
        assert_eq!(
            codex_mcp_command(),
            "codex mcp add mox-hmi --url 'http://127.0.0.1:17880/mcp'"
        );
    }
}
