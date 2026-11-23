use hmi_control_protocol::{BrowserMessage, ControlState, ServerMessage};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::{Duration, timeout};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct ControlHub {
    inner: Arc<HubInner>,
    token: Option<String>,
    next_session_id: Arc<AtomicU64>,
}

struct HubInner {
    browser: Mutex<Option<BrowserHandle>>,
    last_state: Mutex<ControlState>,
    ai_connected: AtomicBool,
}

#[derive(Clone)]
pub struct BrowserHandle {
    pub session_id: u64,
    tx: mpsc::UnboundedSender<ServerMessage>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<BrowserReply>>>>,
    next_request_id: Arc<AtomicU64>,
    state: Arc<Mutex<ControlState>>,
}

#[derive(Debug, Clone)]
pub struct BrowserReply {
    pub ok: bool,
    pub result: Value,
    pub error: Option<String>,
    pub state: ControlState,
    pub duration_ms: u64,
}

impl ControlHub {
    pub fn new(token: Option<String>) -> Self {
        Self {
            inner: Arc::new(HubInner {
                browser: Mutex::new(None),
                last_state: Mutex::new(ControlState::default()),
                ai_connected: AtomicBool::new(false),
            }),
            token,
            next_session_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn token_matches(&self, candidate: Option<&str>) -> bool {
        match &self.token {
            Some(expected) => candidate == Some(expected.as_str()),
            None => true,
        }
    }

    pub fn mark_ai_connected(&self) {
        self.inner.ai_connected.store(true, Ordering::Relaxed);
    }

    pub fn ai_connected(&self) -> bool {
        self.inner.ai_connected.load(Ordering::Relaxed)
    }

    pub async fn register(
        &self,
        state: ControlState,
        tx: mpsc::UnboundedSender<ServerMessage>,
    ) -> BrowserHandle {
        let state = Arc::new(Mutex::new(state));
        let handle = BrowserHandle {
            session_id: self.next_session_id.fetch_add(1, Ordering::Relaxed),
            tx,
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_request_id: Arc::new(AtomicU64::new(1)),
            state: state.clone(),
        };
        *self.inner.browser.lock().await = Some(handle.clone());
        *self.inner.last_state.lock().await = state.lock().await.clone();
        handle
    }

    pub async fn unregister(&self, session_id: u64) {
        let mut browser = self.inner.browser.lock().await;
        if browser
            .as_ref()
            .is_some_and(|handle| handle.session_id == session_id)
        {
            *browser = None;
        }
    }

    pub async fn request(
        &self,
        action: &str,
        label: &str,
        params: Value,
    ) -> Result<BrowserReply, String> {
        let handle = self
            .inner
            .browser
            .lock()
            .await
            .clone()
            .ok_or_else(|| "MOX HMI 浏览器尚未连接控制隧道，请先打开 HMI 页面".to_string())?;
        let reply = handle.request(action, label, params).await?;
        *self.inner.last_state.lock().await = reply.state.clone();
        Ok(reply)
    }

    pub async fn status(&self) -> Value {
        let browser = self.inner.browser.lock().await.clone();
        let (connected, session_id, state) = match browser {
            Some(handle) => (
                true,
                Some(handle.session_id),
                handle.state.lock().await.clone(),
            ),
            None => (false, None, self.inner.last_state.lock().await.clone()),
        };
        json!({
            "connected": connected,
            "session_id": session_id,
            "token_required": self.token.is_some(),
            "state": state,
        })
    }
}

impl BrowserHandle {
    pub async fn update_state(&self, state: ControlState) {
        *self.state.lock().await = state;
    }

    async fn request(
        &self,
        action: &str,
        label: &str,
        params: Value,
    ) -> Result<BrowserReply, String> {
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let (response_tx, response_rx) = oneshot::channel();
        self.pending.lock().await.insert(id, response_tx);

        let message = ServerMessage::Request {
            id,
            action: action.to_string(),
            label: label.to_string(),
            params,
        };
        if self.tx.send(message).is_err() {
            self.pending.lock().await.remove(&id);
            return Err("控制隧道已断开".to_string());
        }

        match timeout(REQUEST_TIMEOUT, response_rx).await {
            Ok(Ok(reply)) => {
                *self.state.lock().await = reply.state.clone();
                Ok(reply)
            }
            Ok(Err(_)) => {
                self.pending.lock().await.remove(&id);
                Err("浏览器在返回结果前断开".to_string())
            }
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err("等待浏览器执行超时（20 秒）".to_string())
            }
        }
    }

    pub async fn complete(&self, message: BrowserMessage) {
        let BrowserMessage::Response {
            id,
            ok,
            result,
            error,
            state,
            duration_ms,
        } = message
        else {
            return;
        };
        *self.state.lock().await = state.clone();
        if let Some(sender) = self.pending.lock().await.remove(&id) {
            let _ = sender.send(BrowserReply {
                ok,
                result,
                error,
                state,
                duration_ms,
            });
        }
    }
}
