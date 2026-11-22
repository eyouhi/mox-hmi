# MOX HMI Studio BAS MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付可在 Chromium 120+ 浏览器中编辑、保存、导入导出并运行车站 BAS 2D/3D 画面的 WebGPU HMI MVP。

**Architecture:** `crates/hmi-core` 持有全部领域数据、校验、Tag 仿真、绑定解析、命令与撤销；`apps/studio` 用 Dioxus 0.7.10 管 DOM 外壳，用 Bevy 0.20.1 管同一个 Canvas 内的 2D/3D 场景。两者通过单线程 `EditorSession`、少量 `GlobalSignal` 和 revision 双向同步，工程包不复制到 Bevy。

**Tech Stack:** Rust 1.100.0、edition 2024、Bevy 0.20.1（WebGPU）、Dioxus 0.7.10（Web）、Serde、thiserror、web-sys、Playwright。

**Spec:** `docs/superpowers/specs/2026-11-20-mox-hmi-bas-mvp-design.md`

## Global Constraints

- 固定版本：Bevy `0.20.1`、Dioxus `0.7.10`、Rust `1.100.0`。
- 只支持 WebGPU，不实现 WebGL2 回退；目标 Chrome/Edge 120+。
- 生产环境必须通过 HTTPS 或 localhost 部署，CSP 至少允许 `wasm-unsafe-eval`。
- 全 Rust Web 前端，不引入 Node/React 运行时。
- 同一 WASM 进程、同一 `#bevy-canvas`；Dioxus 管 DOM，Bevy 管 2D/3D，不使用 iframe 或 bevy_egui。
- 2D 与 3D 是两张独立画面，不混排。
- 工程 JSON 固定 `schema_version = 1`；所有字段和枚举值使用 `snake_case`；导入拒绝未知字段。
- 设计空间固定 `1600 × 900`，原点左上角，Y 轴向下；Bevy 世界坐标中心为原点，Y 轴向上。
- Tag 仿真固定 4 Hz（250 ms）；运行态写入不进入工程 JSON、不进撤销栈、不触发自动保存。
- localStorage key 固定为 `mox-hmi.project.v1`；自动保存延迟 500 ms；导入上限 5 MB。
- 单个 2D 画面最多 200 个节点，3D 画面最多 80 个设备节点，工程最多 300 个 Tag。
- 只初始化一个 tracing subscriber；使用 Bevy `LogPlugin` 作为统一日志入口，不初始化 Dioxus logger。
- 中文为第一语言；界面、提示、测试选择器文案使用规格中的中文术语。
- 未经用户明确要求，不执行 `git commit`、不创建分支；每个任务以检查点结束。
- 所有命令从仓库根目录 `/Users/wuliwei/Projects/mox-hmi` 执行。

## File Map

```text
Cargo.toml                                  # workspace、公共版本、release profile
crates/hmi-core/Cargo.toml
crates/hmi-core/src/lib.rs
crates/hmi-core/src/model.rs                # Project/Screen/Node/Widget/Device/Binding
crates/hmi-core/src/tags.rs                 # TagDefinition/TagValue/TagSnapshot
crates/hmi-core/src/validation.rs           # 全部导入与结构校验
crates/hmi-core/src/demo.rs                 # 内置 BAS 示例工程
crates/hmi-core/src/simulation.rs           # 确定性 Tag 仿真
crates/hmi-core/src/bindings.rs             # Binding -> VisualPatch
crates/hmi-core/src/commands.rs             # EditorCommand/EditorSession/Undo
crates/hmi-core/src/persistence.rs          # JSON、schema、文件名、导入限制
crates/hmi-core/src/geometry.rs             # 设计坐标、命中测试、缩放手柄
apps/studio/Cargo.toml
apps/studio/Dioxus.toml
apps/studio/assets/main.css
apps/studio/public/assets/fonts/             # OFL 中文字体子集与许可证
apps/studio/src/main.rs                     # Dioxus 入口 + panic hook
apps/studio/src/app.rs                      # WebGPU 预检、Bevy 启动、全局快捷键
apps/studio/src/bridge/mod.rs               # GlobalSignal、Session 单例、通知
apps/studio/src/bridge/storage.rs           # localStorage 适配器
apps/studio/src/ui/mod.rs
apps/studio/src/ui/topbar.rs
apps/studio/src/ui/left_panel.rs
apps/studio/src/ui/inspector.rs
apps/studio/src/ui/runtime.rs
apps/studio/src/ui/device_details.rs
apps/studio/src/ui/status_bar.rs
apps/studio/src/scene/mod.rs
apps/studio/src/scene/plugin.rs
apps/studio/src/scene/two_d.rs
apps/studio/src/scene/two_d_edit.rs
apps/studio/src/scene/three_d.rs
apps/studio/src/scene/orbit.rs
apps/studio/src/scene/assets.rs
scripts/build_font_subset.py              # 从 UI 源码重建 OFL 中文字体子集
scripts/e2e.py
scripts/verify.sh
README.md
```

### Task 1: Workspace and quality gate

**Files:**
- Modify: `Cargo.toml`
- Modify: `.gitignore`
- Create: `crates/hmi-core/Cargo.toml`
- Create: `crates/hmi-core/src/lib.rs`
- Create: `crates/hmi-core/tests/smoke.rs`
- Delete: `src/main.rs`

**Interfaces:**
- Produces: `hmi_core::SCHEMA_VERSION: u32 = 1`
- Produces: workspace command `cargo test -p hmi-core`

- [ ] **Step 1: Write the failing smoke test**

`crates/hmi-core/tests/smoke.rs`:

```rust
#[test]
fn schema_version_is_one() {
    assert_eq!(hmi_core::SCHEMA_VERSION, 1);
}
```

- [ ] **Step 2: Create the workspace and run the test to verify it fails**

`Cargo.toml`:

```toml
[workspace]
members = ["crates/hmi-core"]
resolver = "3"

[workspace.package]
version = "0.1.0"
edition = "2024"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"

[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3

[profile.release]
opt-level = "z"
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"
```

`crates/hmi-core/Cargo.toml`:

```toml
[package]
name = "hmi-core"
version.workspace = true
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
```

`crates/hmi-core/src/lib.rs` starts empty, and remove the root `src/main.rs`.

```bash
cargo test -p hmi-core --test smoke
```

Expected: FAIL because `SCHEMA_VERSION` is not defined.

- [ ] **Step 3: Add the minimal constant**

`crates/hmi-core/src/lib.rs`:

```rust
pub const SCHEMA_VERSION: u32 = 1;
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cargo test -p hmi-core --test smoke
```

Expected: PASS.

- [ ] **Step 5: Lock ignore rules and check the workspace**

`.gitignore`:

```gitignore
/target
target/
__pycache__/
*.pyc
.DS_Store
/tmp/
```

```bash
cargo fmt --check
cargo test -p hmi-core
```

Expected: PASS. Checkpoint: stop and review the workspace before adding domain types.

### Task 2: Domain model and snake_case wire format

**Files:**
- Create: `crates/hmi-core/src/model.rs`
- Create: `crates/hmi-core/src/tags.rs`
- Create: `crates/hmi-core/tests/wire_format.rs`
- Modify: `crates/hmi-core/src/lib.rs`

**Interfaces:**
- Produces: `ProjectDocument`, `Screen`, `UiNode`, `Widget2D`, `DeviceNode`, `DeviceModel`
- Produces: `Rect { x: f32, y: f32, width: f32, height: f32 }`
- Produces: `HexColor(pub String)` with `parse_rgb() -> Option<[u8; 3]>`
- Produces: `Binding`, `BindingTarget`, `BindingMapping`, `ThresholdStop`
- Produces: `TagDefinition`, `TagValue`, `TagValueType`, `SimulatorKind`
- Produces: `ButtonAction`

- [ ] **Step 1: Write failing wire-format tests**

`crates/hmi-core/tests/wire_format.rs`:

```rust
use hmi_core::{
    Binding, BindingMapping, BindingTarget, ButtonAction, DeviceModel, DeviceNode, EquipmentKind,
    HexColor, ProjectDocument, Rect, Screen, TagDefinition, TagValue, TagValueType, UiNode,
    Widget2D,
};

fn sample_project() -> ProjectDocument {
    let tag = TagDefinition {
        id: "bas.supply_fan.run".into(),
        name: "送风机运行".into(),
        value_type: TagValueType::Bool,
        unit: String::new(),
        min: None,
        max: None,
        precision: 0,
        initial: TagValue::Bool(false),
        writable: true,
        simulator: hmi_core::SimulatorKind::Manual,
    };
    let node = UiNode {
        id: "node.supply_fan".into(),
        name: "送风机 SF-01".into(),
        rect: Rect { x: 360.0, y: 220.0, width: 160.0, height: 120.0 },
        rotation_deg: 0.0,
        z: 10,
        visible: true,
        locked: false,
        widget: Widget2D::EquipmentSymbol {
            equipment: EquipmentKind::Fan,
            label: "SF-01".into(),
        },
        bindings: vec![Binding {
            tag_id: tag.id.clone(),
            target: BindingTarget::Color,
            mapping: BindingMapping::BoolColor {
                true_color: HexColor("#37e6d4".into()),
                false_color: HexColor("#5f7780".into()),
            },
        }],
    };
    ProjectDocument {
        schema_version: 1,
        id: "project.bas_demo".into(),
        name: "车站 BAS 环控示例".into(),
        design_size: [1600, 900],
        tags: vec![tag],
        startup_screen: "screen.bas_2d".into(),
        screens: vec![
            Screen::TwoD {
                id: "screen.bas_2d".into(),
                name: "环控工艺图".into(),
                background: HexColor("#071217".into()),
                nodes: vec![node],
            },
            Screen::ThreeD {
                id: "screen.bas_3d".into(),
                name: "环控机房".into(),
                background: HexColor("#061016".into()),
                nodes: vec![DeviceNode {
                    id: "node.ahu".into(),
                    name: "AHU-01".into(),
                    position: [0.0, 0.0, 0.0],
                    rotation_deg: [0.0, 0.0, 0.0],
                    scale: [1.0, 1.0, 1.0],
                    visible: true,
                    locked: false,
                    label: Some("AHU-01".into()),
                    model: DeviceModel::Ahu {
                        size: [3.0, 2.0, 2.0],
                        color: HexColor("#23434d".into()),
                    },
                    bindings: vec![],
                }],
            },
        ],
    }
}

#[test]
fn serializes_all_enum_values_as_snake_case() {
    let json = serde_json::to_string(&sample_project()).unwrap();
    for expected in ["\"kind\":\"two_d\"", "\"kind\":\"three_d\"", "\"type\":\"equipment_symbol\"", "\"target\":\"color\"", "\"type\":\"bool_color\"", "\"value_type\":\"bool\"", "\"simulator\":\"manual\""] {
        assert!(json.contains(expected), "missing {expected} in {json}");
    }
    for forbidden in ["\"TwoD\"", "\"ThreeD\"", "\"EquipmentSymbol\"", "\"BoolColor\"", "\"Manual\""] {
        assert!(!json.contains(forbidden), "PascalCase leaked: {forbidden}");
    }
}

#[test]
fn rejects_pascal_case_and_unknown_fields() {
    let value = serde_json::to_value(sample_project()).unwrap();
    let mut bad_kind = value.clone();
    bad_kind["screens"][0]["kind"] = "TwoD".into();
    assert!(serde_json::from_value::<ProjectDocument>(bad_kind).is_err());

    let mut unknown = value;
    unknown["unexpected"] = true.into();
    assert!(serde_json::from_value::<ProjectDocument>(unknown).is_err());
}

#[test]
fn button_action_uses_type_tag() {
    let action = ButtonAction::ToggleTag { tag_id: "bas.supply_fan.run".into() };
    let json = serde_json::to_string(&action).unwrap();
    assert_eq!(json, r#"{"type":"toggle_tag","tag_id":"bas.supply_fan.run"}"#);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p hmi-core --test wire_format
```

Expected: FAIL because the model modules do not exist.

- [ ] **Step 3: Implement tags and shared value types**

`crates/hmi-core/src/tags.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagValueType { Number, Bool, Enum }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulatorKind { Constant, Sine, Pulse, Step, RandomWalk, Manual }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TagValue { Number(f32), Bool(bool), Enum(String) }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagDefinition {
    pub id: String,
    pub name: String,
    pub value_type: TagValueType,
    pub unit: String,
    #[serde(default)]
    pub min: Option<f32>,
    #[serde(default)]
    pub max: Option<f32>,
    pub precision: u8,
    pub initial: TagValue,
    pub writable: bool,
    pub simulator: SimulatorKind,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TagSnapshot {
    values: BTreeMap<String, TagValue>,
    revision: u64,
}

impl TagSnapshot {
    pub fn new(values: BTreeMap<String, TagValue>, revision: u64) -> Self {
        Self { values, revision }
    }
    pub fn get(&self, id: &str) -> Option<&TagValue> { self.values.get(id) }
    pub fn values(&self) -> &BTreeMap<String, TagValue> { &self.values }
    pub fn revision(&self) -> u64 { self.revision }
    pub fn set(&mut self, id: impl Into<String>, value: TagValue) -> bool {
        let id = id.into();
        if self.values.get(&id) == Some(&value) { return false; }
        self.values.insert(id, value);
        self.revision += 1;
        true
    }
    pub fn remove(&mut self, id: &str) -> bool {
        let removed = self.values.remove(id).is_some();
        if removed { self.revision += 1; }
        removed
    }
}

impl TagValue {
    pub fn matches_type(&self, ty: TagValueType) -> bool {
        matches!(
            (self, ty),
            (TagValue::Number(_), TagValueType::Number)
                | (TagValue::Bool(_), TagValueType::Bool)
                | (TagValue::Enum(_), TagValueType::Enum)
        )
    }
}
```

`crates/hmi-core/src/model.rs`:

```rust
use crate::tags::TagDefinition;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HexColor(pub String);

impl HexColor {
    pub fn parse_rgb(&self) -> Option<[u8; 3]> {
        let bytes = self.0.as_bytes();
        if bytes.len() != 7 || bytes[0] != b'#' { return None; }
        let r = u8::from_str_radix(std::str::from_utf8(&bytes[1..3]).ok()?, 16).ok()?;
        let g = u8::from_str_radix(std::str::from_utf8(&bytes[3..5]).ok()?, 16).ok()?;
        let b = u8::from_str_radix(std::str::from_utf8(&bytes[5..7]).ok()?, 16).ok()?;
        Some([r, g, b])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign { Left, Center, Right }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarDirection { Horizontal, Vertical }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentKind { Fan, Pump, Ahu, Valve }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ButtonAction {
    ToggleTag { tag_id: String },
    SetTag { tag_id: String, value: crate::tags::TagValue },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BindingMapping {
    Format { #[serde(default)] precision: Option<u8>, #[serde(default)] unit: Option<String> },
    BoolColor { true_color: HexColor, false_color: HexColor },
    ThresholdColor { thresholds: Vec<ThresholdStop> },
    Range { input_min: f32, input_max: f32, output_min: f32, output_max: f32, #[serde(default)] clamp: bool },
    Direct,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThresholdStop { pub value: f32, pub color: HexColor }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingTarget { Text, Color, Visible, Progress, RotationSpeed }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub tag_id: String,
    pub target: BindingTarget,
    pub mapping: BindingMapping,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Widget2D {
    Panel { fill: HexColor, stroke: HexColor, stroke_width: f32 },
    Label { text: String, font_size: f32, color: HexColor, align: TextAlign },
    ValueCard { label: String },
    StatusLamp { label: String },
    BarGauge { min: f32, max: f32, direction: BarDirection },
    Button { label: String, action: ButtonAction },
    Pipe { points: Vec<[f32; 2]>, thickness: f32, color: HexColor },
    EquipmentSymbol { equipment: EquipmentKind, label: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiNode {
    pub id: String,
    pub name: String,
    pub rect: Rect,
    pub rotation_deg: f32,
    pub z: i32,
    pub visible: bool,
    pub locked: bool,
    pub widget: Widget2D,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceModel {
    Ahu { size: [f32; 3], color: HexColor },
    Fan { size: [f32; 3], color: HexColor },
    Pump { size: [f32; 3], color: HexColor },
    Duct { length: f32, diameter: f32, color: HexColor },
    Pipe { length: f32, diameter: f32, color: HexColor },
    Valve { size: f32, color: HexColor },
    Glb { path: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceNode {
    pub id: String,
    pub name: String,
    pub position: [f32; 3],
    pub rotation_deg: [f32; 3],
    pub scale: [f32; 3],
    pub visible: bool,
    pub locked: bool,
    #[serde(default)]
    pub label: Option<String>,
    pub model: DeviceModel,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Screen {
    TwoD { id: String, name: String, background: HexColor, nodes: Vec<UiNode> },
    ThreeD { id: String, name: String, background: HexColor, nodes: Vec<DeviceNode> },
}

impl Screen {
    pub fn id(&self) -> &str {
        match self { Screen::TwoD { id, .. } | Screen::ThreeD { id, .. } => id }
    }
    pub fn name(&self) -> &str {
        match self { Screen::TwoD { name, .. } | Screen::ThreeD { name, .. } => name }
    }
    pub fn is_two_d(&self) -> bool { matches!(self, Screen::TwoD { .. }) }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDocument {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub design_size: [u32; 2],
    #[serde(default)]
    pub tags: Vec<TagDefinition>,
    pub screens: Vec<Screen>,
    pub startup_screen: String,
}
```

`crates/hmi-core/src/lib.rs`:

```rust
pub const SCHEMA_VERSION: u32 = 1;

pub mod model;
pub mod tags;

pub use model::*;
pub use tags::*;
```

- [ ] **Step 4: Run the wire-format tests**

```bash
cargo test -p hmi-core --test wire_format
```

Expected: PASS.

- [ ] **Step 5: Check serialization with a human-readable assertion**

```bash
cargo test -p hmi-core --test wire_format -- --nocapture
cargo fmt --check
```

Expected: PASS. Checkpoint: do not proceed until flags such as `two_d`, `bool_color`, `manual` match the spec example.

### Task 3: Structural validation

**Files:**
- Create: `crates/hmi-core/src/validation.rs`
- Create: `crates/hmi-core/tests/validation.rs`
- Modify: `crates/hmi-core/src/lib.rs`

**Interfaces:**
- Produces: `ValidationIssue { path: String, message: String }`
- Produces: `validate_project(project: &ProjectDocument) -> Result<(), Vec<ValidationIssue>>`
- Produces: `validate_tag_value(tag: &TagDefinition) -> Result<(), ValidationIssue>`
- Produces: `is_valid_glb_path(path: &str) -> bool`

- [ ] **Step 1: Write failing validation tests**

`crates/hmi-core/tests/validation.rs`:

```rust
use hmi_core::*;

fn minimal_project() -> ProjectDocument {
    ProjectDocument {
        schema_version: 1,
        id: "project.test".into(),
        name: "测试工程".into(),
        design_size: [1600, 900],
        tags: vec![TagDefinition {
            id: "bas.temp".into(),
            name: "送风温度".into(),
            value_type: TagValueType::Number,
            unit: "°C".into(),
            min: Some(0.0),
            max: Some(50.0),
            precision: 1,
            initial: TagValue::Number(20.0),
            writable: false,
            simulator: SimulatorKind::Sine,
        }],
        startup_screen: "screen.2d".into(),
        screens: vec![Screen::TwoD {
            id: "screen.2d".into(),
            name: "工艺图".into(),
            background: HexColor("#071217".into()),
            nodes: vec![UiNode {
                id: "node.value".into(),
                name: "温度".into(),
                rect: Rect { x: 10.0, y: 10.0, width: 200.0, height: 80.0 },
                rotation_deg: 0.0,
                z: 1,
                visible: true,
                locked: false,
                widget: Widget2D::ValueCard { label: "送风温度".into() },
                bindings: vec![Binding {
                    tag_id: "bas.temp".into(),
                    target: BindingTarget::Text,
                    mapping: BindingMapping::Format { precision: None, unit: None },
                }],
            }],
        }],
    }
}

fn messages(project: &ProjectDocument) -> Vec<String> {
    validate_project(project).unwrap_err().into_iter().map(|e| e.message).collect()
}

#[test]
fn accepts_minimal_valid_project() {
    validate_project(&minimal_project()).unwrap();
}

#[test]
fn rejects_bad_schema_and_design_size() {
    let mut project = minimal_project();
    project.schema_version = 2;
    project.design_size = [1920, 1080];
    let messages = messages(&project);
    assert!(messages.iter().any(|m| m.contains("schema_version")));
    assert!(messages.iter().any(|m| m.contains("1600")));
}

#[test]
fn rejects_duplicate_ids_and_missing_references() {
    let mut project = minimal_project();
    project.tags.push(project.tags[0].clone());
    project.startup_screen = "missing".into();
    if let Screen::TwoD { nodes, .. } = &mut project.screens[0] {
        nodes[0].bindings[0].tag_id = "missing.tag".into();
    }
    let messages = messages(&project);
    assert!(messages.iter().any(|m| m.contains("重复")));
    assert!(messages.iter().any(|m| m.contains("startup_screen")));
    assert!(messages.iter().any(|m| m.contains("missing.tag")));
}

#[test]
fn rejects_mismatched_initial_value_and_simulator() {
    let mut project = minimal_project();
    project.tags[0].initial = TagValue::Bool(true);
    project.tags[0].simulator = SimulatorKind::Pulse;
    let messages = messages(&project);
    assert!(messages.iter().any(|m| m.contains("initial")));
    assert!(messages.iter().any(|m| m.contains("simulator")));
}

#[test]
fn validates_glb_paths() {
    assert!(is_valid_glb_path("models/ahu.glb"));
    for bad in ["/models/ahu.glb", "../ahu.glb", "https://x/ahu.glb", "models\\ahu.glb", "models/ahu.png"] {
        assert!(!is_valid_glb_path(bad), "{bad} should fail");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p hmi-core --test validation
```

Expected: FAIL because `validate_project` and `is_valid_glb_path` do not exist.

- [ ] **Step 3: Implement validation**

`crates/hmi-core/src/validation.rs` starts with:

