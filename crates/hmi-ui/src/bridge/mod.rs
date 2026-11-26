use dioxus::prelude::*;
use hmi_core::{
    BarDirection, BatchCommandResult, ButtonAction, CommandResult, DeviceModel, DeviceNode,
    EditorCommand, EditorSession, EquipmentKind, HexColor, HmiError, ProjectDocument, Rect, Screen,
    TagSnapshot, TagValue, TagValueType, TextAlign, UiNode, Widget2D, bas_demo,
    display_axis_to_scene, export_filename, from_json, to_json, unique_node_id,
};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
};

#[cfg(not(target_arch = "wasm32"))]
static NATIVE_TEXT_ENTRY_FOCUSED: AtomicBool = AtomicBool::new(false);
#[cfg(not(target_arch = "wasm32"))]
static NATIVE_UI_CAPTURES_POINTER: AtomicBool = AtomicBool::new(false);
#[cfg(not(target_arch = "wasm32"))]
static NATIVE_POINTER_ON_CANVAS: AtomicBool = AtomicBool::new(false);
#[cfg(not(target_arch = "wasm32"))]
static NATIVE_HAPTIC_HANDLER: OnceLock<fn()> = OnceLock::new();

pub mod storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMode {
    Edit,
    Run,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStatus {
    Saved,
    Pending,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

impl NoticeLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            NoticeLevel::Info => "info",
            NoticeLevel::Warning => "warning",
            NoticeLevel::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GamepadStatus {
    pub connected: bool,
    pub id: String,
}

impl GamepadStatus {
    pub fn is_dualsense(&self) -> bool {
        let id = self.id.to_ascii_lowercase();
        id.contains("dualsense") || id.contains("ps5 controller") || id.contains("playstation 5")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControllerTuningHud {
    pub label: String,
    pub value: f32,
    pub step: f32,
    pub index: usize,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamepadMenuHud {
    pub title: String,
    pub items: Vec<GamepadMenuItemHud>,
    pub index: usize,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamepadMenuItemHud {
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GuideSection {
    #[default]
    Start,
    TwoD,
    ThreeD,
    Gamepad,
}

impl GuideSection {
    pub fn next(self, direction: isize) -> Self {
        match (self, direction.is_negative()) {
            (GuideSection::Start, true) => GuideSection::Gamepad,
            (GuideSection::Start, false) => GuideSection::TwoD,
            (GuideSection::TwoD, true) => GuideSection::Start,
            (GuideSection::TwoD, false) => GuideSection::ThreeD,
            (GuideSection::ThreeD, true) => GuideSection::TwoD,
            (GuideSection::ThreeD, false) => GuideSection::Gamepad,
            (GuideSection::Gamepad, true) => GuideSection::ThreeD,
            (GuideSection::Gamepad, false) => GuideSection::Start,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GuideScrollRequest {
    pub revision: u64,
    pub delta: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteEntry {
    pub kind: &'static str,
    pub label: &'static str,
}

pub const PALETTE_2D: &[PaletteEntry] = &[
    PaletteEntry {
        kind: "panel",
        label: "面板",
    },
    PaletteEntry {
        kind: "label",
        label: "文本",
    },
    PaletteEntry {
        kind: "value-card",
        label: "数值卡",
    },
    PaletteEntry {
        kind: "status-lamp",
        label: "状态灯",
    },
    PaletteEntry {
        kind: "bar-gauge",
        label: "仪表条",
    },
    PaletteEntry {
        kind: "button",
        label: "按钮",
    },
    PaletteEntry {
        kind: "pipe",
        label: "管线",
    },
    PaletteEntry {
        kind: "equipment",
        label: "设备",
    },
];

pub const PALETTE_3D: &[PaletteEntry] = &[
    PaletteEntry {
        kind: "ahu",
        label: "AHU",
    },
    PaletteEntry {
        kind: "fan",
        label: "风机",
    },
    PaletteEntry {
        kind: "pump",
        label: "水泵",
    },
    PaletteEntry {
        kind: "duct",
        label: "风管",
    },
    PaletteEntry {
        kind: "pipe-3d",
        label: "水管",
    },
    PaletteEntry {
        kind: "valve",
        label: "阀门",
    },
];

pub fn palette_entries_for_screen(screen: &Screen) -> &'static [PaletteEntry] {
    match screen {
        Screen::TwoD { .. } => PALETTE_2D,
        Screen::ThreeD { .. } => PALETTE_3D,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus {
    Starting,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneLabel3D {
    pub node_id: String,
    pub title: String,
    pub value: String,
    pub left_pct: f32,
    pub top_pct: f32,
    pub point_left_pct: f32,
    pub point_top_pct: f32,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AxisGizmo3D {
    pub x: [f32; 2],
    pub y: [f32; 2],
    pub z: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeleteFxTarget {
    TwoD { node: UiNode },
    ThreeD { node: DeviceNode },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteFxEffect {
    pub screen_id: String,
    pub document_revision: u64,
    pub targets: Vec<DeleteFxTarget>,
}

impl DeleteFxEffect {
    pub fn is_three_d(&self) -> bool {
        matches!(self.targets.first(), Some(DeleteFxTarget::ThreeD { .. }))
    }
}

pub static DOCUMENT_REVISION: GlobalSignal<u64> = Signal::global(|| 1);
pub static TAG_REVISION: GlobalSignal<u64> = Signal::global(|| 1);
pub static SELECTED_NODE: GlobalSignal<Option<String>> = Signal::global(|| None);
pub static SELECTED_NODES: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
pub static ACTIVE_SCREEN: GlobalSignal<Option<String>> = Signal::global(|| None);
pub static WORKSPACE_MODE: GlobalSignal<WorkspaceMode> = Signal::global(|| WorkspaceMode::Edit);
pub static NOTICE_REVISION: GlobalSignal<u64> = Signal::global(|| 0);
pub static ENGINE_STATUS: GlobalSignal<EngineStatus> = Signal::global(|| EngineStatus::Starting);
pub static SAVE_STATUS: GlobalSignal<SaveStatus> = Signal::global(|| SaveStatus::Saved);
pub static VIEW_LABELS_3D: GlobalSignal<Vec<SceneLabel3D>> = Signal::global(Vec::new);
pub static AXIS_GIZMO_3D: GlobalSignal<Option<AxisGizmo3D>> = Signal::global(|| None);
pub static CAMERA_RESET_REVISION: GlobalSignal<u32> = Signal::global(|| 0);
pub static WEBGPU_READY: GlobalSignal<Option<bool>> = Signal::global(|| None);
pub static FIELD_ERROR_ANNOUNCEMENT: GlobalSignal<Option<String>> = Signal::global(|| None);
pub static RUNTIME_ONLY: GlobalSignal<bool> = Signal::global(|| false);
pub static GUIDE_OPEN: GlobalSignal<bool> = Signal::global(|| false);
pub static GAMEPAD_STATUS: GlobalSignal<GamepadStatus> = Signal::global(GamepadStatus::default);
pub static SELECTED_TAG: GlobalSignal<Option<String>> = Signal::global(|| None);
pub static DELETE_FX: GlobalSignal<Option<DeleteFxEffect>> = Signal::global(|| None);
pub static DELETE_FX_REVISION: GlobalSignal<u64> = Signal::global(|| 0);
pub static DELETE_FX_ENABLED: GlobalSignal<bool> = Signal::global(|| true);
pub static CONTROLLER_TUNING_HUD: GlobalSignal<Option<ControllerTuningHud>> =
    Signal::global(|| None);
pub static GAMEPAD_MENU_HUD: GlobalSignal<Option<GamepadMenuHud>> = Signal::global(|| None);
pub static GUIDE_SECTION: GlobalSignal<GuideSection> = Signal::global(GuideSection::default);
pub static GUIDE_SCROLL_REQUEST: GlobalSignal<GuideScrollRequest> =
    Signal::global(GuideScrollRequest::default);

thread_local! {
    static SESSION: RefCell<Option<Rc<RefCell<EditorSession>>>> = const { RefCell::new(None) };
    static NOTICE: RefCell<Option<Notice>> = const { RefCell::new(None) };
    static CLIPBOARD: RefCell<Option<CanvasClipboard>> = const { RefCell::new(None) };
    static PASTE_SERIAL: Cell<u32> = const { Cell::new(0) };
    static VIEW_CENTER_2D: Cell<[f32; 2]> = const { Cell::new([800.0, 450.0]) };
    static VIEW_FOCUS_3D: Cell<[f32; 3]> = const { Cell::new([0.0, 1.4, 0.0]) };
    static ZOOM_2D: Cell<f32> = const { Cell::new(1.0) };
    static SCENE_SHORTCUTS_SUPPRESSED_UNTIL_MS: Cell<f64> = const { Cell::new(0.0) };
    static UI_RUNTIME_PINNED: Cell<bool> = const { Cell::new(false) };
    static SESSION_NEEDS_SAVE: Cell<bool> = const { Cell::new(false) };
}

#[derive(Debug, Clone)]
enum CanvasClipboard {
    TwoD(Vec<UiNode>),
    ThreeD(Vec<DeviceNode>),
}

pub fn suppress_scene_shortcuts_for(duration_ms: f64) {
    let deadline = now_ms() + duration_ms.max(0.0);
    SCENE_SHORTCUTS_SUPPRESSED_UNTIL_MS.with(|until| {
        if deadline > until.get() {
            until.set(deadline);
        }
    });
}

pub fn scene_shortcuts_suppressed() -> bool {
    SCENE_SHORTCUTS_SUPPRESSED_UNTIL_MS.with(|until| now_ms() < until.get())
}

pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64() * 1_000.0)
            .unwrap_or(0.0)
    }
}

pub fn initialize() -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    ensure_native_runtime();
    #[cfg(target_arch = "wasm32")]
    ensure_web_runtime();
    if SESSION.with(|slot| slot.borrow().is_some()) {
        publish_session_state();
        return Ok(());
    }
    SESSION.with(|slot| {
        let (project, warning, needs_save) = match storage::load_raw() {
            Ok(Some(raw)) => match from_json(&raw) {
                Ok(project) => (project, None, false),
                Err(error) => (
                    bas_demo(),
                    Some(format!("自动保存工程无效，已加载内置示例：{error}")),
                    true,
                ),
            },
            Ok(None) => (bas_demo(), None, true),
            Err(error) => (
                bas_demo(),
                Some(format!("localStorage 不可用，工程只保存在内存中：{error}")),
                true,
            ),
        };
        SESSION_NEEDS_SAVE.with(|flag| flag.set(needs_save));
        let session = EditorSession::new(project).map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        let startup = session.project().startup_screen.clone();
        *slot.borrow_mut() = Some(Rc::new(RefCell::new(session)));
        *DOCUMENT_REVISION.write() = 1;
        *TAG_REVISION.write() = 1;
        *ACTIVE_SCREEN.write() = Some(startup);
        let runtime_only = runtime_mode_requested();
        *RUNTIME_ONLY.write() = runtime_only;
        if runtime_only {
            *WORKSPACE_MODE.write() = WorkspaceMode::Run;
        }
        *SAVE_STATUS.write() = if needs_save {
            SaveStatus::Pending
        } else {
            SaveStatus::Saved
        };
        if let Some(message) = warning {
            set_notice(NoticeLevel::Warning, message);
        }
        Ok(())
    })
}

/// Pin the runtime of the live UI so Bevy callbacks resolve the same global
/// signals that the components read. Call this from the root component.
pub fn pin_ui_runtime() {
    UI_RUNTIME_PINNED.with(|pinned| {
        if pinned.get() {
            return;
        }
        pinned.set(true);
        let guard = dioxus::core::RuntimeGuard::new(dioxus::core::Runtime::current());
        std::mem::forget(guard);
    });
}

fn publish_session_state() {
    let Ok((revision, startup_screen)) = with_session(|session| {
        (
            session.document_revision(),
            session.project().startup_screen.clone(),
        )
    }) else {
        return;
    };
    *DOCUMENT_REVISION.write() = revision;
    *TAG_REVISION.write() = 1;
    *ACTIVE_SCREEN.write() = Some(startup_screen);
    set_selection_signals(Vec::new());
    *RUNTIME_ONLY.write() = runtime_mode_requested();
    *WORKSPACE_MODE.write() = if runtime_mode_requested() {
        WorkspaceMode::Run
    } else {
        WorkspaceMode::Edit
    };
    *SAVE_STATUS.write() = if SESSION_NEEDS_SAVE.with(Cell::get) {
        SaveStatus::Pending
    } else {
        SaveStatus::Saved
    };
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_native_runtime() {
    use std::sync::Once;

    static INIT: Once = Once::new();
    INIT.call_once(|| {
        fn root() -> Element {
            rsx! {}
        }

        let mut dom = dioxus::core::VirtualDom::new(root);
        dom.rebuild_in_place();
        let runtime = dom.runtime();
        let guard = dioxus::core::RuntimeGuard::new(runtime);
        std::mem::forget(dom);
        std::mem::forget(guard);
    });
}

/// Keep the Web UI runtime on the thread stack for the Bevy render loop.
#[cfg(target_arch = "wasm32")]
fn ensure_web_runtime() {
    pin_ui_runtime();
}

fn with_session<T>(f: impl FnOnce(&mut EditorSession) -> T) -> Result<T, String> {
    SESSION.with(|slot| {
        let session = slot
            .borrow()
            .as_ref()
            .cloned()
            .ok_or_else(|| "EditorSession 未初始化".to_string())?;
        let mut session = session.borrow_mut();
        Ok(f(&mut session))
    })
}

pub fn project() -> ProjectDocument {
    with_session(|session| session.project().clone()).unwrap_or_else(|_| bas_demo())
}

pub fn tag_snapshot() -> TagSnapshot {
    with_session(|session| session.tag_snapshot().clone()).unwrap_or_default()
}

pub fn document_revision() -> u64 {
    *DOCUMENT_REVISION.read()
}

pub fn tag_revision() -> u64 {
    *TAG_REVISION.read()
}

pub fn delete_fx_revision() -> u64 {
    *DELETE_FX_REVISION.read()
}

pub fn delete_fx_enabled() -> bool {
    DELETE_FX_ENABLED()
}

pub fn set_delete_fx_enabled(enabled: bool) {
    *DELETE_FX_ENABLED.write() = enabled;
}

pub fn delete_fx() -> Option<DeleteFxEffect> {
    DELETE_FX()
}

fn collect_delete_fx(commands: &[EditorCommand]) -> Option<DeleteFxEffect> {
    if !delete_fx_enabled() {
        return None;
    }
    let screen_id = active_screen()?;
    let project = project();
    let screen = project.screen(&screen_id)?;
    let mut seen = HashSet::new();
    let mut targets = Vec::new();
    for command in commands {
        let EditorCommand::DeleteNode {
            screen_id: command_screen,
            node_id,
        } = command
        else {
            continue;
        };
        if command_screen != &screen_id || !seen.insert(node_id.clone()) {
            continue;
        }
        match screen {
            Screen::TwoD { nodes, .. } => {
                if let Some(node) = nodes.iter().find(|node| &node.id == node_id) {
                    targets.push(DeleteFxTarget::TwoD { node: node.clone() });
                }
            }
            Screen::ThreeD { nodes, .. } => {
                if let Some(node) = nodes.iter().find(|node| &node.id == node_id) {
                    targets.push(DeleteFxTarget::ThreeD { node: node.clone() });
                }
            }
        }
    }
    if targets.is_empty() {
        None
    } else {
        Some(DeleteFxEffect {
            screen_id,
            document_revision: 0,
            targets,
        })
    }
}

fn publish_delete_fx(mut effect: Option<DeleteFxEffect>) {
    let Some(effect) = effect.as_mut() else {
        return;
    };
    if !delete_fx_enabled() {
        return;
    }
    effect.document_revision = document_revision();
    *DELETE_FX.write() = Some(effect.clone());
    *DELETE_FX_REVISION.write() += 1;
}

pub fn execute(command: EditorCommand) -> Result<CommandResult, HmiError> {
    let delete_fx = collect_delete_fx(std::slice::from_ref(&command));
    let result =
        with_session(|session| session.execute(command)).map_err(HmiError::InvalidCommand)??;
    after_document_change(result.document_revision);
    publish_delete_fx(delete_fx);
    Ok(result)
}

pub fn execute_batch(commands: Vec<EditorCommand>) -> Result<BatchCommandResult, HmiError> {
    let delete_fx = collect_delete_fx(&commands);
    let result = with_session(|session| session.execute_batch(commands))
        .map_err(HmiError::InvalidCommand)??;
    after_document_change(result.document_revision);
    publish_delete_fx(delete_fx);
    Ok(result)
}

pub fn undo() -> bool {
    let changed = with_session(EditorSession::undo).unwrap_or(false);
    if changed {
        after_history_change();
    }
    changed
}

pub fn redo() -> bool {
    let changed = with_session(EditorSession::redo).unwrap_or(false);
    if changed {
        after_history_change();
    }
    changed
}

pub fn can_undo() -> bool {
    with_session(|session| session.can_undo()).unwrap_or(false)
}

pub fn can_redo() -> bool {
    with_session(|session| session.can_redo()).unwrap_or(false)
}

fn after_history_change() {
    let revision = with_session(|session| session.document_revision()).unwrap_or(1);
    after_document_change(revision);
}

fn after_document_change(revision: u64) {
    *DOCUMENT_REVISION.write() = revision;
    *TAG_REVISION.write() += 1;
    *SAVE_STATUS.write() = SaveStatus::Pending;
    prune_selection();
}

fn prune_selection() {
    let current = project();
    let active = active_screen().unwrap_or_else(|| current.startup_screen.clone());
    let Some(screen) = current.screen(&active) else {
        set_selection_signals(Vec::new());
        return;
    };
    let mut valid = HashSet::new();
    match screen {
        Screen::TwoD { nodes, .. } => valid.extend(nodes.iter().map(|node| node.id.as_str())),
        Screen::ThreeD { nodes, .. } => valid.extend(nodes.iter().map(|node| node.id.as_str())),
    }
    let existing = SELECTED_NODES();
    let mut seen = HashSet::new();
    let filtered: Vec<String> = existing
        .into_iter()
        .filter(|id| valid.contains(id.as_str()) && seen.insert(id.clone()))
        .collect();
    set_selection_signals(filtered);
}

fn set_selection_signals(ids: Vec<String>) {
    let primary = ids.last().cloned();
    if *SELECTED_NODES.read() != ids {
        *SELECTED_NODES.write() = ids;
    }
    if *SELECTED_NODE.read() != primary {
        *SELECTED_NODE.write() = primary;
    }
}

pub fn select_node(node_id: Option<String>) {
    set_selection_signals(node_id.into_iter().collect());
}

pub fn selected_node() -> Option<String> {
    SELECTED_NODE()
}

pub fn selected_nodes() -> Vec<String> {
    SELECTED_NODES()
}

pub fn is_node_selected(node_id: &str) -> bool {
    SELECTED_NODES().iter().any(|id| id == node_id)
}

pub fn set_selected_nodes(ids: Vec<String>) {
    let mut seen = HashSet::new();
    let normalized = ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect();
    set_selection_signals(normalized);
}

pub fn toggle_node_selection(node_id: &str) {
    let mut ids = SELECTED_NODES();
    if let Some(index) = ids.iter().position(|id| id == node_id) {
        ids.remove(index);
    } else {
        ids.push(node_id.to_string());
    }
    set_selection_signals(ids);
}

pub fn add_nodes_to_selection(node_ids: &[String]) {
    let mut ids = SELECTED_NODES();
    for node_id in node_ids {
        if !ids.iter().any(|id| id == node_id) {
            ids.push(node_id.clone());
        }
    }
    set_selection_signals(ids);
}

pub fn select_all_nodes() {
    let Some(screen) = active_screen_snapshot() else {
        set_selection_signals(Vec::new());
        return;
    };
    let ids = match screen {
        Screen::TwoD { nodes, .. } => nodes
            .into_iter()
            .filter(|node| node.visible && node.id != "node.bg")
            .map(|node| node.id)
            .collect(),
        Screen::ThreeD { nodes, .. } => nodes
            .into_iter()
            .filter(|node| node.visible)
            .map(|node| node.id)
            .collect(),
    };
    set_selection_signals(ids);
}

pub fn copy_selection() -> bool {
    let Some(screen) = active_screen_snapshot() else {
        return false;
    };
    let selected = SELECTED_NODES();
    let count = match screen {
        Screen::TwoD { nodes, .. } => {
            let copies = selected
                .iter()
                .filter_map(|id| nodes.iter().find(|node| &node.id == id).cloned())
                .collect::<Vec<_>>();
            let count = copies.len();
            if !copies.is_empty() {
                CLIPBOARD.with(|slot| {
                    *slot.borrow_mut() = Some(CanvasClipboard::TwoD(copies));
                });
            }
            count
        }
        Screen::ThreeD { nodes, .. } => {
            let copies = selected
                .iter()
                .filter_map(|id| nodes.iter().find(|node| &node.id == id).cloned())
                .collect::<Vec<_>>();
            let count = copies.len();
            if !copies.is_empty() {
                CLIPBOARD.with(|slot| {
                    *slot.borrow_mut() = Some(CanvasClipboard::ThreeD(copies));
                });
            }
            count
        }
    };
    if count > 0 {
        PASTE_SERIAL.with(|serial| serial.set(0));
        set_notice(NoticeLevel::Info, format!("已复制 {count} 个节点"));
        true
    } else {
        set_notice(NoticeLevel::Warning, "没有可复制的节点");
        false
    }
}

pub fn cut_selection() -> Result<usize, String> {
    let Some(screen) = active_screen_snapshot() else {
        return Ok(0);
    };
    let screen_id = screen.id().to_string();
    let selected = SELECTED_NODES();
    if selected.is_empty() {
        set_notice(NoticeLevel::Warning, "没有可剪切的节点");
        return Ok(0);
    }
    let (unlocked_ids, locked_count, clipboard) = match screen {
        Screen::TwoD { ref nodes, .. } => {
            let unlocked = nodes
                .iter()
                .filter(|node| selected.contains(&node.id) && !node.locked)
                .cloned()
                .collect::<Vec<_>>();
            let locked_count = nodes
                .iter()
                .filter(|node| selected.contains(&node.id) && node.locked)
                .count();
            let ids: Vec<String> = unlocked.iter().map(|node| node.id.clone()).collect();
            (ids, locked_count, CanvasClipboard::TwoD(unlocked))
        }
        Screen::ThreeD { ref nodes, .. } => {
            let unlocked = nodes
                .iter()
                .filter(|node| selected.contains(&node.id) && !node.locked)
                .cloned()
                .collect::<Vec<_>>();
            let locked_count = nodes
                .iter()
                .filter(|node| selected.contains(&node.id) && node.locked)
                .count();
            let ids: Vec<String> = unlocked.iter().map(|node| node.id.clone()).collect();
            (ids, locked_count, CanvasClipboard::ThreeD(unlocked))
        }
    };
    if unlocked_ids.is_empty() {
        set_notice(NoticeLevel::Warning, "所选节点均已锁定，无法剪切");
        return Ok(0);
    }
    CLIPBOARD.with(|slot| {
        *slot.borrow_mut() = Some(clipboard);
    });
    PASTE_SERIAL.with(|serial| serial.set(0));
    let commands = unlocked_ids
        .iter()
        .map(|node_id| EditorCommand::DeleteNode {
            screen_id: screen_id.clone(),
            node_id: node_id.clone(),
        })
        .collect();
    execute_batch(commands).map_err(|error| error.to_string())?;
    let cut_locked = locked_count > 0;
    set_selection_signals(Vec::new());
    let count = unlocked_ids.len();
    if cut_locked {
        set_notice(
            NoticeLevel::Warning,
            format!("已剪切 {count} 个节点，{locked_count} 个锁定节点未处理"),
        );
    } else {
        set_notice(NoticeLevel::Info, format!("已剪切 {count} 个节点"));
    }
    Ok(count)
}

pub fn paste_clipboard() -> Result<usize, String> {
    let Some(screen) = active_screen_snapshot() else {
        return Ok(0);
    };
    let Some(clipboard) = CLIPBOARD.with(|slot| slot.borrow().clone()) else {
        set_notice(NoticeLevel::Warning, "剪贴板为空");
        return Ok(0);
    };
    let serial = PASTE_SERIAL.with(|value| value.get()).saturating_add(1);
    let screen_id = screen.id().to_string();
    let (commands, selected_ids) = match (screen, clipboard) {
        (Screen::TwoD { nodes, .. }, CanvasClipboard::TwoD(items)) => {
            if items.is_empty() {
                return Ok(0);
            }
            let offset = 24.0 * serial as f32;
            let max_x = items
                .iter()
                .map(|node| node.rect.right())
                .fold(f32::NEG_INFINITY, f32::max);
            let max_y = items
                .iter()
                .map(|node| node.rect.bottom())
                .fold(f32::NEG_INFINITY, f32::max);
            let dx = offset.min((1600.0 - max_x).max(0.0));
            let dy = offset.min((900.0 - max_y).max(0.0));
            let mut ids = nodes.iter().map(|node| node.id.clone()).collect::<Vec<_>>();
            let mut commands = Vec::with_capacity(items.len());
            let mut selected_ids = Vec::with_capacity(items.len());
            for item in items {
                let id = unique_node_id(&item.id, ids.iter().map(String::as_str))
                    .map_err(|error| error.to_string())?;
                ids.push(id.clone());
                let mut node = item;
                node.id = id.clone();
                node.name = format!("{} 副本", node.name);
                node.rect = Rect {
                    x: node.rect.x + dx,
                    y: node.rect.y + dy,
                    ..node.rect
                };
                selected_ids.push(id);
                commands.push(EditorCommand::Add2dNode {
                    screen_id: screen_id.clone(),
                    node,
                });
            }
            (commands, selected_ids)
        }
        (Screen::ThreeD { nodes, .. }, CanvasClipboard::ThreeD(items)) => {
            if items.is_empty() {
                return Ok(0);
            }
            let offset = 0.5 * serial as f32;
            let mut ids = nodes.iter().map(|node| node.id.clone()).collect::<Vec<_>>();
            let mut commands = Vec::with_capacity(items.len());
            let mut selected_ids = Vec::with_capacity(items.len());
            for item in items {
                let id = unique_node_id(&item.id, ids.iter().map(String::as_str))
                    .map_err(|error| error.to_string())?;
                ids.push(id.clone());
                let mut node = item;
                node.id = id.clone();
                node.name = format!("{} 副本", node.name);
                node.position[0] += offset;
                node.position[2] += offset;
                selected_ids.push(id);
                commands.push(EditorCommand::Add3dNode {
                    screen_id: screen_id.clone(),
                    node,
                });
            }
            (commands, selected_ids)
        }
        (Screen::TwoD { .. }, CanvasClipboard::ThreeD(_)) => {
            set_notice(
                NoticeLevel::Warning,
                "剪贴板中是 3D 设备，不能粘贴到 2D 画面",
            );
            return Ok(0);
        }
        (Screen::ThreeD { .. }, CanvasClipboard::TwoD(_)) => {
            set_notice(
                NoticeLevel::Warning,
                "剪贴板中是 2D 节点，不能粘贴到 3D 画面",
            );
            return Ok(0);
        }
    };
    execute_batch(commands).map_err(|error| error.to_string())?;
    PASTE_SERIAL.with(|value| value.set(serial));
    let count = selected_ids.len();
    set_selected_nodes(selected_ids);
    set_notice(NoticeLevel::Info, format!("已粘贴 {count} 个节点"));
    Ok(count)
}

pub fn duplicate_selection() -> Result<usize, String> {
    let (Some(screen_id), selected) = (active_screen(), SELECTED_NODES()) else {
        set_notice(NoticeLevel::Warning, "没有可复制的节点");
        return Ok(0);
    };
    if selected.is_empty() {
        set_notice(NoticeLevel::Warning, "没有可复制的节点");
        return Ok(0);
    }
    let Some(screen) = active_screen_snapshot() else {
        return Ok(0);
    };
    let unlocked = match screen {
        Screen::TwoD { nodes, .. } => nodes
            .iter()
            .filter(|node| selected.contains(&node.id) && !node.locked)
            .map(|node| node.id.clone())
            .collect::<Vec<_>>(),
        Screen::ThreeD { nodes, .. } => nodes
            .iter()
            .filter(|node| selected.contains(&node.id) && !node.locked)
            .map(|node| node.id.clone())
            .collect::<Vec<_>>(),
    };
    if unlocked.is_empty() {
        set_notice(NoticeLevel::Warning, "所选节点均已锁定，无法复制");
        return Ok(0);
    }
    let commands = unlocked
        .iter()
        .map(|node_id| EditorCommand::DuplicateNode {
            screen_id: screen_id.clone(),
            node_id: node_id.clone(),
        })
        .collect();
    let result = execute_batch(commands).map_err(|error| error.to_string())?;
    let count = result.affected_ids.len();
    set_selected_nodes(result.affected_ids);
    set_notice(NoticeLevel::Info, format!("已复制 {count} 个节点"));
    Ok(count)
}

pub fn delete_selection() -> Result<usize, String> {
    let (Some(screen_id), selected) = (active_screen(), SELECTED_NODES()) else {
        return Ok(0);
    };
    if selected.is_empty() {
        set_notice(NoticeLevel::Warning, "没有可删除的节点");
        return Ok(0);
    }
    let Some(screen) = active_screen_snapshot() else {
        return Ok(0);
    };
    let (deletable, locked) = match screen {
        Screen::TwoD { nodes, .. } => nodes
            .iter()
            .filter(|node| selected.contains(&node.id))
            .fold(
                (Vec::new(), Vec::new()),
                |(mut deletable, mut locked), node| {
                    if node.locked {
                        locked.push(node.id.clone());
                    } else {
                        deletable.push(node.id.clone());
                    }
                    (deletable, locked)
                },
            ),
        Screen::ThreeD { nodes, .. } => nodes
            .iter()
            .filter(|node| selected.contains(&node.id))
            .fold(
                (Vec::new(), Vec::new()),
                |(mut deletable, mut locked), node| {
                    if node.locked {
                        locked.push(node.id.clone());
                    } else {
                        deletable.push(node.id.clone());
                    }
                    (deletable, locked)
                },
            ),
    };
    if deletable.is_empty() {
        set_notice(NoticeLevel::Warning, "所选节点均已锁定，无法删除");
        return Ok(0);
    }
    let locked_count = locked.len();
    let commands = deletable
        .iter()
        .map(|node_id| EditorCommand::DeleteNode {
            screen_id: screen_id.clone(),
            node_id: node_id.clone(),
        })
        .collect();
    execute_batch(commands).map_err(|error| error.to_string())?;
    set_selected_nodes(locked);
    let count = deletable.len();
    if locked_count > 0 {
        set_notice(
            NoticeLevel::Warning,
            format!("已删除 {count} 个节点，{locked_count} 个锁定节点未处理"),
        );
    } else {
        set_notice(NoticeLevel::Info, format!("已删除 {count} 个节点"));
    }
    Ok(count)
}

pub fn select_screen(screen_id: &str) {
    if project()
        .screens
        .iter()
        .any(|screen| screen.id() == screen_id)
    {
        *ACTIVE_SCREEN.write() = Some(screen_id.to_string());
        set_selection_signals(Vec::new());
        clear_scene_labels_3d();
    }
}

pub fn active_screen() -> Option<String> {
    ACTIVE_SCREEN()
}

pub fn workspace_mode() -> WorkspaceMode {
    WORKSPACE_MODE()
}

pub fn set_workspace_mode(mode: WorkspaceMode) {
    *WORKSPACE_MODE.write() = mode;
}

pub fn runtime_only() -> bool {
    RUNTIME_ONLY()
}

pub fn guide_open() -> bool {
    GUIDE_OPEN()
}

pub fn set_guide_open(open: bool) {
    *GUIDE_OPEN.write() = open;
}

pub fn toggle_guide() {
    let open = !guide_open();
    set_guide_open(open);
}

pub fn guide_section() -> GuideSection {
    GUIDE_SECTION()
}

pub fn set_guide_section(section: GuideSection) {
    if *GUIDE_SECTION.read() != section {
        *GUIDE_SECTION.write() = section;
    }
}

pub fn cycle_guide_section(direction: isize) {
    set_guide_section(guide_section().next(direction));
}

pub fn guide_scroll_request() -> GuideScrollRequest {
    GUIDE_SCROLL_REQUEST()
}

pub fn request_guide_scroll(delta: f64) {
    if !delta.is_finite() || delta.abs() < f64::EPSILON {
        return;
    }
    let mut request = GUIDE_SCROLL_REQUEST.write();
    request.revision = request.revision.wrapping_add(1);
    request.delta = delta;
}

pub fn text_entry_focused() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        NATIVE_TEXT_ENTRY_FOCUSED.load(Ordering::Relaxed)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let Some(active) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element())
        else {
            return false;
        };
        let tag = active.tag_name().to_ascii_uppercase();
        if matches!(tag.as_str(), "INPUT" | "TEXTAREA" | "SELECT") {
            return true;
        }
        js_sys::Reflect::get(
            active.as_ref(),
            &wasm_bindgen::JsValue::from_str("isContentEditable"),
        )
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    }
}

pub fn set_text_entry_focused(focused: bool) {
    #[cfg(not(target_arch = "wasm32"))]
    NATIVE_TEXT_ENTRY_FOCUSED.store(focused, Ordering::Relaxed);
    #[cfg(target_arch = "wasm32")]
    let _ = focused;
}

pub fn ui_captures_pointer() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        NATIVE_UI_CAPTURES_POINTER.load(Ordering::Relaxed)
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

pub fn set_ui_captures_pointer(captured: bool) {
    #[cfg(not(target_arch = "wasm32"))]
    NATIVE_UI_CAPTURES_POINTER.store(captured, Ordering::Relaxed);
    #[cfg(target_arch = "wasm32")]
    let _ = captured;
}

pub fn pointer_on_native_canvas() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        NATIVE_POINTER_ON_CANVAS.load(Ordering::Relaxed)
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

pub fn set_pointer_on_native_canvas(inside: bool) {
    #[cfg(not(target_arch = "wasm32"))]
    NATIVE_POINTER_ON_CANVAS.store(inside, Ordering::Relaxed);
    #[cfg(target_arch = "wasm32")]
    let _ = inside;
}

pub fn install_native_haptic(handler: fn()) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = NATIVE_HAPTIC_HANDLER.set(handler);
    #[cfg(target_arch = "wasm32")]
    let _ = handler;
}

pub fn native_haptic() {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(handler) = NATIVE_HAPTIC_HANDLER.get() {
        handler();
    }
}

pub fn gamepad_status() -> GamepadStatus {
    GAMEPAD_STATUS()
}

pub fn set_gamepad_status(connected: bool, id: impl Into<String>) {
    let next = GamepadStatus {
        connected,
        id: id.into(),
    };
    if *GAMEPAD_STATUS.read() != next {
        *GAMEPAD_STATUS.write() = next;
    }
}

pub fn controller_tuning_hud() -> Option<ControllerTuningHud> {
    CONTROLLER_TUNING_HUD()
}

pub fn set_controller_tuning_hud(hud: Option<ControllerTuningHud>) {
    if *CONTROLLER_TUNING_HUD.read() != hud {
        *CONTROLLER_TUNING_HUD.write() = hud;
    }
}

pub fn controller_tuning_active() -> bool {
    controller_tuning_hud().is_some()
}

pub fn gamepad_menu_hud() -> Option<GamepadMenuHud> {
    GAMEPAD_MENU_HUD()
}

pub fn set_gamepad_menu_hud(hud: Option<GamepadMenuHud>) {
    if *GAMEPAD_MENU_HUD.read() != hud {
        *GAMEPAD_MENU_HUD.write() = hud;
    }
}

pub fn gamepad_menu_open() -> bool {
    gamepad_menu_hud().is_some()
}

fn runtime_mode_requested() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("MOX_HMI_RUNTIME_ONLY")
            .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "on" | "yes"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let Some(window) = web_sys::window() else {
            return false;
        };
        let runtime_marker = js_sys::Reflect::get(
            window.as_ref(),
            &wasm_bindgen::JsValue::from_str("__MOX_HMI_RUNTIME_ONLY__"),
        )
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
        if runtime_marker {
            return true;
        }
        let search = window.location().search().unwrap_or_default();
        search
            .trim_start_matches('?')
            .split('&')
            .any(|part| matches!(part, "runtime=1" | "runtime=true" | "mode=run"))
    }
}

pub fn tick_tags(dt_ms: u64) {
    let changed = with_session(|session| session.tick_tags(dt_ms)).unwrap_or(false);
    if changed {
        *TAG_REVISION.write() += 1;
    }
}

pub fn write_tag(tag_id: &str, value: TagValue) -> Result<(), HmiError> {
    let result = with_session(|session| session.write_tag(tag_id, value))
        .map_err(HmiError::InvalidCommand)?;
    if result.is_ok() {
        *TAG_REVISION.write() += 1;
    }
    result
}

pub fn invoke_button_action(action: &ButtonAction) -> Result<(), HmiError> {
    match action {
        ButtonAction::ToggleTag { tag_id } => {
            let value = tag_snapshot()
                .get(tag_id)
                .cloned()
                .ok_or_else(|| HmiError::UnknownTag(tag_id.clone()))?;
            let next = match value {
                TagValue::Bool(value) => TagValue::Bool(!value),
                _ => {
                    return Err(HmiError::InvalidCommand(
                        "ToggleTag 只支持 Bool Tag".to_string(),
                    ));
                }
            };
            write_tag(tag_id, next)
        }
        ButtonAction::SetTag { tag_id, value } => write_tag(tag_id, value.clone()),
    }
}

pub fn persist_now() -> Result<(), String> {
    let json = export_json()?;
    match storage::save_raw(&json) {
        Ok(()) => {
            *SAVE_STATUS.write() = SaveStatus::Saved;
            Ok(())
        }
        Err(error) => {
            *SAVE_STATUS.write() = SaveStatus::Failed;
            set_notice(NoticeLevel::Error, error.clone());
            Err(error)
        }
    }
}

pub fn export_json() -> Result<String, String> {
    to_json(&project()).map_err(|error| error.to_string())
}

pub fn export_download_name() -> String {
    export_filename(&project())
}

pub fn import_json(raw: &str) -> Result<(), String> {
    let project = from_json(raw).map_err(|error| error.to_string())?;
    let startup = project.startup_screen.clone();
    let replacement = with_session(|session| session.replace_project(project))?;
    replacement.map_err(|errors| {
        errors
            .into_iter()
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let revision = with_session(|session| session.document_revision()).unwrap_or(2);
    *DOCUMENT_REVISION.write() = revision;
    *TAG_REVISION.write() += 1;
    *ACTIVE_SCREEN.write() = Some(startup);
    set_selection_signals(Vec::new());
    *SAVE_STATUS.write() = SaveStatus::Pending;
    set_notice(NoticeLevel::Info, "工程导入成功");
    Ok(())
}

pub fn mark_engine_ready() {
    *ENGINE_STATUS.write() = EngineStatus::Ready;
    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&wasm_bindgen::JsValue::from_str("MOX HMI runtime ready"));
}

pub fn mark_engine_failed(message: impl Into<String>) {
    *ENGINE_STATUS.write() = EngineStatus::Failed(message.into());
}

pub fn set_notice(level: NoticeLevel, message: impl Into<String>) {
    NOTICE.with(|slot| {
        *slot.borrow_mut() = Some(Notice {
            level,
            message: message.into(),
        });
    });
    *NOTICE_REVISION.write() += 1;
}

pub fn notice() -> Option<Notice> {
    let _ = NOTICE_REVISION();
    NOTICE.with(|slot| slot.borrow().clone())
}

pub fn clear_notice() {
    NOTICE.with(|slot| {
        *slot.borrow_mut() = None;
    });
    *NOTICE_REVISION.write() += 1;
}

pub fn announce_field_error() {
    *FIELD_ERROR_ANNOUNCEMENT.write() = Some("请输入有效数值".to_string());
}

pub fn clear_field_error_announcement() {
    *FIELD_ERROR_ANNOUNCEMENT.write() = None;
}

pub fn view_center_2d() -> [f32; 2] {
    VIEW_CENTER_2D.with(Cell::get)
}

pub fn set_view_center_2d(center: [f32; 2]) {
    VIEW_CENTER_2D.with(|cell| cell.set(center));
}

pub fn zoom_2d() -> f32 {
    ZOOM_2D.with(Cell::get)
}

pub fn view_focus_3d() -> [f32; 3] {
    VIEW_FOCUS_3D.with(Cell::get)
}

pub fn set_view_focus_3d(focus: [f32; 3]) {
    VIEW_FOCUS_3D.with(|cell| cell.set(focus));
}

pub fn set_zoom_2d(zoom: f32) {
    ZOOM_2D.with(|cell| cell.set(zoom));
}

pub fn camera_reset_revision() -> u32 {
    *CAMERA_RESET_REVISION.read()
}

pub fn reset_camera_view() {
    *CAMERA_RESET_REVISION.write() += 1;
}

pub fn reset_camera_3d() {
    reset_camera_view();
}

pub fn align_selected_3d(display_axis: usize) -> Result<usize, String> {
    if display_axis > 2 {
        return Err("无效的 3D 中心面".to_string());
    }
    let project = project();
    let screen_id = active_screen().unwrap_or_else(|| project.startup_screen.clone());
    let Some(Screen::ThreeD { nodes, .. }) = project.screen(&screen_id) else {
        return Err("当前画面不是 3D 画面".to_string());
    };
    let selected = selected_nodes();
    let unlocked = selected
        .iter()
        .filter_map(|id| nodes.iter().find(|node| &node.id == id))
        .filter(|node| !node.locked && node.visible)
        .collect::<Vec<_>>();
    if unlocked.is_empty() {
        return Err("请先选择至少一个未锁定的 3D 设备".to_string());
    }
    let scene_axis = display_axis_to_scene(display_axis);
    let center = unlocked
        .iter()
        .map(|node| node.position[scene_axis])
        .sum::<f32>()
        / unlocked.len() as f32;
    if center.abs() <= f32::EPSILON {
        return Ok(0);
    }
    let commands = unlocked
        .iter()
        .map(|node| {
            let mut updated = (*node).clone();
            updated.position[scene_axis] -= center;
            EditorCommand::Update3dNode {
                screen_id: screen_id.clone(),
                node: updated,
            }
        })
        .collect::<Vec<_>>();
    execute_batch(commands).map_err(|error| error.to_string())?;
    let axis = ["X", "Y", "Z"][display_axis];
    set_notice(
        NoticeLevel::Info,
        format!("已将 {} 个设备对齐到 {axis}=0 中心面", unlocked.len()),
    );
    Ok(unlocked.len())
}

pub fn set_scene_labels_3d(labels: Vec<SceneLabel3D>) {
    if *VIEW_LABELS_3D.read() != labels {
        *VIEW_LABELS_3D.write() = labels;
    }
}

pub fn clear_scene_labels_3d() {
    if !VIEW_LABELS_3D.read().is_empty() {
        *VIEW_LABELS_3D.write() = Vec::new();
    }
}

pub fn axis_gizmo_3d() -> Option<AxisGizmo3D> {
    *AXIS_GIZMO_3D.read()
}

pub fn set_axis_gizmo_3d(gizmo: Option<AxisGizmo3D>) {
    if *AXIS_GIZMO_3D.read() != gizmo {
        *AXIS_GIZMO_3D.write() = gizmo;
    }
}

pub fn create_screen(three_d: bool) -> Result<String, String> {
    let project = project();
    let screen_number = project
        .screens
        .iter()
        .filter(|screen| {
            if three_d {
                matches!(screen, Screen::ThreeD { .. })
            } else {
                matches!(screen, Screen::TwoD { .. })
            }
        })
        .count()
        + 1;
    let screen_id = next_screen_id(&project);
    let screen = if three_d {
        Screen::ThreeD {
            id: screen_id.clone(),
            name: format!("3D 机房 {screen_number}"),
            background: HexColor("#071217".to_string()),
            nodes: Vec::new(),
        }
    } else {
        Screen::TwoD {
            id: screen_id.clone(),
            name: format!("新画面 {screen_number}"),
            background: HexColor("#071217".to_string()),
            nodes: vec![UiNode {
                id: next_palette_node_id(&project, "new.bg"),
                name: "画面背景".to_string(),
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1600.0,
                    height: 900.0,
                },
                rotation_deg: 0.0,
                z: 0,
                visible: true,
                locked: true,
                widget: Widget2D::Panel {
                    fill: HexColor("#071217".to_string()),
                    stroke: HexColor("#1f3540".to_string()),
                    stroke_width: 1.0,
                },
                bindings: Vec::new(),
            }],
        }
    };
    execute(EditorCommand::AddScreen { screen }).map_err(|error| error.to_string())?;
    select_screen(&screen_id);
    Ok(screen_id)
}

pub fn add_palette_node(kind: &str) -> Result<String, String> {
    let project = project();
    let Some(screen) = active_screen_snapshot() else {
        return Err("请先选择画面".to_string());
    };
    match screen {
        Screen::TwoD { id, nodes, .. } => add_2d_palette_node(&project, &id, &nodes, kind),
        Screen::ThreeD { id, nodes, .. } => add_3d_palette_node(&project, &id, &nodes, kind),
    }
}

fn add_2d_palette_node(
    project: &ProjectDocument,
    screen_id: &str,
    nodes: &[UiNode],
    kind: &str,
) -> Result<String, String> {
    let (width, height, display_name) = match kind {
        "panel" => (420.0, 260.0, "新面板"),
        "label" => (260.0, 48.0, "新文本"),
        "value-card" => (240.0, 112.0, "新数值卡"),
        "status-lamp" => (230.0, 72.0, "新状态灯"),
        "bar-gauge" => (260.0, 96.0, "新仪表条"),
        "button" => (240.0, 72.0, "新按钮"),
        "pipe" => (260.0, 24.0, "新管线"),
        _ => (180.0, 120.0, "新设备"),
    };
    let placement = next_2d_placement(nodes, view_center_2d(), width, height);

    let widget = match kind {
        "panel" => Widget2D::Panel {
            fill: HexColor("#101e27".to_string()),
            stroke: HexColor("#2a4b58".to_string()),
            stroke_width: 1.0,
        },
        "label" => Widget2D::Label {
            text: "新文本".to_string(),
            font_size: 24.0,
            color: HexColor("#e8f2f4".to_string()),
            align: TextAlign::Left,
        },
        "value-card" => Widget2D::ValueCard {
            label: "新数值卡".to_string(),
        },
        "status-lamp" => Widget2D::StatusLamp {
            label: "新状态灯".to_string(),
        },
        "bar-gauge" => Widget2D::BarGauge {
            min: 0.0,
            max: 100.0,
            direction: BarDirection::Horizontal,
        },
        "button" => {
            let Some(tag) = project
                .tags
                .iter()
                .find(|tag| tag.writable && tag.value_type == TagValueType::Bool)
            else {
                return Err("请先创建一个可写 Bool Tag 再添加按钮".to_string());
            };
            Widget2D::Button {
                label: "新按钮".to_string(),
                action: ButtonAction::ToggleTag {
                    tag_id: tag.id.clone(),
                },
            }
        }
        "pipe" => Widget2D::Pipe {
            points: vec![[0.0, height * 0.5], [width, height * 0.5]],
            thickness: 8.0,
            color: HexColor("#2d6d77".to_string()),
        },
        _ => Widget2D::EquipmentSymbol {
            equipment: EquipmentKind::Fan,
            label: "新设备".to_string(),
        },
    };
    let id = next_palette_node_id(project, kind);
    let z = nodes.iter().map(|node| node.z).max().unwrap_or(0) + 1;
    let node = UiNode {
        id: id.clone(),
        name: display_name.into(),
        rect: placement,
        rotation_deg: 0.0,
        z,
        visible: true,
        locked: false,
        widget,
        bindings: Vec::new(),
    };
    execute(EditorCommand::Add2dNode {
        screen_id: screen_id.to_string(),
        node,
    })
    .map_err(|error| error.to_string())?;
    select_node(Some(id.clone()));
    Ok(id)
}

fn add_3d_palette_node(
    project: &ProjectDocument,
    screen_id: &str,
    nodes: &[DeviceNode],
    kind: &str,
) -> Result<String, String> {
    let (display_name, y, model) = match kind {
        "ahu" => (
            "AHU-新增",
            1.4,
            DeviceModel::Ahu {
                size: [4.0, 2.8, 2.2],
                color: HexColor("#23434d".to_string()),
            },
        ),
        "fan" => (
            "风机-新增",
            1.0,
            DeviceModel::Fan {
                size: [1.8, 1.8, 1.8],
                color: HexColor("#31505a".to_string()),
            },
        ),
        "pump" => (
            "水泵-新增",
            0.55,
            DeviceModel::Pump {
                size: [1.4, 1.1, 1.4],
                color: HexColor("#31505a".to_string()),
            },
        ),
        "duct" => (
            "风管-新增",
            2.0,
            DeviceModel::Duct {
                length: 3.0,
                diameter: 0.55,
                color: HexColor("#24434f".to_string()),
            },
        ),
        "pipe-3d" => (
            "水管-新增",
            0.5,
            DeviceModel::Pipe {
                length: 3.0,
                diameter: 0.18,
                color: HexColor("#2d6d77".to_string()),
            },
        ),
        _ => (
            "阀门-新增",
            1.0,
            DeviceModel::Valve {
                size: 0.7,
                color: HexColor("#31505a".to_string()),
            },
        ),
    };
    let id = next_palette_node_id(project, kind);
    let focus = view_focus_3d();
    let radius = device_footprint_radius(&model, [1.0, 1.0, 1.0]);
    let node = DeviceNode {
        id: id.clone(),
        name: display_name.into(),
        position: next_3d_position(nodes, focus, y, radius),
        rotation_deg: [0.0, 0.0, 0.0],
        scale: [1.0, 1.0, 1.0],
        visible: true,
        locked: false,
        label: Some(display_name.into()),
        model,
        bindings: Vec::new(),
    };
    execute(EditorCommand::Add3dNode {
        screen_id: screen_id.to_string(),
        node,
    })
    .map_err(|error| error.to_string())?;
    select_node(Some(id.clone()));
    Ok(id)
}

const CANVAS_WIDTH: f32 = 1600.0;
const CANVAS_HEIGHT: f32 = 900.0;
const CANVAS_GAP: f32 = 8.0;

fn next_2d_placement(nodes: &[UiNode], center: [f32; 2], width: f32, height: f32) -> Rect {
    let max_x = (CANVAS_WIDTH - width).max(0.0);
    let max_y = (CANVAS_HEIGHT - height).max(0.0);
    let start_x = (center[0] - width * 0.5).clamp(0.0, max_x);
    let start_y = (center[1] - height * 0.5).clamp(0.0, max_y);
    for step in 0..=400 {
        let angle = step as f32 * 2.399_963_2;
        let radius = if step == 0 {
            0.0
        } else {
            28.0 * (step as f32).sqrt()
        };
        let candidate = Rect {
            x: (start_x + radius * angle.cos()).clamp(0.0, max_x),
            y: (start_y + radius * angle.sin()).clamp(0.0, max_y),
            width,
            height,
        };
        if !overlaps_nodes_2d(candidate, nodes) {
            return candidate;
        }
    }
    Rect {
        x: start_x,
        y: start_y,
        width,
        height,
    }
}

fn overlaps_nodes_2d(candidate: Rect, nodes: &[UiNode]) -> bool {
    nodes.iter().any(|node| {
        if !node.visible {
            return false;
        }
        let other = node.rect;
        candidate.x < other.right() + CANVAS_GAP
            && candidate.right() + CANVAS_GAP > other.x
            && candidate.y < other.bottom() + CANVAS_GAP
            && candidate.bottom() + CANVAS_GAP > other.y
    })
}

const ROOM_HALF_EXTENT: f32 = 14.0;
const PLACEMENT_GAP: f32 = 0.45;

fn next_3d_position(nodes: &[DeviceNode], focus: [f32; 3], y: f32, new_radius: f32) -> [f32; 3] {
    let step_size = (new_radius + PLACEMENT_GAP).max(0.6);
    for step in 0..=160 {
        let angle = step as f32 * 2.399_963_2;
        let radius = if step == 0 {
            0.0
        } else {
            step_size * (step as f32).sqrt()
        };
        let candidate = [
            (focus[0] + radius * angle.cos()).clamp(-ROOM_HALF_EXTENT, ROOM_HALF_EXTENT),
            y,
            (focus[2] + radius * angle.sin()).clamp(-ROOM_HALF_EXTENT, ROOM_HALF_EXTENT),
        ];
        let occupied = nodes.iter().any(|node| {
            if !node.visible {
                return false;
            }
            let dx = node.position[0] - candidate[0];
            let dz = node.position[2] - candidate[2];
            let clearance =
                new_radius + device_footprint_radius(&node.model, node.scale) + PLACEMENT_GAP;
            dx * dx + dz * dz < clearance * clearance
        });
        if !occupied {
            return candidate;
        }
    }
    [focus[0], y, focus[2]]
}

fn device_footprint_radius(model: &DeviceModel, scale: [f32; 3]) -> f32 {
    let sx = scale[0].abs().max(0.01);
    let sy = scale[1].abs().max(0.01);
    let sz = scale[2].abs().max(0.01);
    let (width, depth) = match model {
        DeviceModel::Ahu { size, .. }
        | DeviceModel::Fan { size, .. }
        | DeviceModel::Pump { size, .. } => (size[0].abs() * sx, size[2].abs() * sz),
        DeviceModel::Duct {
            length, diameter, ..
        }
        | DeviceModel::Pipe {
            length, diameter, ..
        } => (length.abs() * sx, diameter.abs() * sy.max(sz)),
        DeviceModel::Valve { size, .. } => (size.abs() * sx, size.abs() * sz),
        DeviceModel::Glb { .. } => (2.4 * sx, 2.4 * sz),
    };
    0.5 * (width * width + depth * depth).sqrt().max(0.15)
}

fn next_palette_node_id(project: &ProjectDocument, prefix: &str) -> String {
    let used = all_node_ids(project);
    for index in 1..=10_000 {
        let candidate = format!("node.{prefix}.{index}");
        if !used.contains(&candidate) {
            return candidate;
        }
    }
    format!("node.{prefix}.{}", now_ms() as u64)
}

fn next_screen_id(project: &ProjectDocument) -> String {
    for index in 1..=10_000 {
        let candidate = format!("screen.new.{index}");
        if project
            .screens
            .iter()
            .all(|screen| screen.id() != candidate)
        {
            return candidate;
        }
    }
    format!("screen.new.{}", now_ms() as u64)
}

pub fn project_contains_node(project: &ProjectDocument, node_id: &str) -> bool {
    project.screens.iter().any(|screen| match screen {
        Screen::TwoD { nodes, .. } => nodes.iter().any(|node| node.id == node_id),
        Screen::ThreeD { nodes, .. } => nodes.iter().any(|node| node.id == node_id),
    })
}

pub fn all_node_ids(project: &ProjectDocument) -> HashSet<String> {
    let mut ids = HashSet::new();
    for screen in &project.screens {
        match screen {
            Screen::TwoD { nodes, .. } => ids.extend(nodes.iter().map(|node| node.id.clone())),
            Screen::ThreeD { nodes, .. } => ids.extend(nodes.iter().map(|node| node.id.clone())),
        }
    }
    ids
}

pub fn active_screen_snapshot() -> Option<Screen> {
    let project = project();
    let active = active_screen().or_else(|| Some(project.startup_screen.clone()))?;
    project.screen(&active).cloned()
}

pub fn shutdown_for_tests() {
    SESSION.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

#[cfg(test)]
mod tests {
    use super::{
        GamepadStatus, PLACEMENT_GAP, active_screen, device_footprint_radius, initialize,
        next_2d_placement, next_3d_position, overlaps_nodes_2d, project, shutdown_for_tests,
    };
    use hmi_core::{DeviceModel, DeviceNode, HexColor, Rect, UiNode, Widget2D};

    #[test]
    fn initializes_outside_browser_runtime() {
        shutdown_for_tests();
        initialize().expect("native bridge should initialize");
        assert!(!project().screens.is_empty());
        assert!(active_screen().is_some());
        shutdown_for_tests();
    }

    #[test]
    fn recognizes_dualsense_across_platform_names() {
        for id in [
            "DualSense Wireless Controller",
            "PS5 Controller",
            "PlayStation 5 Controller",
        ] {
            assert!(
                GamepadStatus {
                    connected: true,
                    id: id.to_string(),
                }
                .is_dualsense(),
                "{id}"
            );
        }
        assert!(
            !GamepadStatus {
                connected: true,
                id: "Xbox Controller".to_string(),
            }
            .is_dualsense()
        );
    }

    fn panel(id: &str, x: f32, y: f32) -> UiNode {
        UiNode {
            id: id.to_string(),
            name: id.to_string(),
            rect: Rect {
                x,
                y,
                width: 420.0,
                height: 260.0,
            },
            rotation_deg: 0.0,
            z: 1,
            visible: true,
            locked: false,
            widget: Widget2D::Panel {
                fill: HexColor("#101e27".to_string()),
                stroke: HexColor("#2a4b58".to_string()),
                stroke_width: 1.0,
            },
            bindings: Vec::new(),
        }
    }

    fn ahu(id: &str, x: f32, z: f32) -> DeviceNode {
        DeviceNode {
            id: id.to_string(),
            name: id.to_string(),
            position: [x, 1.4, z],
            rotation_deg: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            visible: true,
            locked: false,
            label: None,
            model: DeviceModel::Ahu {
                size: [4.0, 2.8, 2.2],
                color: HexColor("#23434d".to_string()),
            },
            bindings: Vec::new(),
        }
    }

    #[test]
    fn new_2d_nodes_are_staggered_instead_of_overlapping() {
        let mut nodes = vec![panel("node.panel.0", 500.0, 300.0)];
        for index in 1..6 {
            let placement = next_2d_placement(&nodes, [800.0, 450.0], 420.0, 260.0);
            assert!(
                !overlaps_nodes_2d(placement, &nodes),
                "new panel {index} overlaps: {placement:?}"
            );
            assert!(placement.right() <= 1600.0);
            assert!(placement.bottom() <= 900.0);
            nodes.push(panel(
                &format!("node.panel.{index}"),
                placement.x,
                placement.y,
            ));
        }
    }

    #[test]
    fn new_3d_devices_clear_existing_footprints() {
        let model = DeviceModel::Ahu {
            size: [4.0, 2.8, 2.2],
            color: HexColor("#23434d".to_string()),
        };
        let radius = device_footprint_radius(&model, [1.0, 1.0, 1.0]);
        let mut nodes = vec![ahu("node.3d.ahu.0", 0.0, 0.0)];
        for index in 1..5 {
            let position = next_3d_position(&nodes, [0.0, 1.4, 0.0], 1.4, radius);
            for existing in &nodes {
                let dx = existing.position[0] - position[0];
                let dz = existing.position[2] - position[2];
                let clearance = radius
                    + device_footprint_radius(&existing.model, existing.scale)
                    + PLACEMENT_GAP;
                assert!(
                    dx * dx + dz * dz >= clearance * clearance - 1e-4,
                    "new AHU {index} overlaps {}: {position:?}",
                    existing.id
                );
            }
            nodes.push(ahu(
                &format!("node.3d.ahu.{index}"),
                position[0],
                position[2],
            ));
        }
    }
}
