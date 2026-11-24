use crate::{ControlHub, mcp};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::SinkExt;
use hmi_control_protocol::{BrowserMessage, PROTOCOL_VERSION, ServerMessage};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::mpsc, time::timeout};
use tower_http::services::{ServeDir, ServeFile};

pub type HapticCallback = Arc<dyn Fn() + Send + Sync>;
pub type EnabledCallback = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone)]
pub struct HttpConfig {
    pub token: Option<String>,
    pub web_dir: Option<PathBuf>,
    pub haptic: Option<HapticCallback>,
    /// Optional runtime gate for the MCP endpoint and browser control tunnel.
    /// `None` keeps the standalone control service enabled.
    pub control_enabled: Option<EnabledCallback>,
}

#[derive(Clone)]
struct AppState {
    hub: ControlHub,
    token: Option<String>,
    haptic: Option<HapticCallback>,
    control_enabled: Option<EnabledCallback>,
    session_id: Arc<str>,
}

pub async fn serve(
    listener: TcpListener,
    hub: ControlHub,
    config: HttpConfig,
) -> Result<(), String> {
    let state = AppState {
        hub,
        token: config.token,
        haptic: config.haptic,
        control_enabled: config.control_enabled,
        session_id: Arc::from("mox-hmi-local"),
    };

    let mut router = Router::new()
        .route("/control", get(ws_upgrade))
        .route("/mcp", post(mcp_post).get(mcp_get))
        .route("/haptic", post(haptic_post))
        .route("/health", get(health));

    if let Some(web_dir) = config.web_dir {
        let index = web_dir.join("index.html");
        if !index.is_file() {
            return Err(format!(
                "Web 静态目录缺少 index.html: {}",
                web_dir.display()
            ));
        }
        let static_files = ServeDir::new(&web_dir).not_found_service(ServeFile::new(index));
        router = router.fallback_service(static_files);
    }

    axum::serve(listener, router.with_state(state))
        .await
        .map_err(|error| format!("HTTP 服务失败: {error}"))
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    let enabled = control_enabled(&state);
    Json(json!({
        "ok": true,
        "service": "mox-hmi-editor",
        "ws": "/control",
        "mcp": "/mcp",
        "control_enabled": enabled,
        "mcp_enabled": enabled,
        "haptic": state.haptic.is_some(),
    }))
}

async fn haptic_post(State(state): State<AppState>) -> StatusCode {
    let Some(callback) = state.haptic else {
        return StatusCode::NOT_FOUND;
    };
    callback();
    StatusCode::NO_CONTENT
}

async fn mcp_get(State(state): State<AppState>) -> Response {
    if !control_enabled(&state) {
        return control_disabled();
    }
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({
            "error": "MOX HMI 的 MCP 使用 streamable HTTP POST，不支持独立 SSE 流"
        })),
    )
        .into_response()
}

async fn mcp_post(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    if !control_enabled(&state) {
        return control_disabled();
    }
    if !token_matches(&state, &query, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "控制令牌无效" })),
        )
            .into_response();
    }
    state.hub.mark_ai_connected();

    let request: Value = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return Json(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": { "code": -32700, "message": format!("JSON 解析失败: {error}") }
            }))
            .into_response();
        }
    };

    let response = match request {
        Value::Array(requests) => {
            let mut responses = Vec::new();
            for request in requests {
                if let Some(response) = mcp::handle_jsonrpc(&state.hub, request).await {
                    responses.push(response);
                }
            }
            (!responses.is_empty()).then_some(Value::Array(responses))
        }
        request => mcp::handle_jsonrpc(&state.hub, request).await,
    };

    let Some(response) = response else {
        return StatusCode::ACCEPTED.into_response();
    };

    let mut response = Json(response).into_response();
    if let Ok(value) = HeaderValue::from_str(&state.session_id) {
        response.headers_mut().insert("mcp-session-id", value);
    }
    response.headers_mut().insert(
        "mcp-protocol-version",
        HeaderValue::from_static("2025-06-18"),
    );
    response
}

fn control_enabled(state: &AppState) -> bool {
    state
        .control_enabled
        .as_ref()
        .map(|enabled| enabled())
        .unwrap_or(true)
}