```rust
use crate::{
    DeviceModel, ProjectDocument, Screen, SimulatorKind, TagDefinition, TagValue,
    TagValueType, Widget2D,
};

pub const MAX_TAGS: usize = 300;
pub const MAX_NODES_2D: usize = 200;
pub const MAX_NODES_3D: usize = 80;
pub const MAX_IMPORT_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue { pub path: String, pub message: String }

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

fn issue(path: impl Into<String>, message: impl Into<String>) -> ValidationIssue {
    ValidationIssue { path: path.into(), message: message.into() }
}

pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':'))
}

pub fn is_valid_glb_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    !path.is_empty()
        && path.len() <= 256
        && lower.ends_with(".glb")
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains("..")
        && !lower.starts_with("http:")
        && !lower.starts_with("https:")
}

pub fn validate_tag_value(tag: &TagDefinition) -> Result<(), ValidationIssue> {
    let path = format!("tags[{}].initial", tag.id);
    if !tag.initial.matches_type(tag.value_type) {
        return Err(issue(path, "initial 与 value_type 不一致"));
    }
    let simulator_ok = match tag.simulator {
        SimulatorKind::Sine | SimulatorKind::RandomWalk | SimulatorKind::Step => {
            tag.value_type == TagValueType::Number
        }
        SimulatorKind::Pulse => tag.value_type == TagValueType::Bool,
        SimulatorKind::Constant | SimulatorKind::Manual => true,
    };
    if !simulator_ok {
        return Err(issue(format!("tags[{}].simulator", tag.id), "simulator 与 value_type 不兼容"));
    }
    if tag.value_type == TagValueType::Number {
        let min = tag.min.ok_or_else(|| issue(format!("tags[{}].min", tag.id), "Number Tag 必须提供 min"))?;
        let max = tag.max.ok_or_else(|| issue(format!("tags[{}].max", tag.id), "Number Tag 必须提供 max"))?;
        if !min.is_finite() || !max.is_finite() || min >= max {
            return Err(issue(format!("tags[{}]", tag.id), "min/max 必须是有限值且 min < max"));
        }
        if tag.precision > 6 {
            return Err(issue(format!("tags[{}].precision", tag.id), "precision 不能超过 6"));
        }
    }
    Ok(())
}

pub fn validate_project(project: &ProjectDocument) -> Result<(), Vec<ValidationIssue>> {
    use crate::{
        Binding, BindingMapping, BindingTarget, ButtonAction, DeviceModel, HexColor,
        TagValue, UiNode, Widget2D,
    };
    use std::collections::{HashMap, HashSet};

    fn check_color(errors: &mut Vec<ValidationIssue>, path: &str, color: &HexColor) {
        if color.parse_rgb().is_none() {
            errors.push(issue(path, "颜色必须是 #RRGGBB"));
        }
    }

    fn check_finite(errors: &mut Vec<ValidationIssue>, path: &str, value: f32) {
        if !value.is_finite() {
            errors.push(issue(path, "必须是有限数值"));
        }
    }

    fn validate_action(
        errors: &mut Vec<ValidationIssue>,
        path: &str,
        action: &ButtonAction,
        tags: &HashMap<String, &TagDefinition>,
    ) {
        let (tag_id, value) = match action {
            ButtonAction::ToggleTag { tag_id } => (tag_id, None),
            ButtonAction::SetTag { tag_id, value } => (tag_id, Some(value)),
        };
        let Some(tag) = tags.get(tag_id) else {
            errors.push(issue(path, format!("按钮引用的 Tag `{tag_id}` 不存在")));
            return;
        };
        if !tag.writable {
            errors.push(issue(path, format!("按钮 Tag `{tag_id}` 不可写")));
        }
        if let Some(value) = value {
            if !value.matches_type(tag.value_type) {
                errors.push(issue(path, "SetTag 的值类型与 Tag 不一致"));
            }
        } else if tag.value_type != TagValueType::Bool {
            errors.push(issue(path, "ToggleTag 只支持 Bool Tag"));
        }
    }

    fn validate_binding(
        errors: &mut Vec<ValidationIssue>,
        path: &str,
        binding: &Binding,
        tags: &HashMap<String, &TagDefinition>,
    ) {
        let Some(tag) = tags.get(&binding.tag_id) else {
            errors.push(issue(path, format!("绑定的 Tag `{}` 不存在", binding.tag_id)));
            return;
        };
        let legal = matches!(
            (binding.target, &binding.mapping),
            (BindingTarget::Text, BindingMapping::Format { .. })
                | (BindingTarget::Color, BindingMapping::BoolColor { .. })
                | (BindingTarget::Color, BindingMapping::ThresholdColor { .. })
                | (BindingTarget::Visible, BindingMapping::Direct)
                | (BindingTarget::Progress, BindingMapping::Direct)
                | (BindingTarget::Progress, BindingMapping::Range { .. })
                | (BindingTarget::RotationSpeed, BindingMapping::Direct)
                | (BindingTarget::RotationSpeed, BindingMapping::Range { .. })
        );
        if !legal {
            errors.push(issue(path, "BindingTarget 与 BindingMapping 不兼容"));
        }
        match &binding.mapping {
            BindingMapping::Format { precision, .. } => {
                if precision.is_some_and(|value| value > 6) {
                    errors.push(issue(path, "Format precision 不能超过 6"));
                }
            }
            BindingMapping::BoolColor { true_color, false_color } => {
                if tag.value_type != TagValueType::Bool {
                    errors.push(issue(path, "BoolColor 只支持 Bool Tag"));
                }
                check_color(errors, path, true_color);
                check_color(errors, path, false_color);
            }
            BindingMapping::ThresholdColor { thresholds } => {
                if tag.value_type != TagValueType::Number {
                    errors.push(issue(path, "ThresholdColor 只支持 Number Tag"));
                }
                if !(1..=8).contains(&thresholds.len()) {
                    errors.push(issue(path, "ThresholdColor 必须有 1–8 个阈值"));
                }
                let mut previous = f32::NEG_INFINITY;
                for stop in thresholds {
                    check_finite(errors, path, stop.value);
                    check_color(errors, path, &stop.color);
                    if stop.value <= previous {
                        errors.push(issue(path, "ThresholdColor 阈值必须严格升序"));
                    }
                    previous = stop.value;
                }
            }
            BindingMapping::Range { input_min, input_max, output_min, output_max, .. } => {
                for value in [*input_min, *input_max, *output_min, *output_max] {
                    check_finite(errors, path, value);
                }
                if input_min == input_max {
                    errors.push(issue(path, "Range input_min 不能等于 input_max"));
                }
            }
            BindingMapping::Direct => {}
        }
    }

    fn validate_ui_node(
        errors: &mut Vec<ValidationIssue>,
        path: &str,
        node: &UiNode,
        tags: &HashMap<String, &TagDefinition>,
    ) {
        if node.name.trim().is_empty() {
            errors.push(issue(path, "节点名称不能为空"));
        }
        for (name, value) in [
            ("rect.x", node.rect.x),
            ("rect.y", node.rect.y),
            ("rect.width", node.rect.width),
            ("rect.height", node.rect.height),
            ("rotation_deg", node.rotation_deg),
        ] {
            check_finite(errors, path, value);
            if name.starts_with("rect.") && matches!(name, "rect.width" | "rect.height") && value <= 0.0 {
                errors.push(issue(path, format!("{name} 必须大于 0")));
            }
        }
        match &node.widget {
            Widget2D::Panel { fill, stroke, stroke_width } => {
                check_color(errors, path, fill);
                check_color(errors, path, stroke);
                if !stroke_width.is_finite() || *stroke_width < 0.0 {
                    errors.push(issue(path, "stroke_width 必须是非负有限值"));
                }
            }
            Widget2D::Label { text, font_size, color, .. } => {
                if text.trim().is_empty() {
                    errors.push(issue(path, "Label text 不能为空"));
                }
                if !font_size.is_finite() || *font_size <= 0.0 {
                    errors.push(issue(path, "font_size 必须大于 0"));
                }
                check_color(errors, path, color);
            }
            Widget2D::ValueCard { label } | Widget2D::StatusLamp { label } => {
                if label.trim().is_empty() {
                    errors.push(issue(path, "label 不能为空"));
                }
            }
            Widget2D::BarGauge { min, max, .. } => {
                check_finite(errors, path, *min);
                check_finite(errors, path, *max);
                if min >= max {
                    errors.push(issue(path, "BarGauge min 必须小于 max"));
                }
            }
            Widget2D::Button { label, action } => {
                if label.trim().is_empty() {
                    errors.push(issue(path, "Button label 不能为空"));
                }
                validate_action(errors, path, action, tags);
            }
            Widget2D::Pipe { points, thickness, color } => {
                if points.len() < 2 {
                    errors.push(issue(path, "Pipe 至少需要两个点"));
                }
                for point in points {
                    check_finite(errors, path, point[0]);
                    check_finite(errors, path, point[1]);
                }
                if !thickness.is_finite() || *thickness <= 0.0 {
                    errors.push(issue(path, "Pipe thickness 必须大于 0"));
                }
                check_color(errors, path, color);
            }
            Widget2D::EquipmentSymbol { label, .. } => {
                if label.trim().is_empty() {
                    errors.push(issue(path, "EquipmentSymbol label 不能为空"));
                }
            }
        }
        for binding in &node.bindings {
            validate_binding(errors, path, binding, tags);
        }
    }

    fn validate_device_model(errors: &mut Vec<ValidationIssue>, path: &str, model: &DeviceModel) {
        match model {
            DeviceModel::Ahu { size, color } | DeviceModel::Fan { size, color } | DeviceModel::Pump { size, color } => {
                for value in size {
                    if !value.is_finite() || *value <= 0.0 {
                        errors.push(issue(path, "设备 size 必须全部大于 0"));
                    }
                }
                check_color(errors, path, color);
            }
            DeviceModel::Duct { length, diameter, color } | DeviceModel::Pipe { length, diameter, color } => {
                if !length.is_finite() || *length <= 0.0 || !diameter.is_finite() || *diameter <= 0.0 {
                    errors.push(issue(path, "length/diameter 必须大于 0"));
                }
                check_color(errors, path, color);
            }
            DeviceModel::Valve { size, color } => {
                if !size.is_finite() || *size <= 0.0 {
                    errors.push(issue(path, "Valve size 必须大于 0"));
                }
                check_color(errors, path, color);
            }
            DeviceModel::Glb { path: glb_path } => {
                if !is_valid_glb_path(glb_path) {
                    errors.push(issue(path, "GLB 路径必须是同源相对路径且以 .glb 结尾"));
                }
            }
        }
    }

    let mut errors = Vec::new();
    if project.schema_version != crate::SCHEMA_VERSION {
        errors.push(issue("schema_version", format!("只支持 {}", crate::SCHEMA_VERSION)));
    }
    if project.design_size != [1600, 900] {
        errors.push(issue("design_size", "必须是 1600×900"));
    }
    if project.name.trim().is_empty() {
        errors.push(issue("name", "工程名称不能为空"));
    }
    if !is_valid_id(&project.id) {
        errors.push(issue("id", "工程 ID 只能包含字母、数字、. _ - :，且最长 128"));
    }

    if project.tags.len() > MAX_TAGS {
        errors.push(issue("tags", format!("最多 {MAX_TAGS} 个 Tag")));
    }
    let mut tag_ids = HashSet::new();
    let mut tags = HashMap::new();
    for (index, tag) in project.tags.iter().enumerate() {
        let path = format!("tags[{index}]");
        if !is_valid_id(&tag.id) {
            errors.push(issue(&path, "Tag ID 非法"));
        }
        if !tag_ids.insert(tag.id.clone()) {
            errors.push(issue(&path, format!("Tag ID `{}` 重复", tag.id)));
        }
        if tag.name.trim().is_empty() {
            errors.push(issue(&path, "Tag 名称不能为空"));
        }
        if let Err(error) = validate_tag_value(tag) {
            errors.push(error);
        }
        if tag.value_type == TagValueType::Number {
            if let TagValue::Number(initial) = &tag.initial {
                if !initial.is_finite() {
                    errors.push(issue(&path, "initial 必须是有限数值"));
                }
                if let (Some(min), Some(max)) = (tag.min, tag.max) {
                    if *initial < min || *initial > max {
                        errors.push(issue(&path, "initial 必须位于 min/max 之间"));
                    }
                }
            }
        }
        tags.insert(tag.id.clone(), tag);
    }

    if project.screens.is_empty() {
        errors.push(issue("screens", "工程至少需要一个画面"));
    }
    let mut screen_ids = HashSet::new();
    let mut node_ids = HashSet::new();
    let mut found_startup = false;

    for (screen_index, screen) in project.screens.iter().enumerate() {
        let screen_path = format!("screens[{screen_index}]");
        if !is_valid_id(screen.id()) {
            errors.push(issue(&screen_path, "画面 ID 非法"));
        }
        if !screen_ids.insert(screen.id().to_string()) {
            errors.push(issue(&screen_path, format!("画面 ID `{}` 重复", screen.id())));
        }
        if screen.name().trim().is_empty() {
            errors.push(issue(&screen_path, "画面名称不能为空"));
        }
        if screen.id() == project.startup_screen {
            found_startup = true;
        }
        match screen {
            Screen::TwoD { background, nodes, .. } => {
                check_color(&mut errors, &screen_path, background);
                if nodes.len() > MAX_NODES_2D {
                    errors.push(issue(&screen_path, format!("2D 画面最多 {MAX_NODES_2D} 个节点")));
                }
                for (node_index, node) in nodes.iter().enumerate() {
                    let node_path = format!("{screen_path}.nodes[{node_index}]");
                    if !is_valid_id(&node.id) {
                        errors.push(issue(&node_path, "节点 ID 非法"));
                    }
                    if !node_ids.insert(node.id.clone()) {
                        errors.push(issue(&node_path, format!("节点 ID `{}` 重复", node.id)));
                    }
                    validate_ui_node(&mut errors, &node_path, node, &tags);
                }
            }
            Screen::ThreeD { background, nodes, .. } => {
                check_color(&mut errors, &screen_path, background);
                if nodes.len() > MAX_NODES_3D {
                    errors.push(issue(&screen_path, format!("3D 画面最多 {MAX_NODES_3D} 个设备节点")));
                }
                for (node_index, node) in nodes.iter().enumerate() {
                    let node_path = format!("{screen_path}.nodes[{node_index}]");
                    if !is_valid_id(&node.id) {
                        errors.push(issue(&node_path, "节点 ID 非法"));
                    }
                    if !node_ids.insert(node.id.clone()) {
                        errors.push(issue(&node_path, format!("节点 ID `{}` 重复", node.id)));
                    }
                    if node.name.trim().is_empty() {
                        errors.push(issue(&node_path, "设备名称不能为空"));
                    }
                    for value in node.position.iter().chain(node.rotation_deg.iter()) {
                        if !value.is_finite() {
                            errors.push(issue(&node_path, "position/rotation 必须是有限数值"));
                        }
                    }
                    for value in node.scale {
                        if !value.is_finite() || value <= 0.0 {
                            errors.push(issue(&node_path, "scale 必须全部大于 0"));
                        }
                    }
                    validate_device_model(&mut errors, &node_path, &node.model);
                    for binding in &node.bindings {
                        validate_binding(&mut errors, &node_path, binding, &tags);
                    }
                }
            }
        }
    }

    if !found_startup {
        errors.push(issue("startup_screen", format!("启动画面 `{}` 不存在", project.startup_screen)));
    }

    if errors.is_empty() { Ok(()) } else { Err(errors) }
}
```

Implement the ten checks in the same step; the helper functions above must be included in the final patch.

Binding compatibility matrix:

```rust
Text => Format
Color => BoolColor | ThresholdColor
Visible => Direct
Progress => Direct | Range
RotationSpeed => Direct | Range
```

- [ ] **Step 4: Run validation tests**

```bash
cargo test -p hmi-core --test validation
```

Expected: PASS.

- [ ] **Step 5: Run all core tests**

```bash
cargo test -p hmi-core
cargo fmt --check
```

Expected: PASS. Checkpoint: validation must reject a repository-bad project before any UI is written.

### Task 4: Built-in BAS demonstration project

**Files:**
- Create: `crates/hmi-core/src/demo.rs`
- Create: `crates/hmi-core/tests/demo.rs`
- Modify: `crates/hmi-core/src/lib.rs`

**Interfaces:**
- Produces: `pub fn bas_demo() -> ProjectDocument`
- Guarantees: exactly two screens with IDs `screen.bas_2d` and `screen.bas_3d`
- Guarantees: tags used by every binding exist and the project passes `validate_project`

- [ ] **Step 1: Write the failing demo tests**

`crates/hmi-core/tests/demo.rs`:

```rust
use hmi_core::*;

#[test]
fn bas_demo_is_valid_and_has_two_screens() {
    let project = bas_demo();
    validate_project(&project).unwrap();
    assert_eq!(project.screens.len(), 2);
    assert_eq!(project.screens[0].id(), "screen.bas_2d");
    assert_eq!(project.screens[1].id(), "screen.bas_3d");
    assert!(project.screens[0].is_two_d());
}

#[test]
fn bas_demo_covers_the_required_tags_and_equipment() {
    let project = bas_demo();
    let ids: Vec<_> = project.tags.iter().map(|t| t.id.as_str()).collect();
    for required in [
        "bas.ahu.supply_temp",
        "bas.station.humidity",
        "bas.station.co2",
        "bas.supply_fan.run",
        "bas.supply_fan.fault",
        "bas.supply_fan.freq",
        "bas.chilled_pump.run",
        "bas.ahu.valve.opening",
    ] {
        assert!(ids.contains(&required), "missing tag {required}");
    }
    let two_d = match &project.screens[0] { Screen::TwoD { nodes, .. } => nodes, _ => unreachable!() };
    assert!(two_d.iter().any(|n| matches!(n.widget, Widget2D::Button { .. })));
    assert!(two_d.iter().any(|n| matches!(n.widget, Widget2D::BarGauge { .. })));
    assert!(two_d.iter().any(|n| matches!(n.widget, Widget2D::EquipmentSymbol { equipment: EquipmentKind::Fan, .. })));
    let three_d = match &project.screens[1] { Screen::ThreeD { nodes, .. } => nodes, _ => unreachable!() };
    assert!(three_d.iter().any(|n| matches!(n.model, DeviceModel::Ahu { .. })));
    assert!(three_d.iter().any(|n| matches!(n.model, DeviceModel::Fan { .. })));
    assert!(three_d.iter().any(|n| matches!(n.model, DeviceModel::Pump { .. })));
}

#[test]
fn demo_bindings_are_editable_and_explainable() {
    let project = bas_demo();
    let all_bindings = project.screens.iter().flat_map(|screen| match screen {
        Screen::TwoD { nodes, .. } => nodes.iter().flat_map(|n| n.bindings.iter()).collect::<Vec<_>>(),
        Screen::ThreeD { nodes, .. } => nodes.iter().flat_map(|n| n.bindings.iter()).collect::<Vec<_>>(),
    }).collect::<Vec<_>>();
    assert!(all_bindings.len() >= 10);
    assert!(all_bindings.iter().any(|b| b.target == BindingTarget::RotationSpeed));
    assert!(all_bindings.iter().any(|b| b.target == BindingTarget::Text));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p hmi-core --test demo
```

Expected: FAIL because `bas_demo` does not exist.

- [ ] **Step 3: Implement deterministic demo constructors**

`crates/hmi-core/src/demo.rs` must build the project without random data or timestamps. Use these helper signatures:

```rust
fn tag(
    id: &str, name: &str, value_type: TagValueType, unit: &str,
    min: Option<f32>, max: Option<f32>, precision: u8,
    initial: TagValue, writable: bool, simulator: SimulatorKind,
) -> TagDefinition

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect

fn ui(
    id: &str, name: &str, rect: Rect, z: i32, widget: Widget2D,
    bindings: Vec<Binding>,
) -> UiNode

fn device(
    id: &str, name: &str, position: [f32; 3], model: DeviceModel,
    bindings: Vec<Binding>,
) -> DeviceNode

fn bind(tag_id: &str, target: BindingTarget, mapping: BindingMapping) -> Binding

fn color_bool(tag_id: &str, on: &str, off: &str) -> Binding

fn text_format(tag_id: &str) -> Binding

fn range(tag_id: &str, input_min: f32, input_max: f32, output_min: f32, output_max: f32) -> Binding
```

Populate the demo exactly as follows. Every 2D node has `rotation_deg = 0.0`, `visible = true`, and `locked = false`; every 3D node has `rotation_deg = [0.0, 0.0, 0.0]`, `scale = [1.0, 1.0, 1.0]`, `visible = true`, and `locked = false` unless the table states otherwise. `Pipe.points` are local design pixels relative to the node rect origin; the rect is the selection and layout bounds.

Tags:

| ID | Name | Type | Unit | Min | Max | Precision | Initial | Writable | Simulator |
| --- | --- | --- | --- | ---: | ---: | ---: | --- | --- | --- |
| `bas.ahu.supply_temp` | 送风温度 | number | °C | 18 | 30 | 1 | 23.5 | false | sine |
| `bas.station.humidity` | 车站湿度 | number | % | 30 | 80 | 0 | 55 | false | sine |
| `bas.station.co2` | CO₂ 浓度 | number | ppm | 400 | 1200 | 0 | 650 | false | sine |
| `bas.supply_fan.run` | 送风机运行 | bool | — | — | — | 0 | false | true | manual |
| `bas.supply_fan.fault` | 送风机故障 | bool | — | — | — | 0 | false | false | constant |
| `bas.supply_fan.freq` | 送风机频率 | number | Hz | 0 | 50 | 1 | 35.0 | false | sine |
| `bas.chilled_pump.run` | 冷冻水泵运行 | bool | — | — | — | 0 | true | false | constant |
| `bas.ahu.valve.opening` | 冷冻水阀开度 | number | % | 0 | 100 | 0 | 62 | false | sine |

2D base and equipment nodes:

| ID | Name | Rect `(x,y,w,h)` | Z | Widget | Bindings |
| --- | --- | --- | ---: | --- | --- |
| `node.bg` | 画面背景 | `(0,0,1600,900)` | 0 | `Panel { fill: #071217, stroke: #132b34, stroke_width: 1.0 }` | — |
| `node.title` | 标题 | `(48,24,620,48)` | 10 | `Label { text: 车站 BAS 环控工艺, font_size: 30.0, color: #e8f2f4, align: left }` | — |
| `node.ahu` | `AHU-01` | `(650,250,260,180)` | 20 | `EquipmentSymbol { equipment: Ahu, label: AHU-01 }` | `Color <- bas.supply_fan.run / BoolColor(#37e6d4, #31424b)` |
| `node.supply_fan` | `SF-01` | `(380,460,180,120)` | 20 | `EquipmentSymbol { equipment: Fan, label: SF-01 }` | `Color <- bas.supply_fan.run / BoolColor(#37e6d4, #31424b)` |
| `node.exhaust_fan` | `EF-01` | `(1040,460,180,120)` | 20 | `EquipmentSymbol { equipment: Fan, label: EF-01 }` | `Color <- bas.chilled_pump.run / BoolColor(#37e6d4, #31424b)` |
| `node.chilled_pump` | `CHP-01` | `(650,640,160,120)` | 20 | `EquipmentSymbol { equipment: Pump, label: CHP-01 }` | `Color <- bas.chilled_pump.run / BoolColor(#37e6d4, #31424b)` |
| `node.cooling_pump` | `CWP-01` | `(880,640,160,120)` | 20 | `EquipmentSymbol { equipment: Pump, label: CWP-01 }` | `Color <- bas.chilled_pump.run / BoolColor(#37e6d4, #31424b)` |
| `node.valve_fresh` | 新风阀 | `(330,260,90,80)` | 20 | `EquipmentSymbol { equipment: Valve, label: 新风阀 }` | — |
| `node.valve_return` | 回风阀 | `(1160,260,90,80)` | 20 | `EquipmentSymbol { equipment: Valve, label: 回风阀 }` | — |
| `node.valve_chilled` | 冷冻水阀 | `(800,560,90,80)` | 20 | `EquipmentSymbol { equipment: Valve, label: 冷冻水阀 }` | `Progress <- bas.ahu.valve.opening / Range(0,100,0,1,clamp=true)` |

2D pipes, value cards, status and controls:

