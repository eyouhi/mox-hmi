use hmi_control_protocol::{BrowserMessage, ServerMessage};
use mox_hmi_control::{ControlHub, hub::BrowserHandle};
use serde_json::Value;
use std::{
    cell::RefCell,
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};
use tokio::sync::mpsc as tokio_mpsc;

struct NativeRequest {
    id: u64,
    action: String,
    params: Value,
}

pub struct NativeControl {
    requests: Arc<Mutex<mpsc::Receiver<NativeRequest>>>,
    responses: tokio_mpsc::UnboundedSender<BrowserMessage>,
    handle: BrowserHandle,
    runtime: tokio::runtime::Handle,
}

thread_local! {
    static INSTALLED: RefCell<Option<NativeControl>> = const { RefCell::new(None) };
}

pub fn install(control: NativeControl) {
    INSTALLED.with(|slot| *slot.borrow_mut() = Some(control));
}

pub fn with_installed(use_control: impl FnOnce(&NativeControl)) {
    INSTALLED.with(|slot| {
        if let Some(control) = slot.borrow().as_ref() {
            use_control(control);
        }
    });
}

impl NativeControl {
    pub fn start(hub: ControlHub, runtime: &tokio::runtime::Handle) -> Result<Self, String> {
        let (request_tx, request_rx) = mpsc::channel();
        let (out_tx, mut out_rx) = tokio_mpsc::unbounded_channel();
        let (response_tx, mut response_rx) = tokio_mpsc::unbounded_channel();
        let initial = mox_hmi_ui::control::current_state();
        let handle = runtime.block_on(hub.register(initial, out_tx));
        let responder = handle.clone();
        runtime.spawn(async move {
            loop {
                tokio::select! {
                    message = out_rx.recv() => {
                        let Some(ServerMessage::Request { id, action, params, .. }) = message else {
                            if out_rx.is_closed() {
                                break;
                            }
                            continue;
                        };
                        if request_tx
                            .send(NativeRequest { id, action, params })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Some(response) = response_rx.recv() => responder.complete(response).await,
                    else => break,
                }
            }
        });
        Ok(Self {
            requests: Arc::new(Mutex::new(request_rx)),
            responses: response_tx,
            handle,
            runtime: runtime.clone(),
        })
    }

    pub fn process(&self) {
        let Ok(requests) = self.requests.lock() else {
            return;
        };
        while let Ok(request) = requests.try_recv() {
            let started = Instant::now();
            let result = mox_hmi_ui::control::run_action(&request.action, &request.params);
            let state = mox_hmi_ui::control::current_state();
            let message = match result {
                Ok(result) => BrowserMessage::Response {
                    id: request.id,
                    ok: true,
                    result,
                    error: None,
                    state,
                    duration_ms: started.elapsed().as_millis() as u64,
                },
                Err(error) => BrowserMessage::Response {
                    id: request.id,
                    ok: false,
                    result: Value::Null,
                    error: Some(error),
                    state,
                    duration_ms: started.elapsed().as_millis() as u64,
                },
            };
            let _ = self.responses.send(message);
        }
    }

    pub fn sync_state(&self) {
        let state = mox_hmi_ui::control::current_state();
        let handle = self.handle.clone();
        self.runtime.spawn(async move {
            handle.update_state(state).await;
        });
    }
}