fn control_disabled() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": "MCP 已关闭，请在 MOX HMI“AI 操作隧道”中启用"
        })),
    )
        .into_response()
}

fn token_matches(
    state: &AppState,
    query: &HashMap<String, String>,
    headers: &axum::http::HeaderMap,
) -> bool {
    let Some(expected) = state.token.as_deref() else {
        return true;
    };
    if query.get("token").is_some_and(|token| token == expected) {
        return true;
    }
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {expected}"))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    if !control_enabled(&state) {
        return control_disabled();
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state))
        .into_response()
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let first = match timeout(Duration::from_secs(10), socket.recv()).await {
        Err(_) => {
            reject(socket, "等待浏览器 hello 超时").await;
            return;
        }
        Ok(None) => return,
        Ok(Some(Err(error))) => {
            eprintln!("control tunnel hello read error: {error}");
            return;
        }
        Ok(Some(Ok(message))) => message,
    };

    let Message::Text(text) = first else {
        reject(socket, "第一条消息必须是 hello").await;
        return;
    };
    let hello: BrowserMessage = match serde_json::from_str(text.as_str()) {
        Ok(message) => message,
        Err(error) => {
            reject(socket, &format!("hello JSON 无效: {error}")).await;
            return;
        }
    };
    let BrowserMessage::Hello {
        protocol,
        token,
        app,
        state: browser_state,
    } = hello
    else {
        reject(socket, "第一条消息必须是 hello").await;
        return;
    };
    if protocol != PROTOCOL_VERSION {
        reject(
            socket,
            &format!("协议版本不匹配: browser={protocol}, server={PROTOCOL_VERSION}"),
        )
        .await;
        return;
    }
    if !state.hub.token_matches(token.as_deref()) {
        reject(socket, "控制令牌无效").await;
        return;
    }

    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();
    let browser = state.hub.register(browser_state, outbound_tx).await;
    eprintln!(
        "control tunnel session {} connected: {app}",
        browser.session_id
    );
    if send(
        &mut socket,
        &ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            session_id: browser.session_id,
        },
    )
    .await
    .is_err()
    {
        state.hub.unregister(browser.session_id).await;
        return;
    }

    let mut enabled_check = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = enabled_check.tick() => {
                if !control_enabled(&state) {
                    let _ = send(
                        &mut socket,
                        &ServerMessage::Reject {
                            reason: "MCP 已关闭".to_string(),
                        },
                    )
                    .await;
                    break;
                }
            }
            outbound = outbound_rx.recv() => {
                let Some(message) = outbound else { break; };
                if send(&mut socket, &message).await.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => {
                let Some(incoming) = incoming else { break; };
                let message = match incoming {
                    Ok(message) => message,
                    Err(error) => {
                        eprintln!("control tunnel read error: {error}");
                        break;
                    }
                };
                match message {
                    Message::Text(text) => {
                        if let Ok(message) = serde_json::from_str::<BrowserMessage>(text.as_str()) {
                            browser.complete(message).await;
                        }
                    }
                    Message::Ping(payload) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }

    state.hub.unregister(browser.session_id).await;
    eprintln!("control tunnel session {} disconnected", browser.session_id);
}

async fn send(socket: &mut WebSocket, message: &ServerMessage) -> Result<(), ()> {
    let text = serde_json::to_string(message).map_err(|_| ())?;
    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}

async fn reject(mut socket: WebSocket, reason: &str) {
    let _ = send(
        &mut socket,
        &ServerMessage::Reject {
            reason: reason.to_string(),
        },
    )
    .await;
    let _ = socket.close().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[tokio::test]
    async fn mcp_routes_follow_the_runtime_gate() {
        let enabled = Arc::new(AtomicBool::new(false));
        let gate = enabled.clone();
        let state = AppState {
            hub: ControlHub::new(None),
            token: None,
            haptic: None,
            control_enabled: Some(Arc::new(move || {
                gate.load(std::sync::atomic::Ordering::Acquire)
            })),
            session_id: Arc::from("test-session"),
        };

        let disabled = mcp_get(State(state.clone())).await;
        assert_eq!(disabled.status(), StatusCode::NOT_FOUND);

        enabled.store(true, std::sync::atomic::Ordering::Release);
        let enabled = mcp_get(State(state)).await;
        assert_eq!(enabled.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