| ID | Name | Rect `(x,y,w,h)` | Z | Widget | Bindings |
| --- | --- | --- | ---: | --- | --- |
| `node.duct.supply` | 送风管 | `(560,330,260,24)` | 15 | `Pipe { points: [[0,12],[260,12]], thickness: 12.0, color: #2c6672 }` | — |
| `node.duct.return` | 回风管 | `(960,330,200,24)` | 15 | `Pipe { points: [[0,12],[200,12]], thickness: 12.0, color: #31505a }` | — |
| `node.duct.chilled` | 冷冻水管 | `(700,430,200,24)` | 15 | `Pipe { points: [[0,12],[200,12]], thickness: 10.0, color: #2d6d77 }` | — |
| `node.pipe.chilled` | 冷冻水回水 | `(650,760,160,20)` | 15 | `Pipe { points: [[0,10],[160,10]], thickness: 8.0, color: #2d6d77 }` | — |
| `node.pipe.return` | 回水管 | `(300,560,160,20)` | 15 | `Pipe { points: [[0,10],[160,10]], thickness: 8.0, color: #31505a }` | — |
| `node.card.temp` | 送风温度 | `(40,170,230,104)` | 30 | `ValueCard { label: 送风温度 }` | `Text <- bas.ahu.supply_temp / Format(None,None)` |
| `node.card.humidity` | 车站湿度 | `(40,292,230,104)` | 30 | `ValueCard { label: 车站湿度 }` | `Text <- bas.station.humidity / Format(None,None)` |
| `node.card.co2` | CO₂ 浓度 | `(40,414,230,104)` | 30 | `ValueCard { label: CO₂ 浓度 }` | `Text <- bas.station.co2 / Format(None,None)` |
| `node.card.freq` | 送风机频率 | `(40,536,230,104)` | 30 | `ValueCard { label: 送风机频率 }` | `Text <- bas.supply_fan.freq / Format(None,None)` |
| `node.lamp.fan` | 送风机运行 | `(300,720,230,72)` | 30 | `StatusLamp { label: 送风机运行 }` | `Color <- bas.supply_fan.run / BoolColor(#37e6d4, #31424b)` |
| `node.lamp.fault` | 送风机故障 | `(560,720,230,72)` | 30 | `StatusLamp { label: 送风机故障 }` | `Color <- bas.supply_fan.fault / BoolColor(#ff4d5e, #31424b)` |
| `node.gauge.valve` | 冷冻水阀开度 | `(1220,600,300,90)` | 30 | `BarGauge { min: 0.0, max: 100.0, direction: horizontal }` | `Progress <- bas.ahu.valve.opening / Range(0,100,0,1,clamp=true)` |
| `node.button.fan` | 启动/停止送风机 | `(840,720,260,72)` | 40 | `Button { label: 启动/停止送风机, action: ToggleTag(bas.supply_fan.run) }` | — |

3D nodes (`label` equals the `name` unless shown otherwise):

| ID | Name | Position | Model | Bindings |
| --- | --- | --- | --- | --- |
| `node.3d.ahu` | `AHU-01` | `[0,1.4,0]` | `Ahu { size: [4.0,2.8,2.2], color: #23434d }` | `Text <- bas.ahu.supply_temp / Format(None,None)`; `Color <- bas.supply_fan.run / BoolColor(#37e6d4, #31424b)` |
| `node.3d.supply_fan` | `SF-01` | `[-4.2,1.0,1.0]` | `Fan { size: [1.8,1.8,1.8], color: #31505a }` | `Color <- bas.supply_fan.run / BoolColor(#37e6d4, #31424b)`; `RotationSpeed <- bas.supply_fan.freq / Range(0,50,0,15,clamp=true)` |
| `node.3d.exhaust_fan` | `EF-01` | `[4.2,1.0,1.0]` | `Fan { size: [1.8,1.8,1.8], color: #31505a }` | `Color <- bas.chilled_pump.run / BoolColor(#37e6d4, #31424b)` |
| `node.3d.chilled_pump` | `CHP-01` | `[-2.0,0.55,3.0]` | `Pump { size: [1.4,1.1,1.4], color: #31505a }` | `Color <- bas.chilled_pump.run / BoolColor(#37e6d4, #31424b)`; `RotationSpeed <- bas.chilled_pump.run / Direct` |
| `node.3d.cooling_pump` | `CWP-01` | `[2.0,0.55,3.0]` | `Pump { size: [1.4,1.1,1.4], color: #31505a }` | `Color <- bas.chilled_pump.run / BoolColor(#37e6d4, #31424b)` |
| `node.3d.duct_supply` | 送风管 | `[-2.8,2.2,-0.5]` | `Duct { length: 3.6, diameter: 0.55, color: #24434f }` | — |
| `node.3d.duct_return` | 回风管 | `[2.8,2.2,-0.5]` | `Duct { length: 3.0, diameter: 0.55, color: #24434f }` | — |
| `node.3d.pipe_chilled` | 冷冻水管 | `[-2.0,0.25,1.9]` | `Pipe { length: 4.0, diameter: 0.18, color: #2d6d77 }` | — |
| `node.3d.pipe_condensate` | 冷凝水管 | `[2.0,0.25,1.9]` | `Pipe { length: 3.0, diameter: 0.18, color: #2d6d77 }` | — |
| `node.3d.valve_fresh` | 新风阀 | `[0,1.9,-1.4]` | `Valve { size: 0.7, color: #31505a }` | — |
| `node.3d.valve_return` | 回风阀 | `[0,1.9,1.4]` | `Valve { size: 0.7, color: #31505a }` | `Progress <- bas.ahu.valve.opening / Range(0,100,0,1,clamp=true)` |
| `node.3d.valve_chilled` | 冷冻水阀 | `[0,0.9,1.9]` | `Valve { size: 0.7, color: #31505a }` | `Progress <- bas.ahu.valve.opening / Range(0,100,0,1,clamp=true)` |

The exact `UiNode` / `DeviceNode` constructors must set the displayed `name` and `label` values above. In particular, `node.ahu` must have `name: "AHU-01"` so the inspector and E2E assertion agree.

- [ ] **Step 4: Run demo tests**

```bash
cargo test -p hmi-core --test demo
```

Expected: PASS.

- [ ] **Step 5: Run all core tests**

```bash
cargo test -p hmi-core
```

Expected: PASS. Checkpoint: inspect `bas_demo()` visually in code; every binding must refer to a real tag and a target supported by the chosen node type.

### Task 5: Deterministic 4 Hz Tag simulation

**Files:**
- Create: `crates/hmi-core/src/error.rs`
- Create: `crates/hmi-core/src/simulation.rs`
- Create: `crates/hmi-core/tests/simulation.rs`
- Modify: `crates/hmi-core/src/lib.rs`

**Interfaces:**
- Produces: `HmiError::{UnknownTag, TagNotWritable, UnknownScreen, UnknownNode, DuplicateId, InvalidCommand, TagInUse, Json}`
- Produces: `TagSimulation::new(project: &ProjectDocument) -> Self`
- Produces: `TagSimulation::tick(&mut self, dt_ms: u64) -> bool`
- Produces: `TagSimulation::write(&mut self, tag_id: &str, value: TagValue) -> Result<(), HmiError>`
- Produces: `TagSimulation::resume(&mut self, tag_id: &str) -> Result<(), HmiError>`
- Produces: `TagSimulation::replace_definitions(&mut self, tags: &[TagDefinition])`
- Produces: `TagSimulation::snapshot(&self) -> &TagSnapshot`
- Constants: `SIMULATION_TICK_MS = 250`, `SINE_PERIOD_MS = 12_000.0`

- [ ] **Step 1: Write failing deterministic simulation tests**

`crates/hmi-core/tests/simulation.rs`:

```rust
use hmi_core::*;

fn project() -> ProjectDocument {
    let tags = vec![
        TagDefinition {
            id: "tag.sine".into(), name: "正弦".into(), value_type: TagValueType::Number,
            unit: "°C".into(), min: Some(10.0), max: Some(30.0), precision: 1,
            initial: TagValue::Number(20.0), writable: true, simulator: SimulatorKind::Sine,
        },
        TagDefinition {
            id: "tag.fixed".into(), name: "只读".into(), value_type: TagValueType::Number,
            unit: String::new(), min: Some(0.0), max: Some(100.0), precision: 0,
            initial: TagValue::Number(1.0), writable: false, simulator: SimulatorKind::Sine,
        },
        TagDefinition {
            id: "tag.pulse".into(), name: "脉冲".into(), value_type: TagValueType::Bool,
            unit: String::new(), min: None, max: None, precision: 0,
            initial: TagValue::Bool(false), writable: false, simulator: SimulatorKind::Pulse,
        },
        TagDefinition {
            id: "tag.walk".into(), name: "随机游走".into(), value_type: TagValueType::Number,
            unit: String::new(), min: Some(0.0), max: Some(100.0), precision: 0,
            initial: TagValue::Number(50.0), writable: false, simulator: SimulatorKind::RandomWalk,
        },
    ];
    ProjectDocument {
        schema_version: 1,
        id: "project.sim".into(),
        name: "仿真".into(),
        design_size: [1600, 900],
        tags,
        startup_screen: "screen".into(),
        screens: vec![Screen::TwoD {
            id: "screen".into(), name: "画面".into(), background: HexColor("#000000".into()), nodes: vec![],
        }],
    }
}

#[test]
fn two_simulations_with_same_fixed_ticks_are_identical() {
    let mut a = TagSimulation::new(&project());
    let mut b = TagSimulation::new(&project());
    let initial = a.snapshot().get("tag.sine").cloned();
    let mut changed = false;
    for _ in 0..40 {
        assert_eq!(a.tick(250), b.tick(250));
        assert_eq!(a.snapshot(), b.snapshot());
        if a.snapshot().get("tag.sine") != initial.as_ref() {
            changed = true;
        }
    }
    assert!(changed, "at least one fixed tick must change the sine value");
}

#[test]
fn pulse_has_fixed_period_and_toggles() {
    let mut sim = TagSimulation::new(&project());
    assert_eq!(sim.snapshot().get("tag.pulse"), Some(&TagValue::Bool(false)));
    sim.tick(3_750);
    assert_eq!(sim.snapshot().get("tag.pulse"), Some(&TagValue::Bool(true)));
    sim.tick(250);
    assert_eq!(sim.snapshot().get("tag.pulse"), Some(&TagValue::Bool(false)));
    sim.tick(3_750);
    assert_eq!(sim.snapshot().get("tag.pulse"), Some(&TagValue::Bool(true)));
    sim.tick(250);
    assert_eq!(sim.snapshot().get("tag.pulse"), Some(&TagValue::Bool(false)));
}

#[test]
fn writable_sine_tag_pauses_after_write_until_resume() {
    let mut sim = TagSimulation::new(&project());
    sim.write("tag.sine", TagValue::Number(12.5)).unwrap();
    for _ in 0..12 { sim.tick(250); }
    assert_eq!(sim.snapshot().get("tag.sine"), Some(&TagValue::Number(12.5)));
    sim.resume("tag.sine").unwrap();
    for _ in 0..12 { sim.tick(250); }
    assert_ne!(sim.snapshot().get("tag.sine"), Some(&TagValue::Number(12.5)));
}

#[test]
fn write_errors_are_explicit() {
    let mut sim = TagSimulation::new(&project());
    assert!(matches!(sim.write("missing", TagValue::Bool(true)), Err(HmiError::UnknownTag(id)) if id == "missing"));
    assert!(matches!(sim.write("tag.fixed", TagValue::Number(12.0)), Err(HmiError::TagNotWritable(id)) if id == "tag.fixed"));
    assert!(matches!(sim.write("tag.sine", TagValue::Bool(true)), Err(HmiError::InvalidCommand(_))));
}

#[test]
fn random_walk_is_deterministic_and_stays_in_range() {
    let mut sim = TagSimulation::new(&project());
    let mut previous = 50.0;
    for _ in 0..40 {
        sim.tick(250);
        let TagValue::Number(current) = sim.snapshot().get("tag.walk").unwrap() else { panic!() };
        assert!((0.0..=100.0).contains(current));
        assert_ne!(*current, previous);
        previous = *current;
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p hmi-core --test simulation
```

Expected: FAIL because `TagSimulation`, `HmiError`, and `SIMULATION_TICK_MS` do not exist.

- [ ] **Step 3: Implement shared errors**

`crates/hmi-core/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HmiError {
    #[error("Tag `{0}` 不存在")]
    UnknownTag(String),
    #[error("Tag `{0}` 不可写")]
    TagNotWritable(String),
    #[error("画面 `{0}` 不存在")]
    UnknownScreen(String),
    #[error("节点 `{0}` 不存在")]
    UnknownNode(String),
    #[error("ID `{0}` 重复")]
    DuplicateId(String),
    #[error("命令无效：{0}")]
    InvalidCommand(String),
    #[error("Tag `{tag_id}` 正被节点引用：{nodes:?}")]
    TagInUse { tag_id: String, nodes: Vec<String> },
    #[error("JSON 无效：{0}")]
    Json(String),
}
```

- [ ] **Step 4: Implement the simulation engine**

`crates/hmi-core/src/simulation.rs`:

```rust
use crate::{
    error::HmiError,
    tags::{TagDefinition, TagSnapshot, TagValue, TagValueType},
    ProjectDocument,
};
use std::collections::{BTreeMap, BTreeSet};

pub const SIMULATION_TICK_MS: u64 = 250;
pub const SINE_PERIOD_MS: f32 = 12_000.0;

#[derive(Debug, Clone)]
struct SimEntry {
    definition: TagDefinition,
    value: TagValue,
    rng: u64,
    manual_override: bool,
}

#[derive(Debug, Clone)]
pub struct TagSimulation {
    entries: BTreeMap<String, SimEntry>,
    clock_ms: u64,
    accumulator_ms: u64,
    snapshot: TagSnapshot,
}

impl TagSimulation {
    pub fn new(project: &ProjectDocument) -> Self {
        let mut entries = BTreeMap::new();
        let mut values = BTreeMap::new();
        for definition in &project.tags {
            let value = definition.initial.clone();
            values.insert(definition.id.clone(), value.clone());
            entries.insert(definition.id.clone(), SimEntry {
                rng: fnv1a(definition.id.as_bytes()),
                definition: definition.clone(),
                value,
                manual_override: false,
            });
        }
        Self {
            entries,
            clock_ms: 0,
            accumulator_ms: 0,
            snapshot: TagSnapshot::new(values, 1),
        }
    }

    pub fn snapshot(&self) -> &TagSnapshot { &self.snapshot }

    pub fn tick(&mut self, dt_ms: u64) -> bool {
        self.accumulator_ms += dt_ms;
        let mut changed = false;
        while self.accumulator_ms >= SIMULATION_TICK_MS {
            self.accumulator_ms -= SIMULATION_TICK_MS;
            self.clock_ms += SIMULATION_TICK_MS;
            for entry in self.entries.values_mut() {
                if entry.manual_override { continue; }
                let next = next_value(entry, self.clock_ms);
                if next != entry.value {
                    entry.value = next;
                    changed |= self.snapshot.set(entry.definition.id.clone(), entry.value.clone());
                }
            }
        }
        changed
    }

    pub fn write(&mut self, tag_id: &str, value: TagValue) -> Result<(), HmiError> {
        let entry = self.entries.get_mut(tag_id).ok_or_else(|| HmiError::UnknownTag(tag_id.into()))?;
        if !entry.definition.writable {
            return Err(HmiError::TagNotWritable(tag_id.into()));
        }
        if !value.matches_type(entry.definition.value_type) {
            return Err(HmiError::InvalidCommand(format!("Tag `{tag_id}` 的值类型不匹配")));
        }
        entry.value = value.clone();
        entry.manual_override = true;
        self.snapshot.set(tag_id, value);
        Ok(())
    }

    pub fn resume(&mut self, tag_id: &str) -> Result<(), HmiError> {
        let entry = self.entries.get_mut(tag_id).ok_or_else(|| HmiError::UnknownTag(tag_id.into()))?;
        entry.manual_override = false;
        Ok(())
    }

    pub fn replace_definitions(&mut self, tags: &[TagDefinition]) {
        let old = std::mem::take(&mut self.entries);
        for definition in tags {
            let preserved = old.get(&definition.id);
            let value = match preserved {
                Some(entry) if entry.definition.value_type == definition.value_type => entry.value.clone(),
                _ => definition.initial.clone(),
            };
            self.entries.insert(definition.id.clone(), SimEntry {
                definition: definition.clone(),
                value,
                rng: preserved.map_or_else(|| fnv1a(definition.id.as_bytes()), |entry| entry.rng),
                manual_override: preserved.is_some_and(|entry| entry.manual_override),
            });
        }
        self.snapshot = TagSnapshot::new(
            self.entries.iter().map(|(id, entry)| (id.clone(), entry.value.clone())).collect(),
            1,
        );
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn next_value(entry: &mut SimEntry, clock_ms: u64) -> TagValue {
    let def = &entry.definition;
    match def.simulator {
        crate::SimulatorKind::Constant | crate::SimulatorKind::Manual => entry.value.clone(),
        crate::SimulatorKind::Pulse => TagValue::Bool((clock_ms % 8_000) < 4_000),
        crate::SimulatorKind::Sine => {
            let min = def.min.unwrap_or(0.0);
            let max = def.max.unwrap_or(100.0);
            let center = (min + max) * 0.5;
            let amplitude = (max - min) * 0.5;
            let phase = (entry.rng % 360) as f32 * std::f32::consts::PI / 180.0;
            let angle = clock_ms as f32 / SINE_PERIOD_MS * std::f32::consts::TAU + phase;
            TagValue::Number(center + amplitude * angle.sin())
        }
        crate::SimulatorKind::Step => {
            if clock_ms >= 5_000 {
                match def.value_type {
                    TagValueType::Number => TagValue::Number(def.max.unwrap_or(100.0)),
                    TagValueType::Bool => TagValue::Bool(true),
                    TagValueType::Enum => entry.value.clone(),
                }
            } else {
                match def.value_type {
                    TagValueType::Number => TagValue::Number(def.min.unwrap_or(0.0)),
                    TagValueType::Bool => TagValue::Bool(false),
                    TagValueType::Enum => entry.value.clone(),
                }
            }
        }
        crate::SimulatorKind::RandomWalk => {
            if let TagValue::Number(current) = entry.value {
                let min = def.min.unwrap_or(0.0);
                let max = def.max.unwrap_or(100.0);
                entry.rng = entry.rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                let unit = ((entry.rng >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0;
                TagValue::Number((current + unit * (max - min) * 0.015).clamp(min, max))
            } else {
                entry.value.clone()
            }
        }
    }
}
```

Also add a compile-time guard that every tag in a valid project has compatible simulation settings; validation already enforces it.

- [ ] **Step 5: Wire modules and run tests**

`crates/hmi-core/src/lib.rs` adds:

```rust
pub mod error;
pub mod simulation;

pub use error::HmiError;
pub use simulation::{TagSimulation, SIMULATION_TICK_MS, SINE_PERIOD_MS};
```

```bash
cargo test -p hmi-core --test simulation
cargo test -p hmi-core
cargo fmt --check
```

Expected: PASS. Checkpoint: fixed tick sequences must be byte-for-byte reproducible across two fresh simulations.

### Task 6: Binding resolution to VisualPatch

**Files:**
- Create: `crates/hmi-core/src/bindings.rs`
- Create: `crates/hmi-core/tests/bindings.rs`
- Modify: `crates/hmi-core/src/lib.rs`

**Interfaces:**
- Produces: `VisualValue::{Text, Color, Visible, Progress, RotationSpeed}`
- Produces: `VisualPatch { node_id: String, value: VisualValue }`
- Produces: `resolve_binding(node_id: &str, binding: &Binding, tag: &TagDefinition, value: &TagValue) -> Result<VisualPatch, HmiError>`
- Produces: `resolve_bindings(node_id: &str, bindings: &[Binding], tags: &BTreeMap<String, TagDefinition>, snapshot: &TagSnapshot) -> Vec<VisualPatch>`
- Produces: `default_mapping(target: BindingTarget, tag: &TagDefinition) -> BindingMapping`
- Produces: `format_tag_value(value: &TagValue, precision: u8, unit: &str) -> String`

- [ ] **Step 1: Write failing binding tests**

`crates/hmi-core/tests/bindings.rs`:

```rust
use hmi_core::*;

fn tag(value_type: TagValueType, unit: &str) -> TagDefinition {
    TagDefinition {
        id: "tag.value".into(), name: "值".into(), value_type, unit: unit.into(),
        min: Some(0.0), max: Some(100.0), precision: 1,
        initial: match value_type {
            TagValueType::Number => TagValue::Number(25.0),
            TagValueType::Bool => TagValue::Bool(true),
            TagValueType::Enum => TagValue::Enum("run".into()),
        },
        writable: false,
        simulator: SimulatorKind::Constant,
    }
}

fn binding(target: BindingTarget, mapping: BindingMapping, tag_id: &str) -> Binding {
    Binding { tag_id: tag_id.into(), target, mapping }
}

#[test]
fn format_inherits_tag_precision_and_unit() {
    let tag = tag(TagValueType::Number, "°C");
    let patch = resolve_binding(
        "n", &binding(BindingTarget::Text, BindingMapping::Format { precision: None, unit: None }, "tag.value"),
        &tag, &TagValue::Number(23.456),
    ).unwrap();
    assert_eq!(patch, VisualPatch { node_id: "n".into(), value: VisualValue::Text("23.5 °C".into()) });
}

#[test]
fn format_overrides_and_handles_bool_and_enum() {
    let numeric = tag(TagValueType::Number, "Hz");
    let patch = resolve_binding(
        "n", &binding(BindingTarget::Text, BindingMapping::Format { precision: Some(0), unit: Some("rpm".into()) }, "tag.value"),
        &numeric, &TagValue::Number(12.6),
    ).unwrap();
    assert_eq!(patch.value, VisualValue::Text("13 rpm".into()));

    let boolean = tag(TagValueType::Bool, "");
    let patch = resolve_binding(
        "n", &binding(BindingTarget::Text, BindingMapping::Format { precision: None, unit: None }, "tag.value"),
        &boolean, &TagValue::Bool(true),
    ).unwrap();
    assert_eq!(patch.value, VisualValue::Text("运行".into()));
}

#[test]
fn bool_and_threshold_colors_select_exact_stops() {
    let boolean = tag(TagValueType::Bool, "");
    let bool_patch = resolve_binding(
        "n", &binding(BindingTarget::Color, BindingMapping::BoolColor {
            true_color: HexColor("#00ff00".into()), false_color: HexColor("#ff0000".into()),
        }, "tag.value"), &boolean, &TagValue::Bool(false),
    ).unwrap();
    assert_eq!(bool_patch.value, VisualValue::Color(HexColor("#ff0000".into())));

    let numeric = tag(TagValueType::Number, "");
    let mapping = BindingMapping::ThresholdColor { thresholds: vec![
        ThresholdStop { value: 10.0, color: HexColor("#00ff00".into()) },
        ThresholdStop { value: 20.0, color: HexColor("#ffcc00".into()) },
        ThresholdStop { value: 30.0, color: HexColor("#ff0000".into()) },
    ]};
    let at_25 = resolve_binding("n", &binding(BindingTarget::Color, mapping.clone(), "tag.value"), &numeric, &TagValue::Number(25.0)).unwrap();
    assert_eq!(at_25.value, VisualValue::Color(HexColor("#ffcc00".into())));
    let below = resolve_binding("n", &binding(BindingTarget::Color, mapping, "tag.value"), &numeric, &TagValue::Number(1.0)).unwrap();
    assert_eq!(below.value, VisualValue::Color(HexColor("#00ff00".into())));
}

#[test]
fn range_clamps_and_direct_coerces_targets() {
    let numeric = tag(TagValueType::Number, "");
    let range = BindingMapping::Range { input_min: 0.0, input_max: 50.0, output_min: 0.0, output_max: 1.0, clamp: true };
    let high = resolve_binding("n", &binding(BindingTarget::Progress, range, "tag.value"), &numeric, &TagValue::Number(75.0)).unwrap();
    assert_eq!(high.value, VisualValue::Progress(1.0));
    let direct = resolve_binding("n", &binding(BindingTarget::RotationSpeed, BindingMapping::Direct, "tag.value"), &numeric, &TagValue::Number(12.0)).unwrap();
    assert_eq!(direct.value, VisualValue::RotationSpeed(12.0));
    let boolean = tag(TagValueType::Bool, "");
    let visible = resolve_binding("n", &binding(BindingTarget::Visible, BindingMapping::Direct, "tag.value"), &boolean, &TagValue::Bool(true)).unwrap();
    assert_eq!(visible.value, VisualValue::Visible(true));
}

#[test]
fn invalid_target_mapping_is_rejected() {
    let numeric = tag(TagValueType::Number, "");
    let result = resolve_binding(
        "n", &binding(BindingTarget::Text, BindingMapping::Direct, "tag.value"),
        &numeric, &TagValue::Number(1.0),
    );
    assert!(result.is_err());
}

#[test]
fn resolve_bindings_keeps_order_and_skips_unknown_tags() {
    let number = tag(TagValueType::Number, "°C");
    let tags = std::collections::BTreeMap::from([(number.id.clone(), number)]);
    let snapshot = TagSnapshot::new(
        std::collections::BTreeMap::from([("tag.value".to_string(), TagValue::Number(21.0))]), 1,
    );
    let bindings = vec![
        binding(BindingTarget::Text, BindingMapping::Format { precision: None, unit: None }, "tag.value"),
        binding(BindingTarget::Text, BindingMapping::Format { precision: None, unit: None }, "missing"),
    ];
    let patches = resolve_bindings("n", &bindings, &tags, &snapshot);
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].value, VisualValue::Text("21.0 °C".into()));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p hmi-core --test bindings
```

