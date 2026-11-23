use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod tools;

pub const PROTOCOL_VERSION: u32 = 1;
pub const DEFAULT_CONTROL_PORT: u16 = 17880;
pub const DEFAULT_CONTROL_PATH: &str = "/control";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserMessage {
    Hello {
        protocol: u32,
        #[serde(default)]
        token: Option<String>,
        app: String,
        state: ControlState,
    },
    Response {
        id: u64,
        ok: bool,
        #[serde(default)]
        result: Value,
        #[serde(default)]
        error: Option<String>,
        state: ControlState,
        duration_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        protocol: u32,
        session_id: u64,
    },
    Request {
        id: u64,
        action: String,
        label: String,
        #[serde(default)]
        params: Value,
    },
    Reject {
        reason: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ControlState {
    pub project_name: String,
    pub document_revision: u64,
    pub tag_revision: u64,
    pub active_screen: Option<String>,
    pub selected_node: Option<String>,
    pub mode: String,
    pub can_undo: bool,
    pub can_redo: bool,
    pub screens: Vec<ScreenSummary>,
    pub tags: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub nodes: usize,
}