Expected: FAIL because binding resolution does not exist.

- [ ] **Step 3: Implement binding resolution**

Implementation rules:

```rust
pub fn format_tag_value(value: &TagValue, precision: u8, unit: &str) -> String {
    let body = match value {
        TagValue::Number(n) => format!("{n:.precision$}"),
        TagValue::Bool(true) => "运行".into(),
        TagValue::Bool(false) => "停止".into(),
        TagValue::Enum(v) => v.clone(),
    };
    if unit.is_empty() { body } else { format!("{body} {unit}") }
}
```

Additional mandatory rules:

- `Text` accepts `Format` only.
- `Color` accepts `BoolColor` only for Bool tags and `ThresholdColor` only for Number tags.
- `Visible` accepts `Direct`; Bool passes through, Number uses `> 0.5`.
- `Progress` accepts `Direct` or `Range`; final value is clamped to `0.0..=1.0`.
- `RotationSpeed` accepts `Direct` or `Range`; Bool maps to `1.0/0.0`, Number is clamped to `>= 0.0`.
- `Range` divides by `input_min - input_max`; validation already rejects equal bounds. If `clamp = true`, clamp the input before mapping.
- `ThresholdColor` uses the last stop whose `value <= input`; if none, use the first stop.
- `resolve_bindings` preserves input order, skips tags absent from `tags` or `snapshot`, and never panics for a malformed project.
- `default_mapping`:
  - `Text + Bool` -> `Format { precision: None, unit: None }`
  - `Text + Number/Enum` -> same `Format`
  - `Color + Bool` -> green/neutral `BoolColor`
  - `Color + Number` -> one-stop `ThresholdColor`
  - `Visible` -> `Direct`
  - `Progress + Number` -> `Range` using tag min/max or `0..100`, output `0..1`, `clamp: true`
  - `RotationSpeed + Number` -> `Range` using tag min/max or `0..50`, output `0..15`, `clamp: true`
  - `RotationSpeed + Bool` -> `Direct`

- [ ] **Step 4: Run binding and core tests**

```bash
cargo test -p hmi-core --test bindings
cargo test -p hmi-core
```

Expected: PASS.

- [ ] **Step 5: Export the module**

Add to `lib.rs`:

```rust
pub mod bindings;
pub use bindings::{
    default_mapping, format_tag_value, resolve_binding, resolve_bindings, VisualPatch, VisualValue,
};
```

```bash
cargo fmt --check
```

Expected: PASS. Checkpoint: binding resolution must not contain any Bevy type.

### Task 7: Editor commands, snapshots, undo/redo, and reference guards

**Files:**
- Create: `crates/hmi-core/src/commands.rs`
- Create: `crates/hmi-core/tests/commands.rs`
- Modify: `crates/hmi-core/src/lib.rs`

**Interfaces:**
- Produces: `EditorCommand` variants listed below
- Produces: `CommandResult { affected_id: Option<String>, document_revision: u64 }`
- Produces: `EditorSession::new(project: ProjectDocument) -> Result<Self, Vec<ValidationIssue>>`
- Produces: `EditorSession::project() -> &ProjectDocument`
- Produces: `EditorSession::execute(&mut self, command: EditorCommand) -> Result<CommandResult, HmiError>`
- Produces: `EditorSession::undo() -> bool`, `redo() -> bool`, `can_undo() -> bool`, `can_redo() -> bool`
- Produces: `EditorSession::document_revision() -> u64`, `autosave_generation() -> u64`
- Produces: `EditorSession::replace_project(&mut self, project) -> Result<(), Vec<ValidationIssue>>`
- Produces: `EditorSession::tag_snapshot() -> &TagSnapshot`
- Produces: `EditorSession::tick_tags(&mut self, dt_ms: u64) -> bool`
- Produces: `EditorSession::write_tag(...)`, `resume_tag_simulation(...)`
- Constant: `UNDO_LIMIT = 50`

`EditorCommand` exact shape:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum EditorCommand {
    AddScreen { screen: Screen },
    DeleteScreen { screen_id: String, new_startup: Option<String> },
    RenameScreen { screen_id: String, name: String },
    SetStartupScreen { screen_id: String },
    Add2dNode { screen_id: String, node: UiNode },
    Update2dNode { screen_id: String, node: UiNode },
    Add3dNode { screen_id: String, node: DeviceNode },
    Update3dNode { screen_id: String, node: DeviceNode },
    DeleteNode { screen_id: String, node_id: String },
    DuplicateNode { screen_id: String, node_id: String },
    SetBindings { screen_id: String, node_id: String, bindings: Vec<Binding> },
    AddTag { tag: TagDefinition },
    UpdateTag { tag: TagDefinition },
    DeleteTag { tag_id: String },
}
```

- [ ] **Step 1: Write failing command tests**

`crates/hmi-core/tests/commands.rs`:

```rust
use hmi_core::*;

fn session() -> EditorSession {
    EditorSession::new(bas_demo()).unwrap()
}

fn first_2d_node(session: &EditorSession) -> UiNode {
    match &session.project().screens[0] {
        Screen::TwoD { nodes, .. } => nodes[0].clone(),
        _ => unreachable!(),
    }
}

#[test]
fn update_move_and_undo_restore_previous_snapshot() {
    let mut session = session();
    let mut node = first_2d_node(&session);
    let original = node.rect;
    node.rect.x += 120.0;
    let result = session.execute(EditorCommand::Update2dNode {
        screen_id: "screen.bas_2d".into(), node: node.clone(),
    }).unwrap();
    assert!(result.document_revision > 1);
    assert_eq!(first_2d_node(&session).rect.x, original.x + 120.0);
    assert!(session.undo());
    assert_eq!(first_2d_node(&session).rect, original);
    assert!(session.redo());
    assert_eq!(first_2d_node(&session).rect.x, original.x + 120.0);
}

#[test]
fn new_command_clears_redo() {
    let mut session = session();
    let mut node = first_2d_node(&session);
    node.rect.x += 10.0;
    session.execute(EditorCommand::Update2dNode { screen_id: "screen.bas_2d".into(), node: node.clone() }).unwrap();
    assert!(session.undo());
    assert!(session.can_redo());
    node.rect.x += 20.0;
    session.execute(EditorCommand::Update2dNode { screen_id: "screen.bas_2d".into(), node }).unwrap();
    assert!(!session.can_redo());
}

#[test]
fn invalid_update_leaves_project_and_undo_unchanged() {
    let mut session = session();
    let before = session.project().clone();
    let mut node = first_2d_node(&session);
    node.rect.width = -1.0;
    let error = session.execute(EditorCommand::Update2dNode { screen_id: "screen.bas_2d".into(), node }).unwrap_err();
    assert!(matches!(error, HmiError::InvalidCommand(_) | HmiError::UnknownNode(_)));
    assert_eq!(session.project(), &before);
    assert!(!session.can_undo());
}

#[test]
fn duplicate_creates_unique_offset_node_and_selects_it() {
    let mut session = session();
    let original = first_2d_node(&session);
    let result = session.execute(EditorCommand::DuplicateNode {
        screen_id: "screen.bas_2d".into(), node_id: original.id.clone(),
    }).unwrap();
    let new_id = result.affected_id.unwrap();
    let copied = match &session.project().screens[0] {
        Screen::TwoD { nodes, .. } => nodes.iter().find(|n| n.id == new_id).unwrap().clone(),
        _ => unreachable!(),
    };
    assert_ne!(copied.id, original.id);
    assert_eq!(copied.rect.x, original.rect.x + 24.0);
    assert_eq!(copied.rect.y, original.rect.y + 24.0);
}

#[test]
fn deleting_referenced_tag_is_blocked() {
    let mut session = session();
    let before = session.project().clone();
    let error = session.execute(EditorCommand::DeleteTag { tag_id: "bas.supply_fan.run".into() }).unwrap_err();
    assert!(matches!(error, HmiError::TagInUse { .. }));
    assert_eq!(session.project(), &before);
}

#[test]
fn last_screen_and_startup_screen_rules_hold() {
    let mut session = session();
    assert!(session.execute(EditorCommand::DeleteScreen {
        screen_id: "screen.bas_2d".into(), new_startup: None,
    }).is_ok());
    assert!(matches!(
        session.execute(EditorCommand::DeleteScreen {
            screen_id: "screen.bas_3d".into(), new_startup: None,
        }),
        Err(HmiError::InvalidCommand(_))
    ));
}

#[test]
fn tag_write_is_outside_undo_and_document_revision() {
    let mut session = session();
    let doc_revision = session.document_revision();
    let autosave = session.autosave_generation();
    session.write_tag("bas.supply_fan.run", TagValue::Bool(true)).unwrap();
    assert_eq!(session.document_revision(), doc_revision);
    assert_eq!(session.autosave_generation(), autosave);
    assert_eq!(session.tag_snapshot().get("bas.supply_fan.run"), Some(&TagValue::Bool(true)));
    assert!(!session.undo());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p hmi-core --test commands
```

Expected: FAIL because `EditorCommand` and `EditorSession` do not exist.

- [ ] **Step 3: Implement the command engine**

Mandatory semantics:

- Every mutating command works on a clone, validates the entire candidate project, then commits. Validation failures are joined and returned as `HmiError::InvalidCommand`. A failure leaves project, undo stack, redo stack, revisions, and simulation untouched.
- On commit: push the previous project to undo, cap at 50, clear redo, increment `document_revision` and `autosave_generation`, call `simulation.replace_definitions(&project.tags)`.
- `undo`: pop undo, push current to redo, validate (must already be valid), increment document revision and autosave generation, replace simulation definitions.
- `redo`: inverse of undo, also using the 50-entry limit.
- `replace_project`: validate, clear undo and redo, replace project and simulation, increment both counters.
- `Update*Node`: require the node ID in the command to refer to an existing node; screen IDs cannot change through update.
- `DuplicateNode`: use `format!("{}.copy{N}", original.id)` starting at 1 until unused; if 1000 collisions, return `HmiError::InvalidCommand`. Offset 2D by `(24, 24)`; offset 3D `position.x += 0.5`. Append ` 副本` to the name.
- `DeleteNode`: unknown screen/node returns the matching error.
- `DeleteScreen`: reject when only one screen exists; if deleting the startup screen, require `new_startup` to name an existing remaining screen, otherwise use the first remaining screen.
- `RenameScreen`: non-empty name, maximum 120 chars.
- `SetStartupScreen`: screen must exist.
- `AddTag`: reject duplicate ID; `UpdateTag`: require existing ID and keep the ID immutable; `DeleteTag`: collect every referencing node ID and return `TagInUse` when non-empty.
- `SetBindings`: replace bindings on the selected node; validation catches missing tags and invalid target/mapping pairs.
- `write_tag`, `resume_tag_simulation`, `tick_tags` delegate to `TagSimulation` and never modify project revisions.
- `tick_tags` returns `true` only when at least one value changed.

Use this skeleton for the public API:

```rust
pub const UNDO_LIMIT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub affected_id: Option<String>,
    pub document_revision: u64,
}

pub struct EditorSession {
    project: ProjectDocument,
    undo: std::collections::VecDeque<ProjectDocument>,
    redo: Vec<ProjectDocument>,
    simulation: TagSimulation,
    document_revision: u64,
    autosave_generation: u64,
}

impl EditorSession {
    pub fn new(project: ProjectDocument) -> Result<Self, Vec<ValidationIssue>> {
        validate_project(&project)?;
        let simulation = TagSimulation::new(&project);
        Ok(Self {
            project, undo: Default::default(), redo: Vec::new(), simulation,
            document_revision: 1, autosave_generation: 0,
        })
    }

    pub fn project(&self) -> &ProjectDocument { &self.project }
    pub fn document_revision(&self) -> u64 { self.document_revision }
    pub fn autosave_generation(&self) -> u64 { self.autosave_generation }
    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }
    pub fn tag_snapshot(&self) -> &TagSnapshot { self.simulation.snapshot() }
    pub fn tick_tags(&mut self, dt_ms: u64) -> bool { self.simulation.tick(dt_ms) }
    pub fn write_tag(&mut self, tag_id: &str, value: TagValue) -> Result<(), HmiError> {
        self.simulation.write(tag_id, value)
    }
    pub fn resume_tag_simulation(&mut self, tag_id: &str) -> Result<(), HmiError> {
        self.simulation.resume(tag_id)
    }
}
```

- [ ] **Step 4: Run command tests**

```bash
cargo test -p hmi-core --test commands
```

Expected: PASS.

- [ ] **Step 5: Run all core tests and inspect undo behavior**

```bash
cargo test -p hmi-core
cargo fmt --check
```

Expected: PASS. Checkpoint: verify that a failed command does not increment either revision and does not consume an undo slot.

### Task 8: Persistence, import limits, and safe replacement

**Files:**
- Create: `crates/hmi-core/src/persistence.rs`
- Create: `crates/hmi-core/tests/persistence.rs`
- Modify: `crates/hmi-core/src/lib.rs`

**Interfaces:**
- Produces: `PersistenceError::{TooLarge, Json, Validation}`
- Produces: `to_json(project: &ProjectDocument) -> Result<String, PersistenceError>`
- Produces: `from_json(raw: &str) -> Result<ProjectDocument, PersistenceError>`
- Produces: `export_filename(project: &ProjectDocument) -> String`
- Produces: `STORAGE_KEY = "mox-hmi.project.v1"`
- Consumes: `validate_project`, `SCHEMA_VERSION`

- [ ] **Step 1: Write failing persistence tests**

`crates/hmi-core/tests/persistence.rs`:

```rust
use hmi_core::*;

#[test]
fn round_trip_preserves_the_demo() {
    let project = bas_demo();
    let json = to_json(&project).unwrap();
    let restored = from_json(&json).unwrap();
    assert_eq!(restored, project);
}

#[test]
fn rejects_wrong_schema_and_bad_binding_atomically() {
    let mut wrong_schema = serde_json::to_value(bas_demo()).unwrap();
    wrong_schema["schema_version"] = 2.into();
    assert!(matches!(
        from_json(&serde_json::to_string(&wrong_schema).unwrap()),
        Err(PersistenceError::Validation(_))
    ));

    let mut missing_tag = serde_json::to_value(bas_demo()).unwrap();
    missing_tag["screens"][0]["nodes"][0]["bindings"] = serde_json::json!([{
        "tag_id": "missing.tag",
        "target": "visible",
        "mapping": { "type": "direct" }
    }]);
    assert!(matches!(
        from_json(&serde_json::to_string(&missing_tag).unwrap()),
        Err(PersistenceError::Validation(_))
    ));
}

#[test]
fn rejects_unknown_fields_and_oversized_input() {
    assert!(matches!(from_json(r#"{"schema_version":1,"extra":true}"#), Err(PersistenceError::Json(_))));
    let oversized = "x".repeat(MAX_IMPORT_BYTES + 1);
    assert!(matches!(from_json(&oversized), Err(PersistenceError::TooLarge)));
}

#[test]
fn export_filename_uses_project_slug() {
    let mut project = bas_demo();
    assert_eq!(export_filename(&project), "bas-demo.moxhmi.json");
    project.id = "project.车站".into();
    project.name = "车站".into();
    assert_eq!(export_filename(&project), "project.moxhmi.json");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p hmi-core --test persistence
```

Expected: FAIL because persistence functions do not exist.

- [ ] **Step 3: Implement persistence**

`crates/hmi-core/src/persistence.rs`:

```rust
use crate::{
    validation::{validate_project, MAX_IMPORT_BYTES, ValidationIssue},
    ProjectDocument, SCHEMA_VERSION,
};
use thiserror::Error;

pub const STORAGE_KEY: &str = "mox-hmi.project.v1";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PersistenceError {
    #[error("工程文件超过 5 MB 限制")]
    TooLarge,
    #[error("JSON 解析失败：{0}")]
    Json(String),
    #[error("工程校验失败：{}", .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
    Validation(Vec<ValidationIssue>),
}

pub fn to_json(project: &ProjectDocument) -> Result<String, PersistenceError> {
    validate_project(project).map_err(PersistenceError::Validation)?;
    serde_json::to_string_pretty(project).map_err(|e| PersistenceError::Json(e.to_string()))
}

pub fn from_json(raw: &str) -> Result<ProjectDocument, PersistenceError> {
    if raw.len() > MAX_IMPORT_BYTES {
        return Err(PersistenceError::TooLarge);
    }
    let project: ProjectDocument = serde_json::from_str(raw).map_err(|e| PersistenceError::Json(e.to_string()))?;
    if project.schema_version != SCHEMA_VERSION {
        return Err(PersistenceError::Validation(vec![crate::validation::ValidationIssue {
            path: "schema_version".into(),
            message: format!("只支持 schema {SCHEMA_VERSION}"),
        }]));
    }
    validate_project(&project).map_err(PersistenceError::Validation)?;
    Ok(project)
}

pub fn export_filename(project: &ProjectDocument) -> String {
    let raw = project.id.strip_prefix("project.").unwrap_or(&project.id);
    let slug = raw.replace('.', "-");
    let mut clean = String::new();
    let mut last_dash = false;
    for ch in slug.chars() {
        if ch.is_ascii_alphanumeric() {
            clean.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !clean.is_empty() {
            clean.push('-');
            last_dash = true;
        }
    }
    while clean.ends_with('-') { clean.pop(); }
    if clean.is_empty() { clean.push_str("project"); }
    format!("{clean}.moxhmi.json")
}
```

- [ ] **Step 4: Run persistence tests**

```bash
cargo test -p hmi-core --test persistence
```

Expected: PASS.

- [ ] **Step 5: Run the full core suite**

```bash
cargo test -p hmi-core
cargo fmt --check
```

Expected: PASS. Checkpoint: import must validate before returning any replacement document; callers decide when to swap state.

### Task 9: Dioxus + Bevy WebGPU shell on one canvas

**Files:**
- Modify: `Cargo.toml`
- Create: `apps/studio/Cargo.toml`
- Create: `apps/studio/Dioxus.toml`
- Create: `apps/studio/src/main.rs`
- Create: `apps/studio/src/app.rs`
- Create: `apps/studio/src/bridge/mod.rs`
- Create: `apps/studio/src/scene/mod.rs`
- Create: `apps/studio/assets/main.css`

**Interfaces:**
- Produces: Dioxus component `App`
- Produces: `EngineStatus::{Starting, Ready, Failed(String)}`
- Produces: `bridge::ENGINE_STATUS: GlobalSignal<EngineStatus>`
- Produces: `bridge::mark_engine_ready()`, `bridge::mark_engine_failed(message)`
- Produces: `scene::start_bevy()`
- Produces: canvas id `bevy-canvas`
- Produces: test id `webgpu-status`

- [ ] **Step 1: Add the studio package and force the first build to fail**

Add `"apps/studio"` to the workspace members. `apps/studio/Cargo.toml`:

```toml
[package]
name = "mox-hmi-studio"
version.workspace = true
edition.workspace = true

[dependencies]
hmi-core = { path = "../../crates/hmi-core" }
bevy = { version = "0.20.1", default-features = false, features = [
  "2d",
  "3d",
  "webgpu",
  "web",
  "mesh_picking",
  "bevy_log",
  "default_font",
  "png",
] }
dioxus = { version = "0.7.10", default-features = false, features = [
  "minimal",
  "web",
  "asset",
  "document",
] }
console_error_panic_hook = "0.1"
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
web-sys = { version = "0.3", features = [
  "Window",
  "Location",
  "Navigator",
  "Gpu",
  "GpuAdapter",
] }
js-sys = "0.3"
gloo-timers = { version = "0.3", features = ["futures"] }
```

`apps/studio/src/main.rs` initially references `mod app;` and `mod bridge;` and `mod scene;`, but those files do not exist.

```bash
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
```

Expected: FAIL with missing modules.

- [ ] **Step 2: Implement the minimal shell**

`apps/studio/src/main.rs`:

```rust
mod app;
mod bridge;
mod scene;

fn main() {
    console_error_panic_hook::set_once();
    dioxus::launch(app::App);
}
```

`apps/studio/src/bridge/mod.rs` for this task:

```rust
use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus {
    Starting,
    Ready,
    Failed(String),
}

pub static ENGINE_STATUS: GlobalSignal<EngineStatus> = Signal::global(|| EngineStatus::Starting);

pub fn mark_engine_ready() {
    *ENGINE_STATUS.write() = EngineStatus::Ready;
}

pub fn mark_engine_failed(message: impl Into<String>) {
    *ENGINE_STATUS.write() = EngineStatus::Failed(message.into());
}
```

`apps/studio/src/app.rs`:

```rust
use crate::{bridge, scene};
use dioxus::prelude::*;
use wasm_bindgen_futures::JsFuture;

const MAIN_CSS: Asset = asset!("/assets/main.css");

#[component]
pub fn App() -> Element {
    let mut webgpu = use_signal(|| None::<bool>);
    let mut launched = use_signal(|| false);

    use_future(move || async move {
        webgpu.set(Some(detect_webgpu().await));
    });

    use_effect(move || {
        if webgpu() == Some(true) && !launched() {
            launched.set(true);
            scene::start_bevy();
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(8_000).await;
                if bridge::ENGINE_STATUS() == bridge::EngineStatus::Starting {
                    bridge::mark_engine_failed("Bevy 初始化超时");
                }
            });
        }
    });

    let engine = bridge::ENGINE_STATUS();
    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        div { class: "mox-shell", "data-testid": "app-shell",
            header { class: "mox-topbar",
                div { class: "brand-block",
                    div { class: "brand-mark", "M" }
                    div {
                        div { class: "brand-title", "MOX HMI Studio" }
                        div { class: "brand-subtitle", "车站 BAS 环控" }
                    }
                }
                div { class: "topbar-spacer" }
                div {
                    class: "webgpu-pill",
                    "data-testid": "webgpu-status",
                    match webgpu() {
                        None => "检测 WebGPU…".to_string(),
                        Some(true) => match engine {
                            bridge::EngineStatus::Ready => "WebGPU ready".to_string(),
                            bridge::EngineStatus::Failed(ref message) => format!("引擎失败：{message}"),
                            bridge::EngineStatus::Starting => "WebGPU · 初始化引擎".to_string(),
                        },
                        Some(false) => "WebGPU 不可用".to_string(),
                    }
                }
            }
            aside { class: "left-rail",
                div { class: "rail-kicker", "Workspace" }
                div { class: "rail-title", "车站环控" }
                div { class: "rail-note", "2D 工艺图 / 3D 数字孪生" }
            }
            main { class: "viewport-shell",
                div { class: "canvas-stage", "data-testid": "canvas-stage",
                    canvas { id: "bevy-canvas", class: "bevy-canvas" }
                }
            }
            aside { class: "right-rail",
                div { class: "rail-kicker", "Inspector" }
                div { class: "rail-title", "未选择对象" }
                div { class: "rail-note", "选择设备后显示属性与 Tag 绑定" }
            }
            footer { class: "bottom-bar",
                span { "Bevy 0.20.1 · Dioxus 0.7.10" }
                span { "WebGPU only · Chromium 120+" }
            }
            if webgpu() == Some(false) {
                div { class: "blocking-overlay", "data-testid": "webgpu-blocked",
                    div { class: "overlay-card",
                        h1 { "需要 WebGPU" }
                        p { "请使用 Chrome/Edge 120+，通过 HTTPS 或 localhost 打开，并确认 GPU 未被策略禁用。" }
                        button { onclick: move |_| { let _ = web_sys::window().unwrap().location().reload(); }, "重新加载" }
                    }
                }
            }
        }
    }
}

async fn detect_webgpu() -> bool {
    let Some(window) = web_sys::window() else { return false; };
    let gpu = window.navigator().gpu();
    match JsFuture::from(gpu.request_adapter()).await {
        Ok(value) => !value.is_null() && !value.is_undefined(),
        Err(_) => false,
    }
}
```

`apps/studio/src/scene/mod.rs` for this task:

```rust
use crate::bridge;
use bevy::prelude::*;
use wasm_bindgen_futures::spawn_local;

pub fn start_bevy() {
    spawn_local(async {
        run_bevy().await;
    });
}

async fn run_bevy() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            canvas: Some("#bevy-canvas".into()),
            fit_canvas_to_parent: true,
            prevent_default_event_handling: false,
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb(0.025, 0.04, 0.05)))
    .add_systems(Startup, (setup_scene, mark_ready))
    .run();
}

fn mark_ready() {
    info!("MOX HMI runtime ready");
    bridge::mark_engine_ready();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(5.0, 4.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.4, 1.4, 1.4))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.72, 0.67),
            metallic: 0.15,
            perceptual_roughness: 0.35,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.7, 0.0),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 1_800.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(3.0, 7.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
```

`apps/studio/Dioxus.toml`:

```toml
[application]
name = "mox-hmi-studio"
default_platform = "web"

[web.app]
title = "MOX HMI Studio"
```

- [ ] **Step 3: Add the layout CSS**

`apps/studio/assets/main.css` must define:

```css
:root {
  color-scheme: dark;
  font-family: "Inter", "Noto Sans SC", "PingFang SC", sans-serif;
  --bg: #071016;
  --panel: #0c171e;
  --panel-2: #101e27;
  --line: #1f3540;
  --text: #e8f2f4;
  --muted: #7f98a2;
  --cyan: #37e6d4;
  --cyan-dim: #164b4b;
  --amber: #f2b84b;
  --red: #ff4d5e;
}
* { box-sizing: border-box; }
html, body, #main { width: 100%; height: 100%; margin: 0; background: var(--bg); color: var(--text); }
body { overflow: hidden; }
.mox-shell {
  display: grid;
  grid-template-columns: 272px minmax(0, 1fr) 352px;
  grid-template-rows: 64px minmax(0, 1fr) 36px;
  gap: 1px;
  width: 100vw;
  height: 100vh;
  min-width: 1280px;
  min-height: 720px;
  background: var(--line);
}
.mox-topbar { grid-column: 1 / -1; grid-row: 1; display: flex; align-items: center; gap: 16px; padding: 0 18px; background: var(--panel); border-bottom: 1px solid var(--line); }
.left-rail { grid-column: 1; grid-row: 2; min-height: 0; overflow: hidden; padding: 18px; background: var(--panel); border-right: 1px solid var(--line); }
.right-rail { grid-column: 3; grid-row: 2; min-height: 0; overflow: hidden; padding: 18px; background: var(--panel); border-left: 1px solid var(--line); }
.viewport-shell { grid-column: 2; grid-row: 2; min-width: 0; min-height: 0; padding: 16px; background: #050b0f; }
.canvas-stage { position: relative; width: 100%; height: 100%; min-width: 0; min-height: 0; overflow: hidden; border: 1px solid var(--line); background: #071218; }
.bevy-canvas { width: 100%; height: 100%; display: block; outline: none; }
.bottom-bar { grid-column: 1 / -1; grid-row: 3; display: flex; justify-content: space-between; align-items: center; padding: 0 14px; background: var(--panel); border-top: 1px solid var(--line); color: var(--muted); font-size: 11px; }
.mox-shell.is-run { grid-template-columns: 0 minmax(0, 1fr) 0; grid-template-rows: 56px minmax(0, 1fr) 0; }
.mox-shell.is-run .left-rail, .mox-shell.is-run .right-rail, .mox-shell.is-run .bottom-bar { display: none; }
.brand-block { display: flex; align-items: center; gap: 10px; }
.brand-mark { width: 32px; height: 32px; display: grid; place-items: center; border: 1px solid var(--cyan); color: var(--cyan); font-weight: 800; }
.brand-title { font-size: 15px; font-weight: 700; letter-spacing: 0.04em; }
.brand-subtitle, .rail-note { color: var(--muted); font-size: 11px; }
.topbar-spacer { flex: 1; }
.webgpu-pill { border: 1px solid var(--line); background: var(--panel-2); padding: 7px 11px; border-radius: 999px; color: var(--muted); font-size: 11px; }
.rail-kicker { color: var(--cyan); font-size: 10px; letter-spacing: 0.12em; text-transform: uppercase; }
.rail-title { margin-top: 6px; font-size: 16px; font-weight: 650; }
.blocking-overlay { position: absolute; inset: 0; z-index: 50; display: grid; place-items: center; background: rgba(2, 7, 10, 0.86); }
.overlay-card { max-width: 520px; padding: 28px; border: 1px solid var(--line); background: var(--panel); }
.overlay-card h1 { margin-top: 0; }
```

The canvas is a normal child of the stable `main.viewport-shell > div.canvas-stage` grid cell; it is not a full-window absolute overlay. Do not remount or reparent `#bevy-canvas` when editor panels or modes change. The 2D camera, not the DOM, contains the 1600×900 design space with letterboxing.

- [ ] **Step 4: Build and run a browser smoke check**

```bash
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
cd apps/studio
dx build --web
```

Expected: PASS. Then run:

```bash
cd apps/studio
dx serve --web --port 8088 --open false > /tmp/mox-dx.log 2>&1 &
echo $! > /tmp/mox-dx.pid
```

Wait for the server, then run:

```bash
python3 - <<'PY'
import asyncio, json
from playwright.async_api import async_playwright

CHROME = "/tmp/chrome-dl/app/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"

async def main():
    logs = []
    async with async_playwright() as p:
        browser = await p.chromium.launch(executable_path=CHROME, headless=True, args=[
            "--enable-unsafe-webgpu", "--enable-features=WebGPU",
            "--ignore-gpu-blocklist", "--disable-gpu-sandbox",
        ])
        page = await browser.new_page(viewport={"width": 1440, "height": 900})
        page.on("console", lambda m: logs.append(f"{m.type}: {m.text}"))
        await page.goto("http://127.0.0.1:8088", wait_until="domcontentloaded")
        await page.get_by_test_id("webgpu-status").filter(has_text="WebGPU ready").wait_for(timeout=30_000)
        await page.screenshot(path="/tmp/mox-hmi-shell.png")
        result = {
            "gpu": await page.evaluate("!!navigator.gpu"),
            "ready": any("MOX HMI runtime ready" in line for line in logs),
            "webgpu_backend": any("BrowserWebGpu" in line for line in logs),
        }
        print(json.dumps(result, ensure_ascii=False, indent=2))
        await browser.close()
        assert all(result.values()), result

asyncio.run(main())
PY
kill "$(cat /tmp/mox-dx.pid)" || true
```

Expected: `gpu`, `ready`, and `webgpu_backend` all true. Inspect `/tmp/mox-hmi-shell.png` with `view_image`.

- [ ] **Step 5: Checkpoint**

```bash
cargo fmt --check
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
```

Expected: PASS. Do not continue until Bevy logs `BrowserWebGpu` and the Dioxus shell shares the same canvas.

### Task 10: Single EditorSession bridge, autosave, and 4 Hz tick

**Files:**
- Create: `apps/studio/src/bridge/storage.rs`
- Modify: `apps/studio/src/bridge/mod.rs`
- Modify: `apps/studio/src/app.rs`
- Modify: `apps/studio/src/scene/mod.rs`
- Modify: `apps/studio/Cargo.toml`

**Interfaces:**
- Produces signals: `DOCUMENT_REVISION`, `TAG_REVISION`, `SELECTED_NODE`, `ACTIVE_SCREEN`, `WORKSPACE_MODE`, `NOTICE_REVISION`, `ENGINE_STATUS`, `SAVE_STATUS`
- Produces: `initialize() -> Result<(), String>`
- Produces: `project() -> ProjectDocument`
- Produces: `tag_snapshot() -> TagSnapshot`
- Produces: `execute(command) -> Result<CommandResult, HmiError>`
- Produces: `undo() -> bool`, `redo() -> bool`, `can_undo() -> bool`, `can_redo() -> bool`
- Produces: `select_node(Option<String>)`, `select_screen(&str)`
- Produces: `set_workspace_mode(WorkspaceMode)`
- Produces: `tick_tags(dt_ms: u64)`
- Produces: `write_tag(tag_id: &str, value: TagValue) -> Result<(), HmiError>`
- Produces: `persist_now() -> Result<(), String>`
- Produces: `import_json(raw: &str) -> Result<(), String>`, `export_json() -> Result<String, String>`
- Produces: `notice() -> Option<Notice>`, `set_notice(level, message)`
- Produces: `view_center_2d() -> [f32; 2]`, `set_view_center_2d([f32; 2])`
- Produces: `WorkspaceMode::{Edit, Run}`
- Produces: `SaveStatus::{Saved, Pending, Failed}`
- Produces: `NoticeLevel::{Info, Warning, Error}`

- [ ] **Step 1: Add browser storage dependencies and write the adapter**

Extend `web-sys` features with `"Storage"`, and add `wasm-bindgen` already present.

`apps/studio/src/bridge/storage.rs`:

```rust
pub fn load_raw() -> Result<Option<String>, String> {
    let window = web_sys::window().ok_or_else(|| "window 不可用".to_string())?;
    let storage = window.local_storage().map_err(|e| format!("localStorage 不可用：{e:?}"))?
        .ok_or_else(|| "localStorage 不可用".to_string())?;
    storage.get_item(hmi_core::persistence::STORAGE_KEY)
        .map_err(|e| format!("读取 localStorage 失败：{e:?}"))
}

pub fn save_raw(raw: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| "window 不可用".to_string())?;
    let storage = window.local_storage().map_err(|e| format!("localStorage 不可用：{e:?}"))?
        .ok_or_else(|| "localStorage 不可用".to_string())?;
    storage.set_item(hmi_core::persistence::STORAGE_KEY, raw)
        .map_err(|e| format!("写入 localStorage 失败：{e:?}"))
}
```

- [ ] **Step 2: Extend the bridge**

`apps/studio/src/bridge/mod.rs` must contain the full signal/session implementation:

```rust
use dioxus::prelude::*;
use hmi_core::{
    persistence, CommandResult, EditorCommand, EditorSession, HmiError, ProjectDocument,
    TagSnapshot, TagValue,
};
use std::{cell::{Cell, RefCell}, rc::Rc};

pub mod storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMode { Edit, Run }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStatus { Saved, Pending, Failed }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel { Info, Warning, Error }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice { pub level: NoticeLevel, pub message: String }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus { Starting, Ready, Failed(String) }

pub static DOCUMENT_REVISION: GlobalSignal<u64> = Signal::global(|| 1);
pub static TAG_REVISION: GlobalSignal<u64> = Signal::global(|| 1);
pub static SELECTED_NODE: GlobalSignal<Option<String>> = Signal::global(|| None);
pub static ACTIVE_SCREEN: GlobalSignal<Option<String>> = Signal::global(|| None);
pub static WORKSPACE_MODE: GlobalSignal<WorkspaceMode> = Signal::global(|| WorkspaceMode::Edit);
pub static NOTICE_REVISION: GlobalSignal<u64> = Signal::global(|| 0);
pub static ENGINE_STATUS: GlobalSignal<EngineStatus> = Signal::global(|| EngineStatus::Starting);
pub static SAVE_STATUS: GlobalSignal<SaveStatus> = Signal::global(|| SaveStatus::Saved);

thread_local! {
    static SESSION: RefCell<Option<Rc<RefCell<EditorSession>>>> = const { RefCell::new(None) };
    static NOTICE: RefCell<Option<Notice>> = const { RefCell::new(None) };
    static VIEW_CENTER_2D: Cell<[f32; 2]> = const { Cell::new([800.0, 450.0]) };
}

pub fn initialize() -> Result<(), String> {
    SESSION.with(|slot| {
        if slot.borrow().is_some() { return Ok(()); }
        let (project, warning, needs_save) = match storage::load_raw() {
            Ok(Some(raw)) => match persistence::from_json(&raw) {
                Ok(project) => (project, None, false),
                Err(error) => (hmi_core::bas_demo(), Some(format!("自动保存工程无效，已加载内置示例：{error}")), true),
            },
            Ok(None) => (hmi_core::bas_demo(), None, true),
            Err(error) => (hmi_core::bas_demo(), Some(format!("localStorage 不可用，工程只保存在内存中：{error}")), true),
        };
        let session = EditorSession::new(project).map_err(|errors| {
            errors.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; ")
        })?;
        let startup = session.project().startup_screen.clone();
        *slot.borrow_mut() = Some(Rc::new(RefCell::new(session)));
        *DOCUMENT_REVISION.write() = 1;
        *TAG_REVISION.write() = 1;
        *ACTIVE_SCREEN.write() = Some(startup);
        *SAVE_STATUS.write() = if needs_save { SaveStatus::Pending } else { SaveStatus::Saved };
        if let Some(message) = warning {
            set_notice(NoticeLevel::Warning, message);
        }
        Ok(())
    })
}

fn with_session<T>(f: impl FnOnce(&mut EditorSession) -> T) -> Result<T, String> {
    SESSION.with(|slot| {
        let session = slot.borrow().as_ref().cloned().ok_or_else(|| "EditorSession 未初始化".to_string())?;
        let mut session = session.borrow_mut();
        Ok(f(&mut session))
    })
}

pub fn project() -> ProjectDocument {
    with_session(|session| session.project().clone()).unwrap_or_else(|_| hmi_core::bas_demo())
}

pub fn tag_snapshot() -> TagSnapshot {
    with_session(|session| session.tag_snapshot().clone()).unwrap_or_default()
}

pub fn document_revision() -> u64 { *DOCUMENT_REVISION.read() }
pub fn tag_revision() -> u64 { *TAG_REVISION.read() }

pub fn execute(command: EditorCommand) -> Result<CommandResult, HmiError> {
    let result = with_session(|session| session.execute(command))
        .map_err(HmiError::InvalidCommand)??;
    *DOCUMENT_REVISION.write() = result.document_revision;
    *TAG_REVISION.write() += 1;
    *SAVE_STATUS.write() = SaveStatus::Pending;
    if let Some(selected) = SELECTED_NODE() {
        if !project_contains_node(&project(), &selected) {
            *SELECTED_NODE.write() = None;
        }
    }
    Ok(result)
}

pub fn undo() -> bool {
    let changed = with_session(EditorSession::undo).unwrap_or(false);
    if changed { after_history_change(); }
    changed
}

pub fn redo() -> bool {
    let changed = with_session(EditorSession::redo).unwrap_or(false);
    if changed { after_history_change(); }
    changed
}

pub fn can_undo() -> bool {
    with_session(EditorSession::can_undo).unwrap_or(false)
}

pub fn can_redo() -> bool {
    with_session(EditorSession::can_redo).unwrap_or(false)
}

fn after_history_change() {
    let revision = with_session(EditorSession::document_revision).unwrap_or(1);
    *DOCUMENT_REVISION.write() = revision;
    *TAG_REVISION.write() += 1;
    *SAVE_STATUS.write() = SaveStatus::Pending;
    let selected = SELECTED_NODE();
    let project = project();
    *SELECTED_NODE.write() = selected.filter(|node_id| project_contains_node(&project, node_id));
}

pub fn select_node(node_id: Option<String>) {
    *SELECTED_NODE.write() = node_id;
}

pub fn selected_node() -> Option<String> { SELECTED_NODE() }

pub fn select_screen(screen_id: &str) {
    if project().screens.iter().any(|screen| screen.id() == screen_id) {
        *ACTIVE_SCREEN.write() = Some(screen_id.to_string());
        *SELECTED_NODE.write() = None;
    }
}

pub fn active_screen() -> Option<String> { ACTIVE_SCREEN() }

pub fn workspace_mode() -> WorkspaceMode { WORKSPACE_MODE() }
pub fn set_workspace_mode(mode: WorkspaceMode) { *WORKSPACE_MODE.write() = mode; }

pub fn tick_tags(dt_ms: u64) {
    let changed = with_session(|session| session.tick_tags(dt_ms)).unwrap_or(false);
    if changed { *TAG_REVISION.write() += 1; }
}

pub fn write_tag(tag_id: &str, value: TagValue) -> Result<(), HmiError> {
    let result = with_session(|session| session.write_tag(tag_id, value))
        .map_err(HmiError::InvalidCommand)?;
    if result.is_ok() { *TAG_REVISION.write() += 1; }
    result
}

pub fn persist_now() -> Result<(), String> {
    let json = export_json()?;
    match storage::save_raw(&json) {
        Ok(()) => { *SAVE_STATUS.write() = SaveStatus::Saved; Ok(()) }
        Err(error) => {
            *SAVE_STATUS.write() = SaveStatus::Failed;
            set_notice(NoticeLevel::Error, error.clone());
            Err(error)
        }
    }
}

pub fn export_json() -> Result<String, String> {
    let project = project();
    persistence::to_json(&project).map_err(|error| error.to_string())
}

pub fn import_json(raw: &str) -> Result<(), String> {
    let project = persistence::from_json(raw).map_err(|error| error.to_string())?;
    let startup = project.startup_screen.clone();
    with_session(|session| session.replace_project(project))
        .map_err(|errors| errors.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; "))??;
    let revision = with_session(EditorSession::document_revision).unwrap_or(2);
    *DOCUMENT_REVISION.write() = revision;
    *TAG_REVISION.write() += 1;
    *ACTIVE_SCREEN.write() = Some(startup);
    *SELECTED_NODE.write() = None;
    *SAVE_STATUS.write() = SaveStatus::Pending;
    set_notice(NoticeLevel::Info, "工程导入成功");
    Ok(())
}

pub fn mark_engine_ready() { *ENGINE_STATUS.write() = EngineStatus::Ready; }
pub fn mark_engine_failed(message: impl Into<String>) { *ENGINE_STATUS.write() = EngineStatus::Failed(message.into()); }

pub fn set_notice(level: NoticeLevel, message: impl Into<String>) {
    NOTICE.with(|slot| *slot.borrow_mut() = Some(Notice { level, message: message.into() }));
    *NOTICE_REVISION.write() += 1;
}

pub fn notice() -> Option<Notice> {
    let _ = NOTICE_REVISION();
    NOTICE.with(|slot| slot.borrow().clone())
}

pub fn view_center_2d() -> [f32; 2] { VIEW_CENTER_2D.with(Cell::get) }
pub fn set_view_center_2d(center: [f32; 2]) { VIEW_CENTER_2D.with(|cell| cell.set(center)); }

pub fn project_contains_node(project: &ProjectDocument, node_id: &str) -> bool {
    project.screens.iter().any(|screen| match screen {
        hmi_core::Screen::TwoD { nodes, .. } => nodes.iter().any(|node| node.id == node_id),
        hmi_core::Screen::ThreeD { nodes, .. } => nodes.iter().any(|node| node.id == node_id),
    })
}
```

Keep the compiler clean; the bridge exposes only the symbols used by UI and Bevy.

- [ ] **Step 3: Initialize the bridge and add 500 ms autosave**

In `App`, before rendering:

```rust
use_hook(|| {
    if let Err(error) = bridge::initialize() {
        bridge::set_notice(bridge::NoticeLevel::Error, error);
    }
});

use_effect(move || {
    let revision = *bridge::DOCUMENT_REVISION.read();
    if bridge::SAVE_STATUS() != bridge::SaveStatus::Pending {
        return;
    }
    spawn(async move {
        gloo_timers::future::TimeoutFuture::new(500).await;
        if *bridge::DOCUMENT_REVISION.read() == revision
            && bridge::SAVE_STATUS() == bridge::SaveStatus::Pending
            && let Err(error) = bridge::persist_now()
        {
            eprintln!("MOX HMI autosave failed: {error}");
        }
    });
});
```

Add a visible save-state pill so the behavior is observable, with test id `save-status`.

- [ ] **Step 4: Add the 4 Hz tick system to Bevy**

In `scene/mod.rs`:

```rust
#[derive(Default)]
struct SimulationAccumulator(f64);

pub(crate) fn tick_simulation(time: Res<Time>, mut accumulator: Local<SimulationAccumulator>) {
    accumulator.0 += time.delta_secs_f64();
    let steps = (accumulator.0 / 0.25).floor() as u64;
    if steps > 0 {
        accumulator.0 -= steps as f64 * 0.25;
        bridge::tick_tags(steps * 250);
    }
}
```

Do not register this system in `scene/mod.rs`; `HmiScenePlugin` in Task 11 registers it exactly once.

- [ ] **Step 5: Build and verify persistence manually**

```bash
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
cd apps/studio && dx build --web
```

Start the app, wait for ready, then use Playwright:

```python
await page.evaluate("localStorage.clear()")
await page.reload()
await page.get_by_test_id("webgpu-status").filter(has_text="WebGPU ready").wait_for(timeout=30_000)
await page.wait_for_timeout(1_000)
stored = await page.evaluate("localStorage.getItem('mox-hmi.project.v1')")
assert stored and '"schema_version": 1' in stored
```

Expected: PASS. Checkpoint: a fresh project is autosaved once after initialization, and Tag runtime values never appear in that JSON. A valid project loaded from localStorage must not be rewritten until an edit occurs.

### Task 11: 2D geometry, fonts, rendering, and visual patch application

**Files:**
- Create: `crates/hmi-core/src/geometry.rs`
- Create: `crates/hmi-core/tests/geometry.rs`
- Modify: `crates/hmi-core/src/lib.rs`
- Create: `apps/studio/src/scene/plugin.rs`
- Create: `apps/studio/src/scene/two_d.rs`
- Create: `apps/studio/src/scene/assets.rs`
- Modify: `apps/studio/src/scene/mod.rs`
- Create: `apps/studio/public/assets/fonts/NotoSansSC-Subset.otf`
- Create: `apps/studio/public/assets/fonts/OFL.txt`
- Create: `scripts/build_font_subset.py`
- Modify: `apps/studio/src/app.rs`

**Interfaces:**
- Produces: `design_to_world(point: [f32; 2], design_size: [u32; 2]) -> [f32; 3]`
- Produces: `world_to_design(point: [f32; 2], design_size: [u32; 2]) -> [f32; 2]`
- Produces: `point_in_rotated_rect(point: [f32; 2], rect: Rect, rotation_deg: f32) -> bool`
- Produces: `point_near_polyline(point: [f32; 2], points: &[[f32; 2]], tolerance: f32) -> bool`
- Produces Bevy plugin `HmiScenePlugin`
- Produces resources `CachedProject`, `HmiFont`, `TwoDVisualIndex`
- Produces components `HmiNodeRoot`, `HmiPatchTarget`, `HmiRotor`, `HmiSpin`, `RotorAxis`
- Produces systems `sync_scene`, `apply_visual_patches`, `spin_rotors`, `tick_simulation`
- Test IDs unaffected; canvas remains `#bevy-canvas`

- [ ] **Step 1: Write failing geometry tests**

`crates/hmi-core/tests/geometry.rs`:

```rust
use hmi_core::*;

#[test]
fn design_world_round_trip() {
    for point in [[0.0, 0.0], [800.0, 450.0], [1600.0, 900.0]] {
        let world = design_to_world(point, [1600, 900]);
        let back = world_to_design([world[0], world[1]], [1600, 900]);
        assert!((back[0] - point[0]).abs() < 0.001);
        assert!((back[1] - point[1]).abs() < 0.001);
    }
    assert_eq!(design_to_world([800.0, 450.0], [1600, 900]), [0.0, 0.0, 0.0]);
}

#[test]
fn rotated_rect_hit_test_respects_rotation() {
    let rect = Rect { x: 100.0, y: 100.0, width: 200.0, height: 80.0 };
    assert!(point_in_rotated_rect([200.0, 140.0], rect, 0.0));
    assert!(!point_in_rotated_rect([200.0, 10.0], rect, 0.0));
    assert!(point_in_rotated_rect([200.0, 60.0], rect, 90.0));
    assert!(!point_in_rotated_rect([200.0, 180.0], rect, 90.0));
}

#[test]
fn polyline_hit_test_accepts_segments_and_joints() {
    let points = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]];
    assert!(point_near_polyline([50.0, 3.0], &points, 4.0));
    assert!(point_near_polyline([100.0, 50.0], &points, 4.0));
    assert!(!point_near_polyline([50.0, 30.0], &points, 4.0));
}
```

- [ ] **Step 2: Run geometry tests to verify failure**

```bash
cargo test -p hmi-core --test geometry
```

Expected: FAIL because geometry functions do not exist.

- [ ] **Step 3: Implement geometry**

Exact conversions:

```rust
pub fn design_to_world(point: [f32; 2], design_size: [u32; 2]) -> [f32; 3] {
    [
        point[0] - design_size[0] as f32 * 0.5,
        design_size[1] as f32 * 0.5 - point[1],
        0.0,
    ]
}

pub fn world_to_design(point: [f32; 2], design_size: [u32; 2]) -> [f32; 2] {
    [
        point[0] + design_size[0] as f32 * 0.5,
        design_size[1] as f32 * 0.5 - point[1],
    ]
}
```

`point_in_rotated_rect` transforms the point into local rectangle coordinates around the rectangle center using `-rotation_deg`, then checks half extents. `point_near_polyline` checks distance to every segment and returns false for fewer than two points. Export functions from `lib.rs`.

- [ ] **Step 4: Build an OFL Chinese font subset**

Create `scripts/build_font_subset.py`. It downloads the source font and OFL license, scans every `.rs` file under `crates` and `apps/studio/src`, and writes only the glyphs needed by the current sources:

```python
#!/usr/bin/env python3
from pathlib import Path
import subprocess
import sys
import urllib.request

from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
SRC = Path("/tmp/mox-font/NotoSansSC-Regular.otf")
OUT = ROOT / "apps/studio/public/assets/fonts/NotoSansSC-Subset.otf"
LICENSE = ROOT / "apps/studio/public/assets/fonts/OFL.txt"
FONT_URL = "https://raw.githubusercontent.com/notofonts/noto-cjk/main/Sans/OTF/SimplifiedChinese/NotoSansSC-Regular.otf"
LICENSE_URL = "https://raw.githubusercontent.com/notofonts/noto-cjk/main/LICENSE"

OUT.parent.mkdir(parents=True, exist_ok=True)
SRC.parent.mkdir(parents=True, exist_ok=True)
if not SRC.exists():
    urllib.request.urlretrieve(FONT_URL, SRC)
if not LICENSE.exists():
    urllib.request.urlretrieve(LICENSE_URL, LICENSE)

chars = {chr(cp) for cp in range(0x20, 0x7F)}
chars.update("·°℃²₂—–≤≥→←↑↓")
for base in [ROOT / "crates", ROOT / "apps/studio/src"]:
    for path in base.rglob("*.rs"):
        chars.update(ch for ch in path.read_text(encoding="utf-8") if ch.isprintable())
chars = {ch for ch in chars if ch.isprintable()}

text_file = Path("/tmp/mox-font/chars.txt")
text_file.write_text("".join(sorted(chars)), encoding="utf-8")
subprocess.run([
    sys.executable, "-m", "fontTools.subset", str(SRC),
    f"--text-file={text_file}",
    f"--output-file={OUT}",
    "--layout-features=*",
    "--no-hinting",
    "--desubroutinize",
], check=True)

cmap = set(TTFont(OUT).getBestCmap())
missing = sorted(ch for ch in chars if ord(ch) not in cmap)
if missing:
    raise SystemExit(f"missing glyphs: {''.join(missing)}")
if OUT.stat().st_size >= 1_500_000:
    raise SystemExit(f"font subset too large: {OUT.stat().st_size} bytes")
print(f"font subset: {OUT.stat().st_size} bytes, {len(chars)} chars")
```

Run it:

```bash
python3 -m pip install --user fonttools
python3 scripts/build_font_subset.py
```

Expected: no missing glyphs; target under 1 MB and hard-fail at 1.5 MB. Do not ship the 10+ MB source font.

This Task 11 run is provisional because Chinese UI strings are still being added. Regenerate the same asset in Task 17 after every UI source file exists, then rebuild the WASM so `include_bytes!` contains the final subset.

- [ ] **Step 5: Implement scene caching, font embedding, and 2D rendering**

`apps/studio/src/scene/assets.rs`:

```rust
use bevy::prelude::*;

#[derive(Resource)]
pub struct HmiFont(pub Handle<Font>);

pub fn load_font(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let handle = fonts.add(Font::from_bytes(include_bytes!(
        "../../public/assets/fonts/NotoSansSC-Subset.otf"
    ).to_vec()));
    commands.insert_resource(HmiFont(handle));
}
```

`apps/studio/src/scene/plugin.rs`:

```rust
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct CachedProject {
    pub revision: u64,
    pub screen_id: Option<String>,
}

#[derive(Resource, Default)]
pub struct TwoDVisualIndex {
    pub nodes: std::collections::HashMap<String, NodeVisual>,
}

pub struct NodeVisual {
    pub root: Entity,
    pub text: Option<Entity>,
    pub materials: Vec<Handle<ColorMaterial>>,
    pub progress: Option<Entity>,
    pub rotors: Vec<Entity>,
}

impl NodeVisual {
    pub fn new(root: Entity) -> Self {
        Self { root, text: None, materials: Vec::new(), progress: None, rotors: Vec::new() }
    }
}

#[derive(Component, Clone, Copy)]
pub enum RotorAxis { LocalY, LocalZ }

#[derive(Component)]
pub struct HmiRotor {
    pub axis: RotorAxis,
}

#[derive(Component, Default)]
pub struct HmiSpin {
    pub rad_per_sec: f32,
}
```

`apps/studio/src/scene/mod.rs` must register the plugin after `DefaultPlugins`:

```rust
use plugin::HmiScenePlugin;

// inside run_bevy
.add_plugins(HmiScenePlugin)
```

Remove the temporary `setup_scene` added in Task 9. Keep `mark_ready`. The plugin, not `scene/mod.rs`, registers `crate::scene::tick_simulation`.


The plugin must implement:

- Startup: create `HmiSceneRoot`, load font, spawn the 2D camera.
- Update: `sync_scene` reads `ACTIVE_SCREEN`, `DOCUMENT_REVISION`, and `WORKSPACE_MODE`; when the screen or document changes, despawn all children of `HmiSceneRoot`, clear both visual indexes, and rebuild the active screen.
- When no screen is active, show the demo background and no nodes.
- `apply_visual_patches` runs when `TAG_REVISION` changes. It resolves every binding of every visible node using `resolve_bindings` and the current `TagSnapshot`, then applies patch values through `TwoDVisualIndex` or the 3D index.
- A rebuild must apply the current Tag snapshot before the frame ends. Reset the patch cache after every rebuild, or call the patch-application helper directly from `sync_scene`; otherwise a structural edit can leave a fresh scene with unpatched default colors until a Tag value happens to change.
- Plugin systems run in this order: `tick_simulation`, `sync_scene`, `apply_visual_patches`, `spin_rotors`. Register them exactly once with `.add_systems(Update, (crate::scene::tick_simulation, sync_scene, apply_visual_patches, spin_rotors).chain())`.
- `spin_rotors` rotates each `HmiRotor` around its declared local axis by `HmiSpin.rad_per_sec * time.delta_secs()`. Axis choice is data, never inferred from device type at runtime.

2D rendering rules:

- Camera: `Camera2d` with `Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::AutoMin { min_width: 1600.0, min_height: 900.0 }, ..OrthographicProjection::default_2d() })`, centered at world origin. `AutoMin` guarantees the entire 1600×900 design is visible with letterboxing at every aspect ratio; do not use `FixedVertical`, which crops horizontally on 16:10 and wider viewports.
- Screen background drives `ClearColor`.
- Each `UiNode` gets one root entity with `HmiNodeRoot { node_id, rect }`; children inherit the root transform.
- Node root translation uses the rectangle center converted by `design_to_world`; Z uses the node `z`; rotation uses `Quat::from_rotation_z(-rotation_deg.to_radians())`.
- Panel: filled `Rectangle`, optional stroked border using four thin rectangles.
- Label: one `Text2d`, font size, color, and alignment.
- ValueCard: panel + label text + a `Text2d` placeholder `--` registered as the node's patch text.
- StatusLamp: rounded panel + circle + label; circle material is registered for color patches.
- BarGauge: panel, track, fill rectangle, numeric text. The fill entity is registered as the progress target; progress `0..1` controls its X scale and anchor.
- Button: panel + centered `Text2d`; store `HmiButtonAction` on the node root.
- Pipe: `points` are local design pixels relative to the node rect origin. Spawn one thin rotated rectangle per local segment plus joint circles; the root rect is only for selection, and hit-testing subtracts `rect.x/rect.y` before testing local points.
- EquipmentSymbol:
  - Fan: body rectangle + circle + three blade rectangles + label; the blades get `HmiRotor { axis: RotorAxis::LocalZ }`.
  - Pump: body rectangle + circle + impeller line + label; the impeller gets `HmiRotor { axis: RotorAxis::LocalZ }`.
  - Ahu: large rectangle + two internal fan circles + two divider lines + label; both fan blade groups get `HmiRotor { axis: RotorAxis::LocalZ }`.
  - Valve: diamond/rotated square + center line + label.
- Every entity that carries `HmiRotor` is registered in `NodeVisual.rotors`.
- Every material that should change with `Color` patches is stored in `NodeVisual.materials`.
- Text entities that receive `Text` patches are stored in `NodeVisual.text`.
- All runtime text uses the embedded `HmiFont`; when the font resource is absent, log a warning and use `TextFont::default()`.
- Bevy 0.20 text styling uses `TextFont::from_font_size(size).with_font(font_handle.clone())` and `TextColor(color)`; do not assign `f32` directly to the `font_size` field.

Use a single helper for material creation:

```rust
fn mat(
    materials: &mut Assets<ColorMaterial>,
    color: Color,
) -> Handle<ColorMaterial> {
    materials.add(ColorMaterial::from_color(color))
}
```

- [ ] **Step 6: Apply and verify visual patches**

For every `Color` patch, update all stored materials to the patch color. For `Text`, update the stored `Text2d`. For `Visible`, set root `Visibility`. For `Progress`, update the fill entity's transform. For `RotationSpeed`, set `HmiSpin.rad_per_sec` on every entity in `NodeVisual.rotors`; the shared `spin_rotors` system owns the actual rotation.

Add a startup log with the active screen:

```rust
info!("MOX HMI scene: screen={screen_id}");
```

Then run:

```bash
cargo test -p hmi-core --test geometry
cargo test -p hmi-core
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
cd apps/studio && dx build --web
```

Expected: PASS. Start the app and inspect `/tmp/mox-hmi-2d.png` at 1440×900. The BAS 2D screen must show the title, AHU, fans, pumps, valves, pipes, four value cards, two status lamps, gauge, and button. The text must not show tofu glyphs.

- [ ] **Step 7: Checkpoint**

Verify that waiting 1 second changes at least one bound value card and that no scene rebuild is triggered by Tag-only updates. This is observable by a stable camera and no `MOX HMI scene:` log repetition.

### Task 12: 2D selection, drag, resize, pan, zoom, and keyboard editing

**Files:**
- Create: `apps/studio/src/scene/two_d_edit.rs`
- Modify: `apps/studio/src/scene/plugin.rs`
- Modify: `apps/studio/src/scene/two_d.rs`
- Modify: `apps/studio/src/app.rs`
- Modify: `apps/studio/src/bridge/mod.rs`
- Modify: `crates/hmi-core/src/geometry.rs`
- Modify: `crates/hmi-core/src/lib.rs`
- Modify: `crates/hmi-core/tests/geometry.rs`

**Interfaces:**
- Produces in `hmi_core`: `resize_rect(start: Rect, corner: ResizeCorner, delta: [f32; 2], min_size: f32) -> Rect`
- Produces in `hmi_core`: `ResizeCorner::{TopLeft, TopRight, BottomLeft, BottomRight}`
- Produces resources `TwoDEditState`, `TwoDCameraState { zoom: f32 }`
- Produces shared hit-test helper used by both Edit and Run 2D input
- Produces `bridge::set_zoom_2d`, `bridge::zoom_2d`
- Consumes `bridge::project()`, `bridge::execute`, `bridge::select_node`, `bridge::view_center_2d`
- Produces test IDs indirectly through the canvas and inspector

- [ ] **Step 1: Add failing resize and hit-test tests**

Append to `crates/hmi-core/tests/geometry.rs`:

```rust
#[test]
fn resize_rect_clamps_to_min_size() {
    let start = Rect { x: 100.0, y: 100.0, width: 200.0, height: 120.0 };
    let moved = resize_rect(start, ResizeCorner::BottomRight, [30.0, 20.0], 16.0);
    assert_eq!(moved, Rect { x: 100.0, y: 100.0, width: 230.0, height: 140.0 });
    let clamped = resize_rect(start, ResizeCorner::TopLeft, [500.0, 500.0], 16.0);
    assert_eq!(clamped.width, 16.0);
    assert_eq!(clamped.height, 16.0);
}

#[test]
fn rotated_rect_hit_test_matches_expected_corners() {
    let rect = Rect { x: 100.0, y: 100.0, width: 200.0, height: 100.0 };
    assert!(point_in_rotated_rect([100.0, 100.0], rect, 0.0));
    assert!(point_in_rotated_rect([300.0, 200.0], rect, 0.0));
    assert!(!point_in_rotated_rect([100.0, 200.5], rect, 0.0));
}
```

- [ ] **Step 2: Run tests to verify failure**

```bash
cargo test -p hmi-core --test geometry
```

Expected: FAIL because `resize_rect` and `ResizeCorner` do not exist.

- [ ] **Step 3: Implement resize math**

Implement and export `ResizeCorner` and `resize_rect` from `crates/hmi-core/src/geometry.rs`; re-export both from `crates/hmi-core/src/lib.rs`. Do not define a second copy in `two_d_edit.rs`.

`resize_rect` adjusts the dragged corner while keeping the opposite corner fixed. Enforce `min_size` in both axes, normalize negative-width/height cases, and reject non-finite inputs. Invalid input returns a rect with `min_size` dimensions.

- [ ] **Step 4: Implement the editor input state machine**

`apps/studio/src/scene/two_d_edit.rs` must implement these exact behaviors:

```rust
use hmi_core::ResizeCorner;

#[derive(Resource, Default)]
pub struct TwoDEditState {
    drag: Option<DragState>,
}

#[derive(Debug, Clone)]
enum DragState {
    Move { node_id: String, start: [f32; 2], original: Rect, changed: bool },
    Resize { node_id: String, corner: ResizeCorner, start: [f32; 2], original: Rect, preview: Rect },
}
```

Input rules:

- Ignore all 2D canvas input when `WORKSPACE_MODE == Run` or the active screen is not `Screen::TwoD`; the Task 16 runtime handler owns Run and the 3D orbit system owns 3D.
- Convert the cursor to design coordinates through the active `Camera2d` with `viewport_to_world_2d`, then `world_to_design`.
- Extract one `hit_test_2d_node(node, point, zoom)` helper and use it from both Edit and Run input. It skips the full-screen demo background `node.bg`, uses `point_near_polyline` with tolerance `max(thickness / 2, 8 / zoom)` for pipes, and `point_in_rotated_rect` for all other nodes. This guarantees that clicking empty space clears selection instead of selecting the background.
- On left press:
  1. If a node is selected and the cursor is within `12 / zoom` design units of a transformed corner handle, start Resize.
  2. Otherwise hit-test nodes by descending `z` with the shared helper.
  3. Select the hit node through `bridge::select_node(Some(id))`.
  4. Start Move only when the hit node is unlocked and the project is in Edit mode.
  5. Clicking empty space clears selection.
- On cursor move during Move: update the root `Transform` immediately, clamp `x` to `0..=1600 - width` and `y` to `0..=900 - height`, and set `changed = true` when movement exceeds 0.5 design units.
- On cursor move during Resize: compute the preview rect with `resize_rect`, update the selection gizmo preview, and do not mutate the project.
- On left release:
  - Move commits one `EditorCommand::Update2dNode` with the final rect.
  - Resize commits one `EditorCommand::Update2dNode` with the preview rect.
  - If no movement occurred, do not create an undo step.
- Middle mouse drag or Space + left drag pans the camera. Preserve zoom.
- Zoom state starts at `1.0`; clamp `zoom` to `0.5..=4.0`. Write the Bevy projection as `projection.scale = 1.0 / zoom`, because Bevy orthographic `scale` grows as objects appear smaller.
- Wheel zoom is cursor-anchored. Before changing zoom, read the world point under the cursor. Compute `old_scale` and `new_scale`; after setting `zoom`, move the camera translation by `(anchor_world - camera_center) * (1.0 - new_scale / old_scale)` on X/Y. This keeps the same world point under the cursor even though Bevy's camera projection is recomputed in `PostUpdate`.
- `F` focuses the selected node: center the camera on its rect and set zoom so the node occupies about 50% of the viewport, clamped to the same range.
- After every pan, zoom, or focus change, call `bridge::set_zoom_2d(zoom)`; Step 5 publishes the new design-space viewport center.
- `Delete` and `Backspace` are handled by the Dioxus root, not Bevy, to avoid double deletion.

Selection overlay:

- Use `Gizmos` with `Rect2d` for the selection border and small circles for four handles.
- Draw a grid only in Edit mode: 50 design-unit minor lines and 200-unit major lines, low alpha.
- Never draw gizmos in Run mode.

- [ ] **Step 5: Publish the viewport center and zoom**

Add the exact helper in `two_d_edit.rs`; Bevy 0.20 returns `Result` from `viewport_to_world_2d`, so use `.ok()?`:

```rust
fn view_center_design(camera: &Camera, transform: &GlobalTransform) -> Option<[f32; 2]> {
    let viewport_center = camera.logical_viewport_size()? * 0.5;
    let world = camera.viewport_to_world_2d(transform, viewport_center).ok()?;
    let design = world_to_design([world.x, world.y], [1600, 900]);
    Some([
        design[0].clamp(0.0, 1600.0),
        design[1].clamp(0.0, 900.0),
    ])
}

fn publish_view_state(
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    camera_state: Res<TwoDCameraState>,
) {
    let Ok((camera, transform)) = camera_query.single() else { return; };
    if let Some(center) = view_center_design(camera, transform) {
        bridge::set_view_center_2d(center);
    }
    bridge::set_zoom_2d(camera_state.zoom);
}
```

Run `publish_view_state` every frame after 2D camera input. It also establishes the initial value on startup. Add the `ZOOM_2D: Cell<f32>` storage and `set_zoom_2d` / `zoom_2d` helpers to `bridge/mod.rs`; do not turn zoom into a reactive signal.

- [ ] **Step 6: Add Dioxus keyboard shortcuts with input guards**

In `app.rs`, make the app root focusable:

```rust
div {
    class: "mox-shell",
    tabindex: "0",
    onkeydown: move |event| {
        if event.key() == Key::Delete || event.key() == Key::Backspace {
            if let Some(node_id) = bridge::selected_node()
                && let Some(screen_id) = bridge::active_screen()
            {
                let _ = bridge::execute(hmi_core::EditorCommand::DeleteNode { screen_id, node_id });
            }
        } else if event.modifiers().ctrl() && event.key() == Key::Character("z".into()) {
            bridge::undo();
        } else if event.modifiers().ctrl() && event.modifiers().shift() && event.key() == Key::Character("z".into()) {
            bridge::redo();
        }
    },
}
```

Every inspector `<input>` and `<select>` must stop key propagation, so Delete/Backspace typing does not delete nodes:

```rust
onkeydown: move |event| event.stop_propagation(),
```

- [ ] **Step 7: Build and exercise the interactions**

```bash
cargo test -p hmi-core --test geometry
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
cd apps/studio && dx build --web
```

Expected: PASS. In a browser at 1440×900:

1. Click the AHU; the inspector selection ID becomes `node.ahu`.
2. Drag it 100 px; the inspector X changes once after release.
3. Undo restores the original position.
4. Drag a corner; width/height change once after release.
5. Wheel zoom and middle-drag pan do not change the project JSON.
6. Delete removes the selected node; undo restores it.

Checkpoint: no project command is emitted during pointer movement or wheel input.

### Task 13: 3D digital twin, orbit camera, picking, and GLB placeholder

**Files:**
- Create: `apps/studio/src/scene/three_d.rs`
- Create: `apps/studio/src/scene/orbit.rs`
- Modify: `apps/studio/src/scene/plugin.rs`
- Modify: `apps/studio/src/scene/mod.rs`
- Modify: `apps/studio/src/scene/assets.rs`
- Modify: `apps/studio/src/bridge/mod.rs`
- Modify: `apps/studio/src/app.rs`
- Modify: `apps/studio/assets/main.css`
- Modify: `apps/studio/Cargo.toml`

**Interfaces:**
- Produces components `HmiPickable { node_id }`, `DeviceLabel3D`, `HmiGlbPending`, `SelectionRing`
- Reuses `HmiRotor`, `HmiSpin`, `RotorAxis` defined in Task 11
- Produces resources `OrbitCamera { yaw, pitch, distance, focus }`, `HmiCameras`, `ThreeDVisualIndex`
- Produces `bridge::SceneLabel3D { node_id, title, value, left_pct, top_pct, visible }`, `bridge::VIEW_LABELS_3D`, `set_scene_labels_3d`, `clear_scene_labels_3d`
- Consumes `DeviceNode`, `DeviceModel`, `resolve_bindings`
- Produces logging `MOX HMI scene: screen=screen.bas_3d`

- [ ] **Step 1: Build the procedural devices**

Implement `spawn_device(commands, meshes, materials, node) -> Entity` with these mandatory models:

- `Ahu`: main cuboid + top panel + two circular fan housings + four feet.
- `Fan`: housing cuboid + circular inlet + three thin rotor blades + center hub.
- `Pump`: base + cylindrical body + impeller rotor + outlet pipe.
- `Duct`: horizontal cylinder or elongated cuboid with flanges.
- `Pipe`: thin cylinder with end rings.
- `Valve`: body box + wheel torus + stem cylinder.
- `Glb`: spawn a wireframe placeholder cuboid immediately, then load the first glTF scene with a typed `Handle<WorldAsset>`. Bevy 0.20 renamed the old `SceneRoot` component to `WorldAssetRoot` and requires the `#Scene0` label; do not load the unlabeled `.glb` handle.

```rust
use bevy::{
    gltf::prelude::GltfAssetLabel,
    world_serialization::prelude::{WorldAsset, WorldAssetRoot},
};

#[derive(Component)]
pub struct HmiGlbPending {
    pub path: String,
    pub handle: Handle<WorldAsset>,
    pub placeholder: Entity,
}

let handle: Handle<WorldAsset> = asset_server.load(
    GltfAssetLabel::Scene(0).from_asset(path.clone())
);
commands.spawn((
    WorldAssetRoot(handle.clone()),
    HmiGlbPending { path, handle, placeholder },
));
```

Add a settlement system that checks `asset_server.load_state(&pending.handle)`. On `LoadState::Loaded`, despawn the placeholder and remove `HmiGlbPending`. On `LoadState::Failed(_)`, log the path once, remove `HmiGlbPending`, and leave the placeholder visible. Do not call `load_state(handle.id())`; pass the typed handle reference.

In `HmiScenePlugin::build`, add `MeshPickingPlugin` explicitly. `DefaultPlugins` does not add the mesh picking backend for this custom feature set:

```rust
use bevy::picking::mesh_picking::MeshPickingPlugin;

// app.add_plugins(MeshPickingPlugin)
```

Every pickable child gets `HmiPickable { node_id }` and a pointer observer:

```rust
commands.spawn((
    Mesh3d(mesh),
    MeshMaterial3d(material),
    Transform::default(),
    HmiPickable { node_id: node.id.clone() },
)).observe(on_device_click);
```

```rust
fn on_device_click(click: On<PointerClick>, pickables: Query<&HmiPickable>) {
    if let Ok(pickable) = pickables.get(click.entity) {
        crate::bridge::select_node(Some(pickable.node_id.clone()));
    }
}
```

- [ ] **Step 2: Implement the orbit camera**

Replace Task 11's 2D-only camera setup with `setup_cameras`, which spawns both a `Camera2d` and a `Camera3d` and inserts:

```rust
#[derive(Resource)]
pub struct HmiCameras {
    pub two_d: Entity,
    pub three_d: Entity,
}
```

The cameras are persistent engine resources and are never children of `HmiSceneRoot`. Add `sync_active_camera` to read the active screen and set `Camera.is_active` so exactly one camera renders: 2D camera for `Screen::TwoD`, 3D camera for `Screen::ThreeD`. Scene rebuilds despawn picture entities, not cameras. The 3D input systems must no-op unless the active screen is `Screen::ThreeD`.

`OrbitCamera` initial state: `yaw = -0.65`, `pitch = 0.48`, `distance = 13.0`, `focus = Vec3::new(0.0, 1.4, 0.0)`. Limits: `distance 6..=24`, `pitch -0.15..=1.25`.

- Left drag orbits; only while the 3D camera is active.
- Middle drag pans along the camera's right/up vectors.
- Wheel zooms by scaling distance exponentially.
- Apply transform every frame with `looking_at(focus, Vec3::Y)`.
- A toolbar button in the inspector can reset the camera to the initial values.
- When a device is selected, do not auto-focus on every frame; only focus when the selection changes.

Aim the initial camera at the central AHU so clicking the exact center of the canvas picks the AHU. The camera focus equals the AHU center on every axis; do not use the previous `1.2` Y focus. Use this scene placement:

- AHU at `[0.0, 1.4, 0.0]`, size `[4.0, 2.8, 2.2]`.
- Supply fan at `[-4.2, 1.0, 1.0]`.
- Exhaust fan at `[4.2, 1.0, 1.0]`.
- Pumps at `[-2.0, 0.55, 3.0]` and `[2.0, 0.55, 3.0]`.

- [ ] **Step 3: Add labels and status visuals**

- 3D labels are DOM overlays, not `Text2d`. `Text2d` is rendered by Bevy's 2D pass and is not visible through a `Camera3d`; do not try to billboard it.
- Add `DeviceLabel3D { title, value, offset }` to each 3D device root. `title` is `node.label.unwrap_or(node.name)`; `value` starts empty and receives `Text` patches separately so the device name is not replaced by a temperature.
- At 20 Hz, project each label anchor with `camera.world_to_ndc(camera_transform, anchor)` and publish bridge data:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct SceneLabel3D {
    pub node_id: String,
    pub title: String,
    pub value: String,
    pub left_pct: f32,
    pub top_pct: f32,
    pub visible: bool,
}

pub static VIEW_LABELS_3D: GlobalSignal<Vec<SceneLabel3D>> = Signal::global(Vec::new);
```

  Convert NDC to percentages with `left_pct = (ndc.x * 0.5 + 0.5) * 100.0` and `top_pct = (0.5 - ndc.y * 0.5) * 100.0`; hide labels outside `[-1.1, 1.1]` or with NDC depth outside `0.0..=1.0`. Skip roots whose `InheritedVisibility` is false.
- Only write the bridge signal when the label list or displayed values change. Render it in `App` as an absolutely positioned sibling of the canvas:

```rust
#[component]
fn DeviceLabels3D() -> Element {
    let labels = bridge::VIEW_LABELS_3D();
    rsx! {
        div { class: "device-label-layer",
            for label in labels {
                div {
                    key: "{label.node_id}",
                    class: "device-label",
                    "data-testid": "device-label-{sanitize_test_id(&label.node_id)}",
                    style: "left: {label.left_pct:.2}%; top: {label.top_pct:.2}%;",
                    if label.visible {
                        strong { "{label.title}" }
                        if !label.value.is_empty() { span { "{label.value}" } }
                    }
                }
            }
        }
    }
}
```

- `.device-label-layer { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }` must not intercept canvas picking. Clear `VIEW_LABELS_3D` when leaving the 3D screen.
- Define `sanitize_test_id(id: &str) -> String` in `app.rs` now: lowercase ASCII alphanumerics are kept and every other character becomes `-`. Task 14 reuses it for screen, node, and tag test IDs.
- Add this structure for material and rotor indexing:

```rust
pub struct DeviceVisual {
    pub root: Entity,
    pub materials: Vec<Handle<StandardMaterial>>,
    pub progress: Option<Entity>,
    pub rotors: Vec<Entity>,
    pub selection_ring: Entity,
}

#[derive(Resource, Default)]
pub struct ThreeDVisualIndex {
    pub nodes: std::collections::HashMap<String, DeviceVisual>,
}
```

- Create a floor plane at `y = 0`, 32×32, dark blue-gray, non-pickable.
- Draw a 1 m floor grid with `Gizmos` in Edit mode and 2 m grid in Run mode.
- Lighting: one directional key light with shadows, one low-intensity point fill, ambient light `Color::srgb(0.16, 0.2, 0.24)`.
- Selection ring: a torus at the device base, hidden until selected; use the entity's XZ position and a cyan emissive material.
- Fault/run colors come from `Color` patches; also adjust emissive by multiplying the color by 0.45.

- [ ] **Step 4: Apply bindings and animation**

- On `TAG_REVISION`, resolve bindings for every 3D node and apply patches.
- `Color`: update all registered `StandardMaterial.base_color` and `emissive`.
- `Text`: update `DeviceLabel3D.value`; never overwrite `DeviceLabel3D.title`. The next 20 Hz label publish carries the value to the DOM overlay.
- `Visible`: set root visibility; hidden roots are omitted from the DOM label list.
- `Progress`: rotate the valve wheel by `progress * TAU` around local Z.
- `RotationSpeed`: set `HmiSpin.rad_per_sec` on fan and pump rotor entities.
- Rotor axes are deterministic: fan blades and AHU fan housings use `RotorAxis::LocalZ`; pump impellers use `RotorAxis::LocalY`. The shared `spin_rotors` system from Task 11 performs the rotation, so no second rotation system is registered in `HmiScenePlugin`.
- Zero speed stops rotation without rebuilding the scene.
- Add `sync_selection_rings` reading `bridge::SELECTED_NODE`; toggle each persisted `SelectionRing` from `ThreeDVisualIndex` and hide all rings when the selected node is absent from the active 3D screen.

- [ ] **Step 5: Build and verify the 3D screen**

```bash
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
cd apps/studio && dx build --web
```

Expected: PASS. At 1440×900:

1. Switch the active screen to `screen.bas_3d` using the temporary bridge call or inspector tab.
2. The room, devices, labels, and grid render.
3. Clicking the canvas center selects `node.3d.ahu` and updates the Dioxus selection.
4. Orbit/zoom/pan work without changing the project.
5. After 1 second, fan and pump rotors visibly move.
6. Switching back to 2D destroys all 3D entities; switching again rebuilds them.

Checkpoint: no `assets/...` font request is made; the font is embedded in the WASM.

### Task 14: Editor chrome, screen tree, palette, and property inspector

**Files:**
- Create: `apps/studio/src/ui/mod.rs`
- Create: `apps/studio/src/ui/topbar.rs`
- Create: `apps/studio/src/ui/left_panel.rs`
- Create: `apps/studio/src/ui/inspector.rs`
- Create: `apps/studio/src/ui/status_bar.rs`
- Modify: `apps/studio/src/app.rs`
- Modify: `apps/studio/assets/main.css`

**Interfaces:**
- Produces components `Topbar`, `LeftPanel`, `Inspector`, `StatusBar`
- Consumes `bridge::project()`, `bridge::ACTIVE_SCREEN`, `bridge::SELECTED_NODE`, `bridge::DOCUMENT_REVISION`, `bridge::TAG_REVISION`, `bridge::WORKSPACE_MODE`, `bridge::SAVE_STATUS`, `bridge::NOTICE_REVISION`
- Produces test IDs:
  - `project-name`
  - `save-status`
  - `undo-button`, `redo-button`, `save-button`
  - `notice` with `data-level=info|warning|error`
  - `screen-tree`, `screen-tab-screen-bas-2d`, `screen-tab-screen-bas-3d`
  - `node-row-{sanitized_node_id}`, for example `node-row-node-ahu`
  - `component-palette`, `add-panel`, `add-label`, `add-value-card`, `add-status-lamp`, `add-bar-gauge`, `add-button`, `add-pipe`, `add-equipment-symbol`
  - `add-ahu`, `add-fan`, `add-pump`, `add-duct`, `add-pipe-3d`, `add-valve`
  - `inspector`, `selection-id`, `node-name`, `node-x`, `node-y`, `node-width`, `node-height`, `node-visible`, `node-locked`
  - `duplicate-button`, `delete-button`
  - `scene-mode` text `2D` or `3D`
- Test ID sanitization rule: lowercase ASCII alphanumerics are kept; every other character becomes `-`. Apply the same rule to screens, nodes, tags, and binding indexes.

- [ ] **Step 1: Replace the placeholder shell with stable editor slots**

`App` must render one stable grid tree. The canvas path is never conditional:

```text
div.mox-shell
├── div.topbar-slot      -> Topbar in Edit, RuntimeTopbar in Run
├── div.left-slot        -> LeftPanel in Edit, empty in Run
├── main.viewport-shell
│   └── div.canvas-stage
│       ├── canvas#bevy-canvas
│       ├── DeviceLabels3D (pointer-events: none)
│       └── device details overlay (Run only)
├── div.right-slot       -> Inspector in Edit, empty in Run
├── div.footer-slot      -> StatusBar in Edit, empty in Run
└── global notice (position: fixed)
```

Only slot contents and slot visibility change. `main.viewport-shell > div.canvas-stage > canvas#bevy-canvas` must remain at the same RSX position in every mode and must never be wrapped by a mode-specific component that replaces the editor tree. In Run mode add `.is-run` to `.mox-shell`, omit editor panel components, and render the runtime chrome in the same topbar slot.

The canvas fills the central grid cell. It is not forced to a 16:9 DOM box; the Bevy `AutoMin` camera letterboxes the 1600×900 design inside whatever usable canvas size remains, so `design_to_page` must apply the same letterboxing math.

- [ ] **Step 2: Build the topbar**

`Topbar` must expose:

- At the start of the component, read `let _ = bridge::DOCUMENT_REVISION();` before calling `bridge::can_undo()` and `bridge::can_redo()`. Reading the revision subscribes the topbar to every committed project command; without it Undo/Redo enabled states stay stale after execute, undo, redo, import, and autosave.
- Brand and project name from `bridge::project().name` (`data-testid=project-name`).
- Undo button disabled when `!can_undo`; call `bridge::undo()`.
- Redo button disabled when `!can_redo`; call `bridge::redo()`.
- Save button calls `bridge::persist_now()` and sets an Info notice `已保存`.
- Import button opens the hidden file input from Task 15.
- Export button calls the export helper from Task 15.
- Edit/Run toggle sets `WorkspaceMode`.
- WebGPU status pill with `data-testid=webgpu-status`.
- Save status pill with `data-testid=save-status`; text is `已保存`, `保存中`, or `保存失败`.
- Render `bridge::notice()` as a fixed, non-blocking element with `data-testid=notice` and `data-level` equal to `info`, `warning`, or `error`. Invalid import assertions use `notice[data-level=error]`.

Use button classes `toolbar-button`, `toolbar-button-primary`, and `toolbar-button-danger`.

- [ ] **Step 3: Build the left panel**

Screen tree:

- Render each screen as a row with `data-testid=screen-tab-{sanitized_id}`. The demo IDs are exactly `screen-tab-screen-bas-2d` and `screen-tab-screen-bas-3d`; do not only replace dots, because `screen.bas_2d` must become `screen-bas-2d`.
- TwoD row shows `2D`, ThreeD row shows `3D`.
- Clicking a row calls `bridge::select_screen(screen.id())`.
- Active row gets `aria-selected="true"` and class `is-active`.
- Under the active screen, render one node row per node with `data-testid=node-row-{sanitized_node_id}` and the node name as text. `node.ahu` is exactly `node-row-node-ahu`. Clicking a row calls `bridge::select_node`.
- Rename button opens a local text input; commit through `RenameScreen`.
- Delete button publishes `DeleteScreen`; if only one screen remains, show a warning notice.
- Add screen button creates a blank TwoD screen named `新画面`, ID `screen.new.N` with a panel background node.

Component palette:

- In 2D, show exactly these buttons and IDs: Panel `add-panel`, Label `add-label`, ValueCard `add-value-card`, StatusLamp `add-status-lamp`, BarGauge `add-bar-gauge`, Button `add-button`, Pipe `add-pipe`, EquipmentSymbol `add-equipment-symbol`.
- In 3D, show exactly these buttons and IDs: AHU `add-ahu`, Fan `add-fan`, Pump `add-pump`, Duct `add-duct`, Pipe `add-pipe-3d`, Valve `add-valve`.
- Add nodes at `bridge::view_center_2d()` in 2D and at `Vec3::ZERO` offset by `nodes.len() * 0.4` in X in 3D.
- If an added node overlaps an existing node within 8 design units, offset by 24 pixels until free; stop after 20 attempts.
- IDs use `node.<kind>.<monotonic>` where the monotonic suffix is the smallest integer not already present.
- After add, call `bridge::select_node(Some(new_id))`.

Tag summary:

- Show the first 8 tags with current values.
- A later task adds the full Tag editor.

- [ ] **Step 4: Build the property inspector**

Inspector reads the selected node from `bridge::project()`.

Common fields:

- Name, X, Y, width, height, rotation, Z, visible, locked.
- For 3D: position X/Y/Z, rotation X/Y/Z, scale X/Y/Z.
- UI locks: when `locked = true`, disable geometry inputs but allow unlocking.

Input commit behavior:

- Use local draft signals seeded from the current node.
- Enter or blur commits.
- Escape restores the draft.
- Every input stops keydown propagation.
- Invalid numeric input shows a red outline and does not submit.

Commands:

- 2D geometry commits `Update2dNode`.
- 3D transform commits `Update3dNode`.
- Duplicate calls `DuplicateNode` and selects the returned `affected_id`.
- Delete calls `DeleteNode`; disabled for locked nodes.

Widget-specific fields:

- Label: text, font size, color, alignment.
- Panel: fill, stroke, stroke width.
- ValueCard/StatusLamp/BarGauge/Button: label; gauge min/max/direction; button action tag.
- EquipmentSymbol: equipment type and label.
- Pipe: thickness/color; points are editable as a compact `x,y;x,y` text field.
- Device model: size/color for Ahu/Fan/Pump/Valve; length/diameter/color for Duct/Pipe; GLB path for Glb.

Do not re-render or rebuild the Bevy scene on each keystroke; only committed changes call `bridge::execute`.

- [ ] **Step 5: Build the status bar**

Show:

- Selected node ID or `未选择`.
- Active screen kind and name.
- Current zoom as a percentage; read the Bevy camera value from a bridge cell updated in `two_d_edit.rs`.
- Tag simulation status `仿真 4 Hz`.
- Save status.
- Last engine error prefix if any.

Consume the `bridge::set_zoom_2d` / `bridge::zoom_2d` helpers added in Task 12. They are backed by a `Cell<f32>` and are intentionally not reactive signals; status-bar refreshes are driven by the surrounding simulation/revision renders.

- [ ] **Step 6: Add the industrial visual system**

Update `main.css`:

- Grid columns `272px minmax(0,1fr) 352px`, rows `64px minmax(0,1fr) 36px`.
- Place `.topbar-slot`, `.left-slot`, `.right-slot`, `.footer-slot`, and `.viewport-shell` on the named grid areas. Slot wrappers must not remount the canvas.
- `.mox-shell.is-run` sets columns to `0 minmax(0,1fr) 0`; editor slots are omitted so `inspector` is absent from the DOM, not merely invisible.
- Panels `#0c171e`, surfaces `#101e27`, lines `#1f3540`.
- Selected rows use cyan left border and `#102b2d` fill.
- Buttons 30–34 px high, 6 px radius, no gradients.
- Inputs use monospace numerals for geometry.
- Warning amber, error red, focus ring cyan.
- At 1280×720 the center viewport must remain at least 520 px wide.

- [ ] **Step 7: Verify the editor shell**

```bash
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
cd apps/studio && dx build --web
```

Expected: PASS. At 1440×900:

1. Both screen tabs are visible and switching changes `scene-mode` from `2D` to `3D`.
2. Clicking the 2D AHU shows `selection-id = node.ahu`.
3. Editing `node-x` and pressing Enter moves the AHU after commit.
4. Undo restores the X value and the input.
5. Add ValueCard creates a node, selects it, and the inspector shows `ValueCard`.
6. Add Fan in 3D creates a device at an offset.

Checkpoint: screenshot `/tmp/mox-hmi-editor.png` and inspect with `view_image`; layout must have no clipped controls or horizontal scrollbar at 1440×900.

### Task 15: Tag editor, binding inspector, and JSON import/export

**Files:**
- Create: `apps/studio/src/ui/file_io.rs`
- Modify: `apps/studio/src/ui/mod.rs`
- Modify: `apps/studio/src/ui/left_panel.rs`
- Modify: `apps/studio/src/ui/inspector.rs`
- Modify: `apps/studio/src/ui/topbar.rs`
- Modify: `apps/studio/src/app.rs`
- Modify: `apps/studio/assets/main.css`
- Modify: `apps/studio/Cargo.toml`

**Interfaces:**
- Produces test IDs:
  - `tag-list`, `add-tag`, `tag-row-{sanitized_id}`, `tag-name`, `tag-unit`, `tag-min`, `tag-max`, `tag-precision`, `tag-writable`, `tag-simulator`, `tag-initial`, `apply-tag`, `delete-tag`
  - `binding-list`, `binding-add`, `binding-remove-{index}`, `binding-tag-{index}`, `binding-target-{index}`, `binding-mapping-{index}`, `apply-bindings`
  - `binding-precision`, `binding-unit`, `binding-true-color`, `binding-false-color`, `binding-range-input-min`, `binding-range-input-max`, `binding-range-output-min`, `binding-range-output-max`, `binding-range-clamp`
  - `import-input`, `import-button`, `export-button`
- Produces: `download_json(filename: &str, body: &str) -> Result<(), String>`
- Produces: `read_uploaded_text(file: FileData) -> Result<String, String>`
- Consumes `EditorCommand::{AddTag, UpdateTag, DeleteTag, SetBindings}`

- [ ] **Step 1: Add file I/O dependencies**

Extend `web-sys` features with `"Blob"`, `"BlobPropertyBag"`, `"Url"`, `"HtmlAnchorElement"`, `"Document"`, `"Element"`.

`apps/studio/src/ui/file_io.rs`:

```rust
use dioxus::html::FileData;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement};

pub fn download_json(filename: &str, body: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| "window 不可用".to_string())?;
    let document = window.document().ok_or_else(|| "document 不可用".to_string())?;
    let parts = js_sys::Array::of1(&JsValue::from_str(body));
    let blob = Blob::new_with_str_sequence_and_options(
        &parts,
        BlobPropertyBag::new().type_("application/json;charset=utf-8"),
    ).map_err(|e| format!("创建文件失败：{e:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("创建下载地址失败：{e:?}"))?;
    let anchor = document.create_element("a")
        .map_err(|e| format!("创建下载节点失败：{e:?}"))?
        .dyn_into::<HtmlAnchorElement>()
        .map_err(|_| "下载节点类型错误".to_string())?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    web_sys::Url::revoke_object_url(&url).map_err(|e| format!("释放下载地址失败：{e:?}"))?;
    Ok(())
}

pub async fn read_uploaded_text(file: FileData) -> Result<String, String> {
    let size = file.size();
    if size > hmi_core::MAX_IMPORT_BYTES as u64 {
        return Err("工程文件超过 5 MB 限制".into());
    }
    let bytes = file.read_bytes().await.map_err(|e| format!("读取文件失败：{e}"))?;
    String::from_utf8(bytes.to_vec()).map_err(|_| "工程文件必须是 UTF-8 JSON".to_string())
}
```

- [ ] **Step 2: Build the Tag panel**

Place the Tag panel below the screen tree and component palette.

Tag list behavior:

- Show all tags with current values from `bridge::tag_snapshot()`.
- Sanitize IDs for test IDs by replacing every non-alphanumeric character with `-`.
- Selected tag gets a local editor.

Add Tag defaults:

```rust
TagDefinition {
    id: format!("bas.custom.{}", next_index),
    name: "新 Tag".into(),
    value_type: TagValueType::Number,
    unit: String::new(),
    min: Some(0.0),
    max: Some(100.0),
    precision: 0,
    initial: TagValue::Number(0.0),
    writable: false,
    simulator: SimulatorKind::Constant,
}
```

Tag editor commit:

- ID is read-only after creation.
- Name cannot be empty.
- Number tags require finite min/max with min < max.
- Precision is `0..=6`.
- Simulator select offers only compatible options: Number -> constant/sine/step/random_walk/manual; Bool -> constant/pulse/manual; Enum -> constant/manual.
- Initial value parser:
  - Number: parse `f32`.
  - Bool: `true`/`false` or `运行`/`停止`.
  - Enum: raw string.
- Commit through `UpdateTag`; on error show `set_notice(Error, error.to_string())`.
- Delete through `DeleteTag`; if `TagInUse`, keep selection and show the node list from the error.

- [ ] **Step 3: Build the binding inspector**

The binding editor uses a local draft cloned from the selected node. It does not call commands on every keystroke.

Binding list:

- One row per binding.
- Tag selector lists all project tags.
- Target selector lists only targets valid for the node/widget family.
- Mapping selector lists only mappings legal for the selected target and tag type.
- Changing tag/target/mapping replaces the mapping with `default_mapping(target, tag)`.
- Remove updates only the draft.
- Add appends a binding with the first tag, a legal target, and `default_mapping`.
- Apply button commits the whole vector through `SetBindings`.
- If no tags exist, Add is disabled and an inline note says `请先创建 Tag`.

Mapping fields:

- `Format`: optional precision and unit; blank means inherit from Tag.
- `BoolColor`: two color inputs.
- `ThresholdColor`: 1–8 rows of value + color; add/remove threshold controls; sort on commit.
- `Range`: input min/max, output min/max, clamp checkbox.
- `Direct`: no fields.

Validation before `SetBindings`:

- Every tag ID must exist.
- Target/mapping matrix from Task 3.
- Range bounds must not be equal and all numbers finite.
- Threshold values finite and count 1–8.
- Colors must match `#RRGGBB`.
- Show a notice and do not call the command when these fail.

- [ ] **Step 4: Implement import and export**

Topbar:

- Hidden file input:

```rust
input {
    r#type: "file",
    accept: ".json,.moxhmi.json,application/json",
    "data-testid": "import-input",
    hidden: true,
    onchange: move |event| {
        if let Some(file) = event.files().into_iter().next() {
            spawn(async move {
                match read_uploaded_text(file).await {
                    Ok(text) => {
                        if let Err(error) = bridge::import_json(&text) {
                            bridge::set_notice(bridge::NoticeLevel::Error, error);
                        }
                    }
                    Err(error) => bridge::set_notice(bridge::NoticeLevel::Error, error),
                }
            });
        }
    },
}
button {
    "data-testid": "import-button",
    onclick: move |_| {
        let _ = document::eval("document.querySelector('[data-testid=import-input]').click()");
    },
    "导入"
}
```

Use a typed element reference if available; otherwise the selector above is the required implementation.

Export:

```rust
button {
    "data-testid": "export-button",
    onclick: move |_| {
        let project = bridge::project();
        match bridge::export_json().and_then(|json| {
            download_json(&hmi_core::persistence::export_filename(&project), &json)
        }) {
            Ok(()) => bridge::set_notice(bridge::NoticeLevel::Info, "工程已导出"),
            Err(error) => bridge::set_notice(bridge::NoticeLevel::Error, error),
        }
    },
    "导出"
}
```

Import must never replace the current session on parse, size, schema, or validation failure.

- [ ] **Step 5: Build and exercise Tag/binding editing**

```bash
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
cd apps/studio && dx build --web
```

Expected: PASS. In the browser:

1. Add a Tag; it appears in the list and in Tag selectors.
2. Edit its max and commit; the change survives a 600 ms autosave and refresh.
3. Try deleting `bas.supply_fan.run`; an error notice lists its references and the project is unchanged.
4. Select the 2D supply fan; change its Color binding to another Boolean tag; apply; the scene color updates.
5. Undo restores the previous binding.
6. Export produces `bas-demo.moxhmi.json` with snake_case enums.
7. Import that file; the project remains valid.
8. Import `{invalid`; an error appears and the project name/selection do not change.

Checkpoint: open the exported JSON and confirm no runtime Tag values, selection, camera, or mode fields are present.

### Task 16: Runtime mode, device details, and 2D button actions

**Files:**
- Create: `apps/studio/src/ui/runtime.rs`
- Create: `apps/studio/src/ui/device_details.rs`
- Modify: `apps/studio/src/ui/mod.rs`
- Modify: `apps/studio/src/ui/topbar.rs`
- Modify: `apps/studio/src/app.rs`
- Modify: `apps/studio/src/bridge/mod.rs`
- Modify: `apps/studio/src/scene/two_d_edit.rs`
- Modify: `apps/studio/src/scene/plugin.rs`
- Modify: `apps/studio/assets/main.css`

**Interfaces:**
- Produces: `bridge::invoke_button_action(action: &ButtonAction) -> Result<(), HmiError>`
- Produces test IDs:
  - `run-shell`, `run-exit`, `run-screen-2d`, `run-screen-3d`, `run-clock`, `run-mode-badge`
  - `device-details`, `device-details-title`, `device-details-status`, `device-details-close`
  - `device-tag-{sanitized_tag_id}`
  - `runtime-tag-bas-ahu-supply-temp`, `runtime-tag-bas-supply-fan-freq`
  - `supply-fan-state`
  - `button-supply-fan`
- Consumes `WorkspaceMode::Run`
- Guarantees: runtime writes never call `EditorSession::execute`, never change `DOCUMENT_REVISION`, never add undo entries, and never set `SAVE_STATUS` to Pending

- [ ] **Step 1: Implement the button action bridge**

Add to `bridge/mod.rs`:

```rust
pub fn invoke_button_action(action: &hmi_core::ButtonAction) -> Result<(), hmi_core::HmiError> {
    use hmi_core::{ButtonAction, TagValue};
    match action {
        ButtonAction::ToggleTag { tag_id } => {
            let value = tag_snapshot()
                .get(tag_id)
                .cloned()
                .ok_or_else(|| hmi_core::HmiError::UnknownTag(tag_id.clone()))?;
            let next = match value {
                TagValue::Bool(value) => TagValue::Bool(!value),
                _ => return Err(hmi_core::HmiError::InvalidCommand(
                    "ToggleTag 只支持 Bool Tag".into(),
                )),
            };
            write_tag(tag_id, next)
        }
        ButtonAction::SetTag { tag_id, value } => write_tag(tag_id, value.clone()),
    }
}
```

- [ ] **Step 2: Handle 2D runtime clicks**

In the 2D pointer handler:

- If mode is `Run`, the active screen is `Screen::TwoD`, and left button is pressed:
  1. Hit-test nodes in descending `z` with the shared `hit_test_2d_node` helper from Task 12; it excludes `node.bg`.
  2. If the hit node is a `Widget2D::Button { action, .. }`, call `bridge::invoke_button_action(action)`.
  3. Also select the button node so the details panel can show it.
  4. Otherwise select the hit node.
  5. Clicking empty space clears selection.
- On action success, write an Info log `MOX HMI button: {action:?}`; do not show a blocking notice.
- On action error, set an Error notice.
- Disable drag, resize, grid, and selection handles in Run mode.
- Keep orbit/pan/zoom behavior available in 3D Run mode.

- [ ] **Step 3: Build the runtime shell**

When `WORKSPACE_MODE == Run`, `App` keeps the exact stable grid tree from Task 14. It renders runtime chrome in `div.topbar-slot`, omits `LeftPanel`, `Inspector`, and `StatusBar` from their slots, and keeps `main.viewport-shell > div.canvas-stage > canvas#bevy-canvas` at the same RSX position. Do not return a `RuntimeShell` that replaces the editor tree or remounts the canvas.

Runtime shell requirements:

- Top bar with station/project name, current local time updated every second, 2D/3D screen switch, `仿真 4 Hz` badge, engine status, and return-to-edit button.
- No left panel, right inspector, grid, save/undo controls, or component palette.
- The canvas remains the central visual; no edit toolbar overlaps the device click area.
- `run-screen-2d` and `run-screen-3d` call `bridge::select_screen` and clear the selected node.
- `run-exit` sets `WorkspaceMode::Edit` and leaves the current screen unchanged.
- Time uses `js_sys::Date::new_0().to_locale_time_string("zh-CN")`; format as `HH:mm:ss`.
- If engine status is Failed, show a non-blocking error strip but keep the return button usable.

- [ ] **Step 4: Build the device details overlay**

When `SELECTED_NODE` is `Some` and mode is Run, derive the node from `bridge::project()` and render an overlay card anchored at the lower-right of the central canvas.

Card content:

- Node name and kind (`2D 设备符号`, `2D 按钮`, `3D AHU`, etc.).
- For equipment nodes, a status line:
  - Fault tag true -> `故障` in red.
  - Run tag true -> `运行` in cyan.
  - Otherwise -> `停止` in muted gray.
- Every binding: Tag name, current formatted value, target, and mapping summary.
- Close button clears selection.

Test IDs:

- Card: `device-details`.
- Title: `device-details-title`.
- Status: `device-details-status`.
- Each tag value: `device-tag-{sanitized_tag_id}`.

Use `TAG_REVISION()` in the component so values refresh at 4 Hz.

- [ ] **Step 5: Add deterministic runtime DOM mirrors**

The Bevy canvas is not readable by Playwright, so expose runtime values through DOM mirrors that use the same state and actions as the scene.

In the runtime top strip, include:

```rust
span {
    "data-testid": "supply-fan-state",
    if fan_running { "送风机：运行" } else { "送风机：停止" }
}
span {
    "data-testid": "runtime-tag-bas-ahu-supply-temp",
    format_tag_value(snapshot.get("bas.ahu.supply_temp").unwrap_or(&TagValue::Number(0.0)), 1, "°C")
}
span {
    "data-testid": "runtime-tag-bas-supply-fan-freq",
    format_tag_value(snapshot.get("bas.supply_fan.freq").unwrap_or(&TagValue::Number(0.0)), 1, "Hz")
}
```

Read `bridge::TAG_REVISION()` in this component so all three values refresh at 4 Hz. Exact labels are optional, but the test IDs and displayed values are mandatory.

The 2D button node has ID `node.button.fan`; add `data-testid="button-supply-fan"` to a DOM mirror button in the runtime top strip. It must call the same `bridge::invoke_button_action(&ButtonAction::ToggleTag { tag_id: "bas.supply_fan.run".into() })` path as the Bevy button. The primary interaction remains the Bevy button.

- [ ] **Step 6: Verify the runtime loop**

```bash
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
cd apps/studio && dx build --web
```

Expected: PASS. In the browser:

1. Click Run; all editor panels disappear, `run-shell` appears.
2. 2D/3D switching works in runtime.
3. Waiting 2 seconds changes at least one value card and details tag value.
4. Clicking the Bevy fan button or the DOM mirror `button-supply-fan` toggles `bas.supply_fan.run`; `supply-fan-state` changes from `停止` to `运行`.
5. No undo entry is added; returning to Edit keeps Undo disabled if the history was empty.
6. Reloading resets the manual Tag to its project initial value.
7. The project JSON in localStorage is unchanged by runtime interactions.

Checkpoint: runtime mode must be a read-only viewer plus Tag-write path, not a second editor.

### Task 17: Playwright end-to-end suite and visual QA

**Files:**
- Create: `scripts/e2e.py`
- Create: `scripts/verify.sh`
- Regenerate: `apps/studio/public/assets/fonts/NotoSansSC-Subset.otf`
- Modify: `apps/studio/src/ui/*.rs` only to replace unstable selectors with the test IDs above
- Modify: `apps/studio/assets/main.css` only for defects found by visual QA

**Interfaces:**
- Produces: `python3 scripts/e2e.py` exit code 0/1
- Produces screenshots `/tmp/mox-hmi-2d.png`, `/tmp/mox-hmi-3d.png`, `/tmp/mox-hmi-runtime.png`
- Consumes all test IDs from Tasks 14–16

- [ ] **Step 1: Regenerate the Chinese font subset**

All Chinese UI strings from Tasks 14–16 now exist. Regenerate the embedded subset before building or recording E2E results:

```bash
python3 -m pip install --user fonttools
python3 scripts/build_font_subset.py
cd apps/studio && dx build --web
```

Expected: the script reports no missing glyphs, the subset stays below 1.5 MB, and the WASM rebuild succeeds. Do not reuse the provisional Task 11 subset; `include_bytes!` must pick up this final file.

- [ ] **Step 2: Write the E2E harness and nine required flows**

`scripts/e2e.py` must start `dx serve` itself, wait for HTTP readiness, run Chrome for Testing with WebGPU flags, and always terminate the server. Use `CHROME_PATH` when set; otherwise use:

```text
/tmp/chrome-dl/app/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing
```

Required helper contract. Bevy uses `AutoMin`, so the limiting dimension wins and the scale is `min`, not `max`:

```python
async def design_to_page(page, x: float, y: float) -> tuple[float, float]:
    box = await page.locator("#bevy-canvas").bounding_box()
    assert box is not None
    scale = min(box["width"] / 1600.0, box["height"] / 900.0)
    draw_w, draw_h = 1600.0 * scale, 900.0 * scale
    offset_x = box["x"] + (box["width"] - draw_w) / 2.0
    offset_y = box["y"] + (box["height"] - draw_h) / 2.0
    return offset_x + x * scale, offset_y + y * scale


def sanitize_test_id(value: str) -> str:
    return "".join(ch.lower() if ch.isascii() and ch.isalnum() else "-" for ch in value)
```

The nine flows:

1. **WebGPU and startup:** clear localStorage, reload, assert `navigator.gpu`, wait for `WebGPU ready`, assert console contains `MOX HMI runtime ready` and `BrowserWebGpu`.
2. **Default project and screens:** assert project name contains `BAS`; assert both `screen-tab-screen-bas-2d` and `screen-tab-screen-bas-3d` exist; switch to 3D, assert `scene-mode` contains `3D`, switch back.
3. **2D selection:** click the AHU design point `(780, 340)` through `design_to_page`; assert `selection-id` is `node.ahu`; inspector shows `node-name` value `AHU-01`.
4. **Edit and undo:** fill `node-x` with `700`, press Enter, wait for the selection/geometry commit, fill `node-x` with `820`; undo twice and assert the value returns to the demo origin `650`. Use the inspector value rather than pixels.
5. **Add, autosave, reload:** click `add-value-card`; capture the new selected ID; assert `node-row-{sanitize_test_id(new_id)}` appears; wait for `save-status` to show `已保存`, reload, wait ready, then assert the same node row exists and localStorage contains the node ID.
6. **3D picking:** switch to 3D, click the exact canvas center, assert `selection-id` is `node.3d.ahu`; assert the active screen remains 3D and the inspector/Dioxus state updated.
7. **Tag simulation:** enter Run if needed; read the text of `runtime-tag-bas-ahu-supply-temp` and `runtime-tag-bas-supply-fan-freq`; wait 1500 ms; assert at least one value changed. Do not read canvas pixels. Exit Run when done.
8. **Runtime and button write:** wait for save status `已保存` and snapshot the localStorage project string; click Run; assert `run-shell` visible and `inspector` absent; click `button-supply-fan`; assert `supply-fan-state` changes; click `run-exit`; assert editor returns and the localStorage project string is byte-identical.
9. **Invalid import:** record `project-name`; upload `{not json` through `import-input`; wait for `notice[data-level=error]`; assert `project-name` is unchanged and `webgpu-status` is still ready.

Additional assertions:

- No `pageerror` entries.
- No console `error:` entries, except browser-level WebGPU warnings explicitly allowlisted in the script.
- After each screenshot, assert the page has no horizontal overflow:

```python
overflow = await page.evaluate("document.documentElement.scrollWidth > window.innerWidth")
assert not overflow
```

Canvas text is not accessible to Playwright. Use `selection-id`, `node-name`, `runtime-tag-*`, `supply-fan-state`, `notice`, and the localStorage snapshot for deterministic assertions; reserve pixel clicks for the two documented design-coordinate targets.

- [ ] **Step 3: Make failures actionable**

On any assertion failure:

- Save `/tmp/mox-hmi-failure.png`.
- Print the last 100 console lines.
- Print the failing flow number and selector.
- Terminate the `dx serve` process and browser.

Do not swallow page errors.

- [ ] **Step 4: Add the verification script**

`scripts/verify.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo fmt --check
cargo test -p hmi-core
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
python3 scripts/e2e.py
```

Make it executable:

```bash
chmod +x scripts/verify.sh
```

- [ ] **Step 5: Run the full suite**

```bash
./scripts/verify.sh
```

Expected: PASS, nine flows complete, exit code 0.

- [ ] **Step 6: Visual QA at 1440×900**

Run the E2E script in headed mode or take the three screenshots and inspect each with `view_image`:

- `/tmp/mox-hmi-2d.png`: 2D equipment and text legible with no tofu glyphs; no overlapping cards; cyan/muted palette; no game-like glow.
- `/tmp/mox-hmi-3d.png`: room and devices readable; DOM labels track their devices above the models; AHU center click target obvious; no missing materials.
- `/tmp/mox-hmi-runtime.png`: no editor panels; runtime bar, tag mirrors, and details card readable; canvas is the dominant surface.

If any control is clipped, adjust spacing or grid dimensions and rerun `python3 scripts/e2e.py`.

Checkpoint: E2E and visual QA must both pass; do not mark the MVP complete from unit tests alone.

### Task 18: Production build, WASM budget, and operator documentation

**Files:**
- Modify: `apps/studio/Cargo.toml`
- Modify: `Cargo.toml`
- Create: `README.md`
- Modify: `scripts/verify.sh`
- Create: `scripts/check_wasm_size.sh`
- Optionally modify: `.cargo/config.toml`

**Interfaces:**
- Produces release bundle with base path support
- Produces WASM size report
- Produces run/build/deploy instructions
- Budget: uncompressed release WASM `<= 30 MB`, target `<= 25 MB`

- [ ] **Step 1: Trim Bevy features**

Replace the initial `"2d"`/`"3d"` meta-features with the explicit set:

```toml
bevy = { version = "0.20.1", default-features = false, features = [
  "std",
  "async_executor",
  "bevy_asset",
  "bevy_log",
  "bevy_state",
  "reflect_auto_register",
  "bevy_winit",
  "bevy_window",
  "multi_threaded",
  "web",
  "webgpu",
  "default_font",
  "2d_bevy_render",
  "3d_bevy_render",
  "bevy_picking",
  "mesh_picking",
  "bevy_scene",
  "bevy_gizmos_render",
  "png",
] }
```

Verify that `bevy_audio`, `bevy_gilrs`, `bevy_ui`, `bevy_ui_render`, `webgl2`, and gamepad features are absent:

```bash
cargo tree -p mox-hmi-studio -e features | rg 'bevy_(audio|gilrs|ui|ui_render)|webgl2' && exit 1 || true
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
```

Expected: no forbidden feature lines; check passes. If a missing feature causes a compile error, add only the narrowest required Bevy feature, never `ui`, `audio`, or `webgl2`.

- [ ] **Step 2: Build release with base-path support**

```bash
cd apps/studio
dx build --web --release --debug-symbols false --base-path /mox-hmi/
```

Expected: build succeeds and assets are emitted under the Dioxus target output. Locate the release `.wasm` with:

```bash
find target/dx -name '*.wasm' -type f -print
```

- [ ] **Step 3: Run wasm-opt and enforce the budget**

Install Binaryen if absent:

```bash
command -v wasm-opt >/dev/null || brew install binaryen
```

`scripts/check_wasm_size.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WASM="${1:-$(find "$ROOT/target/dx" -name '*.wasm' -type f | sort | tail -1)}"
test -n "$WASM"
wasm-opt -Oz --strip-debug "$WASM" -o "$WASM.opt"
mv "$WASM.opt" "$WASM"
BYTES="$(wc -c < "$WASM" | tr -d ' ')"
LIMIT=$((30 * 1024 * 1024))
printf 'WASM: %s bytes (%.2f MiB)\n' "$BYTES" "$(awk "BEGIN {printf \"%.2f\", $BYTES/1048576}")"
if [ "$BYTES" -gt "$LIMIT" ]; then
  echo "WASM budget exceeded: limit is 30 MiB" >&2
  exit 1
fi
```

```bash
chmod +x scripts/check_wasm_size.sh
./scripts/check_wasm_size.sh
```

Expected: `WASM: ...` below 30 MiB. If over budget, apply in order:

1. Confirm `Cargo.toml` still has `opt-level = "z"`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`.
2. Confirm no forbidden features with `cargo tree`.
3. Confirm `dx build` uses `--debug-symbols false`.
4. Remove unused image format features; keep `png`.
5. Re-run `wasm-opt -Oz`.

Do not remove WebGPU, 2D rendering, 3D PBR, `bevy_scene`, or text rendering to meet the budget.

- [ ] **Step 4: Verify the base-path bundle**

Serve the release output under `/mox-hmi/` using a local static server that preserves the prefix. A simple Python server is sufficient if the bundle directory is mounted at that prefix.

Then run the E2E suite with:

```bash
MOX_HMI_URL=http://127.0.0.1:8090/mox-hmi/ python3 scripts/e2e.py --url-from-env
```

Expected: PASS. If Bevy assets fail under the prefix, set `AssetPlugin.file_path` from `dioxus::cli_config::base_path()`:

```rust
let base = dioxus::cli_config::base_path().unwrap_or_default();
let asset_root = format!("{base}/assets").trim_end_matches('/').to_string();
AssetPlugin { file_path: asset_root, meta_check: AssetMetaCheck::Never, ..default() }
```

The embedded font must not depend on this path.

- [ ] **Step 5: Write README**

`README.md` must include:

- Product scope: BAS MVP, 2D/3D, edit/run, simulated Tags.
- Fixed versions and WebGPU/browser requirements.
- Local run: `cd apps/studio && dx serve --web --port 8088`.
- Test: `./scripts/verify.sh`.
- Release build and base-path command.
- WASM budget command.
- JSON import/export and localStorage key.
- Deployment requirements: HTTPS/localhost, CSP `wasm-unsafe-eval`, Brotli/gzip at the server, MIME type `application/wasm`.
- Explicit non-goals: real protocols, permissions, alarms, scripting, mobile/Safari/Firefox.
- Note that `spikes/dioxus-bevy` is reference-only and not part of production.

- [ ] **Step 6: Final acceptance run**

```bash
cargo fmt --check
cargo test -p hmi-core
cargo check -p mox-hmi-studio --target wasm32-unknown-unknown
./scripts/e2e.py
cd apps/studio && dx build --web --release --debug-symbols false --base-path /mox-hmi/
cd ../.. && ./scripts/check_wasm_size.sh
```

Expected:

- All core tests pass.
- All nine browser flows pass.
- Release WASM is at most 30 MiB.
- No unhandled console errors.
- 1440×900 visual QA screenshots pass.
- Runtime mode does not mutate the project.
- Import failure preserves the current project.

Checkpoint: the MVP is complete only when every item above is green. Report the exact WASM byte count and screenshot paths in the final handoff.
