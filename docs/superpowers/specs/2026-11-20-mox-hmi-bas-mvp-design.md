# MOX HMI Studio：车站 BAS 轻量编辑器与运行器设计

- 日期：2026-11-20
- 状态：已确认（2026-11-20 用户确认）
- 目标版本：MVP v0.1
- 目标场景：地铁车站 BAS 环控系统

## 1. 摘要

MOX HMI Studio 是一套运行在浏览器中的工业 HMI 编辑器与运行器。首期以地铁车站 BAS 环控系统为示例，提供 2D 工艺画面编辑、3D 设备数字孪生、Tag 数据绑定、仿真数据和运行态预览。

技术路线已经通过一次性 spike 验证：

- Dioxus `0.7.10` 负责 Web DOM 编辑器外壳。
- Bevy `0.20.1` 负责 2D/3D 场景、拾取、动画和数据绑定。
- 两者运行在同一个 WASM、同一个 Canvas 页面中，通过类型化桥接层双向同步。
- Bevy 使用 WebGPU 后端，实测日志为 `BrowserWebGpu`。
- `spikes/dioxus-bevy` 为一次性验证代码，不作为生产实现依赖。

## 2. 目标

### 2.1 产品目标

1. 在浏览器内完成 HMI 画面的编辑、保存、加载和运行预览。
2. 以 2D 工艺画面为主视图，以 3D 数字孪生为设备空间视图。
3. 用统一的 Tag 和绑定模型同时驱动 2D 控件与 3D 设备。
4. 形成可复用到其他工业场景的轻量 HMI 内核。
5. 第一版具备真实演示价值，而不是只有空白画布和静态按钮。

### 2.2 技术目标

1. 全 Rust Web 技术栈，不引入 Node/React 运行时。
2. 基于 WebGPU 渲染，不提供 WebGL2 回退。
3. 领域模型与 UI/渲染解耦，便于后续接入 MQTT、OPC UA 或外部数据网关。
4. 工程文件可导出、可导入、可版本化、可自动保存。
5. 核心行为具备单元测试，主要交互具备浏览器端到端测试。

## 3. 首期范围

### 3.1 包含

- 地铁车站 BAS 演示工程：
  - 2D 环控工艺画面。
  - 3D 环控机房数字孪生画面。
  - 温度、湿度、CO₂、风机频率、设备启停和故障状态等 Tag。
- 编辑器：
  - 画面树和组件面板。
  - 2D 控件添加、选择、移动、缩放、删除、复制。
  - 3D 设备添加、选择、属性编辑。
  - 属性检查器和 Tag 绑定检查器。
  - 撤销/重做。
  - 自动保存、JSON 导入导出。
- 运行器：
  - 编辑/运行模式切换。
  - 2D/3D 画面切换。
  - 内置 Tag 仿真。
  - 运行态按钮写入仿真 Tag。
  - 运行态设备详情浮层。
- 2D 控件：
  - 面板、文本、数值卡、状态灯、条形仪表、按钮、管线、设备符号。
- 3D 设备：
  - AHU、送风机、排风机、水泵、风管、水管、阀门。
  - 同源 GLB 模型节点。
- 工程文件：
  - JSON Schema v1。
  - localStorage 自动保存。
  - 文件导入导出。

### 3.2 不包含

- 真实 MQTT、OPC UA、Modbus 或外部数据网关接入。
- 真实设备写操作、报警工作流、报警确认、趋势历史。
- 用户登录、权限、审批、多人协作。
- 脚本引擎、宏、表达式语言和自定义插件。
- 完整 3D 建模、材质编辑、变换 Gizmo 和动画编辑器。
- 浏览器上传任意 GLB 文件；v1 只加载同源静态 GLB 资产。
- Safari、Firefox、移动端和触摸操作的正式兼容。
- WebGL2 回退构建。
- 2D 与 3D 在同一画面内混排。

## 4. 关键设计决策

| 决策 | 选择 | 理由 |
| --- | --- | --- |
| UI 框架 | Dioxus 0.7.10 | 全 Rust、DOM 表单能力强、可嵌入现有 Web 环境 |
| 引擎 | Bevy 0.20.1 | 2D/3D 统一 ECS、场景能力强、满足长期数字孪生路线 |
| Web 渲染 | WebGPU only | 已锁定 Chromium 120+ 环境，3D 性能上限更高 |
| 2D 渲染 | Bevy 2D Mesh2d + Text2d | 坐标、相机缩放、拾取和动画完全可控 |
| 3D 渲染 | Bevy PBR + 程序化设备 + 同源 GLB | 首期无需外部建模资产即可演示，同时保留扩展点 |
| 2D/3D 关系 | 独立画面切换 | 避免首期混合坐标系和交互复杂度 |
| 编辑器 UI | Dioxus DOM | 复杂表单、布局、中文输入和可访问性优于引擎内 UI |
| 工程格式 | 自定义 JSON Schema v1 | 可读、可测、可迁移，不绑定 Bevy 场景序列化 |
| 数据层 | 仿真 Tag + 手动按钮写入 | 首期闭环真实可演示，后续替换数据适配器即可 |
| 状态管理 | 单一领域状态 + 命令/Revision 桥 | 避免 Dioxus 与 Bevy 各自维护一份工程数据 |

## 5. 演示场景

演示工程名为“车站 BAS 环控示例”。

### 5.1 2D 工艺画面

画面表达车站环控系统的运行关系：

- 组合式空调机组 AHU-01。
- 送风机 SF-01、排风机 EF-01。
- 冷冻水泵 CHP-01、冷却水泵 CWP-01。
- 新风阀、回风阀、冷冻水阀。
- 送风管、回风管、冷冻水管。
- 当前温度、湿度、CO₂、风机频率、设备状态和故障状态。
- “启动/停止送风机”按钮，用于在运行态写入手动 Tag。

画面采用 1600×900 设计空间，按容器等比缩放，四周允许留黑边。编辑模式显示网格和选中框，运行模式隐藏辅助元素。

### 5.2 3D 数字孪生画面

3D 画面表达环控机房的设备空间关系：

- 机房地面、轻量网格和基础灯光。
- AHU、送排风机、水泵、风管、水管、阀门。
- 设备标签使用面向相机的 `Text2d` 广告牌。
- 风机和泵按 Tag 频率旋转。
- 设备颜色反映运行、停止和故障状态。
- 支持轨道相机、缩放、点击选中和设备详情浮层。

### 5.3 Tag 示例

| Tag ID | 类型 | 单位 | 仿真方式 | 用途 |
| --- | --- | --- | --- | --- |
| `bas.ahu.supply_temp` | Number | °C | Sine | 送风温度 |
| `bas.station.humidity` | Number | %RH | Sine | 车站湿度 |
| `bas.station.co2` | Number | ppm | RandomWalk | CO₂ 浓度 |
| `bas.supply_fan.run` | Bool | - | Manual | 送风机启停 |
| `bas.supply_fan.fault` | Bool | - | Manual | 送风机故障 |
| `bas.supply_fan.freq` | Number | Hz | Sine | 送风机频率 |
| `bas.chilled_pump.run` | Bool | - | Pulse | 冷冻水泵状态 |
| `bas.ahu.valve.opening` | Number | % | Sine | 冷冻水阀开度 |

## 6. 用户流程

### 6.1 编辑流程

1. 打开应用，加载自动保存工程；不存在或损坏时加载内置 BAS 示例。
2. 在左侧画面树选择 2D 或 3D 画面。
3. 从组件面板添加控件或设备。
4. 在视口选择对象；右侧检查器显示属性和绑定。
5. 修改位置、尺寸、文本、颜色或绑定，视口即时刷新。
6. 使用撤销/重做调整操作。
7. 切换运行模式检查效果。
8. 导出 JSON 或继续自动保存。

### 6.2 运行流程

1. 点击“运行”，隐藏编辑面板并进入全屏运行态。
2. 顶部显示站点名称、当前时间、画面切换和运行状态。
3. Tag 仿真以 4 Hz 更新，2D 数值和 3D 动画随之变化。
4. 点击 2D 设备或 3D 设备，显示设备详情浮层。
5. 点击运行按钮，将手动 Tag 写入当前仿真状态，驱动设备启停表现。
6. 点击“返回编辑”恢复编辑工作区。

## 7. 系统架构

### 7.1 仓库结构

```text
/
├── Cargo.toml                 # Rust workspace
├── crates/
│   └── hmi-core/              # 纯领域层，不依赖 Dioxus/Bevy
│       ├── model.rs           # Project / Screen / Node / Binding
│       ├── tags.rs            # Tag 定义、值、快照
│       ├── simulation.rs      # Tag 仿真引擎
│       ├── bindings.rs        # 绑定解析和 VisualPatch
│       ├── commands.rs        # 编辑命令、撤销/重做
│       ├── validation.rs      # 工程校验和导入校验
│       └── persistence.rs     # JSON 序列化与 schema 版本
├── apps/
│   └── studio/
│       ├── Dioxus.toml
│       ├── public/assets/     # Bevy 字体、GLB 和静态资源
│       ├── src/main.rs        # Dioxus 入口 + Bevy 启动
│       ├── src/bridge/        # 单线程状态桥和全局信号
│       ├── src/ui/            # Dioxus 编辑器与运行器界面
│       └── src/scene/         # Bevy 2D/3D 插件和渲染系统
├── spikes/dioxus-bevy/        # 一次性验证，不参与生产构建
├── xtask/                     # cargo x 开发、打包、验证与端到端测试入口
└── docs/
```

### 7.2 分层

1. **领域层 `hmi-core`**
   - 拥有工程文档、Tag、绑定、命令、撤销和校验。
   - 不引用 Dioxus、Bevy、浏览器 API。
   - 所有数据和转换均可单元测试。
2. **桥接层 `bridge`**
   - 拥有唯一编辑器状态。
   - 将 Dioxus 命令转成领域命令。
   - 将 Bevy 拾取和拖拽结果转成领域命令。
   - 通过 Revision 信号通知 Dioxus 重绘、通知 Bevy 重建场景。
3. **Dioxus UI 层**
   - 渲染编辑器面板、检查器、Tag 面板、运行态外壳。
   - 不直接持有唯一的工程副本。
4. **Bevy 场景层**
   - 读取工程快照和 Tag 快照。
   - 生成 2D/3D 场景实体。
   - 处理相机、拾取、选择框、动画和绑定应用。
   - 不修改工程文档，只提交编辑命令。

### 7.3 状态流

```text
Dioxus 表单/按钮
        │ EditorCommand
        ▼
   Bridge / EditorState ──────► 撤销栈 / 自动保存
        │ document_revision            │
        │ tag_revision                 │
        ├──────────────► Dioxus 重新渲染
        │
        └──────────────► Bevy 缓存快照并同步场景
                              │
                              ├─ 2D/3D 拾取
                              ├─ 拖拽中的本地预览
                              └─ PointerRelease 后提交命令
```

### 7.4 桥接规则

- 同一 WASM 内只有一个 `EditorState`，由 `thread_local!` 持有。
- Dioxus 与 Bevy 共享少量 `GlobalSignal`：
  - `document_revision: u64`
  - `tag_revision: u64`
  - `selected_node: Option<String>`
  - `active_screen: Option<String>`
  - `workspace_mode: Edit | Run`
  - `notice_revision: u64`
- 工程文档本身不放进 Dioxus Signal；UI 在 `document_revision` 变化后从桥读取只读快照。
- Bevy 在 `document_revision` 或 `tag_revision` 变化后刷新缓存，不在每一帧解析完整 JSON。
- Bevy 拖拽期间只更新本地实体 Transform；释放鼠标时提交一次 `UpdateNodeTransform` 命令。
- 所有桥接读写均在同一条 WASM 线程完成，不使用锁和跨线程共享内存。

## 8. 领域模型

### 8.1 顶层对象

```text
ProjectDocument
├── schema_version: u32
├── id: String
├── name: String
├── design_size: [u32; 2]        # 固定 1600 × 900
├── tags: Vec<TagDefinition>
├── screens: Vec<Screen>
└── startup_screen: String
```

### 8.2 画面

```text
Screen
├── TwoD { id, name, background, nodes: Vec<UiNode> }
└── ThreeD { id, name, background, nodes: Vec<DeviceNode> }
```

- 一个工程至少保留一个画面。
- 画面 ID 在工程内唯一。
- 删除启动画面时，必须同时指定新的启动画面。

### 8.3 2D 节点

```text
UiNode
├── id: String
├── name: String
├── rect: { x, y, width, height }   # 左上角原点，设计像素
├── rotation_deg: f32
├── z: i32
├── visible: bool
├── locked: bool
├── widget: Widget2D
└── bindings: Vec<Binding>
```

`Widget2D`：

- `Panel { fill, stroke, stroke_width }`
- `Label { text, font_size, color, align }`
- `ValueCard { label }`
- `StatusLamp { label }`
- `BarGauge { min, max, direction }`
- `Button { label, action }`
- `Pipe { points, thickness, color }`
- `EquipmentSymbol { equipment, label }`

`EquipmentSymbol` 支持 `Fan`、`Pump`、`Ahu`、`Valve`，由 Bevy 使用基础几何图形绘制，不依赖图片资产。

`ValueCard` 的数值文本和 `StatusLamp`/`EquipmentSymbol` 的颜色由绑定提供。未绑定颜色时使用主题中性色；未绑定文本时显示 `--`。

### 8.4 3D 节点

```text
DeviceNode
├── id: String
├── name: String
├── position: [f32; 3]
├── rotation_deg: [f32; 3]
├── scale: [f32; 3]
├── visible: bool
├── locked: bool
├── label: Option<String>
├── model: DeviceModel
└── bindings: Vec<Binding>
```

`DeviceModel`：

- `Ahu { size, color }`
- `Fan { size, color }`
- `Pump { size, color }`
- `Duct { length, diameter, color }`
- `Pipe { length, diameter, color }`
- `Valve { size, color }`
- `Glb { path }`

`Glb.path` 必须是同源静态资源路径，例如 `models/ahu.glb`。首期不支持浏览器本地文件上传。

### 8.5 Tag

```text
TagDefinition
├── id: String
├── name: String
├── value_type: Number | Bool | Enum
├── unit: String
├── min: Option<f32>
├── max: Option<f32>
├── precision: u8
├── initial: TagValue
├── writable: bool
└── simulator: Constant | Sine | Pulse | Step | RandomWalk | Manual
```

`TagValue`：

- `Number(f32)`
- `Bool(bool)`
- `Enum(String)`

`initial` 必须是未加标签的 JSON 数值、布尔值或字符串，并且与 `value_type` 一致。

Tag 运行时值不进入工程 JSON；工程只保存定义和初值。运行态手动写入值不跨刷新保留。

### 8.6 绑定

```text
Binding
├── tag_id: String
├── target: BindingTarget
└── mapping: BindingMapping
```

`BindingTarget`：

- `Text`
- `Color`
- `Visible`
- `Progress`
- `RotationSpeed`

`BindingMapping`：

- `Format { precision: Option<u8>, unit: Option<String> }`
- `BoolColor { true_color, false_color }`
- `ThresholdColor { thresholds }`
- `Range { input_min, input_max, output_min, output_max, clamp }`
- `Direct`

`Format` 中未提供的精度和单位继承 Tag 定义。`Direct` 直接传递 Bool 或 Number 原值，仅用于 `Visible`、`Progress`、`RotationSpeed` 等目标，目标自身负责边界约束。

绑定解析在 `hmi-core` 中完成，输出与 UI 无关的 `VisualPatch`：

```text
VisualPatch
├── node_id
└── value: Text | Color | Visible | Progress | RotationSpeed
```

Bevy 只负责把 `VisualPatch` 应用到实体。这样绑定逻辑可独立测试，也能在后续复用于其他渲染器。

### 8.7 按钮动作

```text
ButtonAction
├── ToggleTag { tag_id }
└── SetTag { tag_id, value }
```

只有 `writable = true` 的 Tag 接受运行态写入。写入会暂停该 Tag 的自动仿真，直到工程重新加载或显式恢复仿真。

### 8.8 JSON 线格式

- 所有 JSON 字段名和枚举值使用 `snake_case`，大小写敏感。
- Rust 枚举统一切换为 `#[serde(rename_all = "snake_case")]`，不使用默认 PascalCase 线格式。
- 导入解析拒绝未知字段，避免拼写错误被静默忽略。
- 颜色统一使用 `#RRGGBB` 字符串。

带标签枚举的固定结构：

- `Screen` 使用 `kind` 标签：`"two_d"`、`"three_d"`。
- `Widget2D` 使用 `type` 标签：`"panel"`、`"label"`、`"value_card"`、`"status_lamp"`、`"bar_gauge"`、`"button"`、`"pipe"`、`"equipment_symbol"`。
- `DeviceModel` 使用 `type` 标签：`"ahu"`、`"fan"`、`"pump"`、`"duct"`、`"pipe"`、`"valve"`、`"glb"`。
- `BindingTarget`：`"text"`、`"color"`、`"visible"`、`"progress"`、`"rotation_speed"`。
- `BindingMapping` 使用 `type` 标签：`"format"`、`"bool_color"`、`"threshold_color"`、`"range"`、`"direct"`。
- `ButtonAction` 使用 `type` 标签：`"toggle_tag"`、`"set_tag"`。
- `TagDefinition.value_type`：`"number"`、`"bool"`、`"enum"`。
- `TagDefinition.simulator`：`"constant"`、`"sine"`、`"pulse"`、`"step"`、`"random_walk"`、`"manual"`。
- `Label.align`：`"left"`、`"center"`、`"right"`。
- `BarGauge.direction`：`"horizontal"`、`"vertical"`。
- `EquipmentSymbol.equipment`：`"fan"`、`"pump"`、`"ahu"`、`"valve"`。
- `ThresholdColor.thresholds` 为 1–8 个按 `value` 升序排列的 `{ "value": number, "color": "#RRGGBB" }`；输入取其小于等于输入值的最后一项颜色，若低于全部阈值则取第一项颜色。

### 8.9 工程 JSON 示例

```json
{
  "schema_version": 1,
  "id": "project.bas_demo",
  "name": "车站 BAS 环控示例",
  "design_size": [1600, 900],
  "startup_screen": "screen.bas_2d",
  "tags": [
    {
      "id": "bas.supply_fan.run",
      "name": "送风机运行",
      "value_type": "bool",
      "unit": "",
      "precision": 0,
      "initial": false,
      "writable": true,
      "simulator": "manual"
    }
  ],
  "screens": [
    {
      "kind": "two_d",
      "id": "screen.bas_2d",
      "name": "环控工艺图",
      "nodes": [
        {
          "id": "node.supply_fan",
          "name": "送风机 SF-01",
          "rect": { "x": 360, "y": 220, "width": 160, "height": 120 },
          "rotation_deg": 0,
          "z": 10,
          "visible": true,
          "locked": false,
          "widget": { "type": "equipment_symbol", "equipment": "fan", "label": "SF-01" },
          "bindings": [
            {
              "tag_id": "bas.supply_fan.run",
              "target": "color",
              "mapping": {
                "type": "bool_color",
                "true_color": "#37e6d4",
                "false_color": "#5f7780"
              }
            }
          ]
        }
      ]
    }
  ]
}
```

## 9. 渲染设计

### 9.1 2D 画面

- 设计空间固定 1600×900，原点左上角，Y 轴向下。
- 渲染时转换为 Bevy 2D 世界坐标，中心为原点，Y 轴向上。
- 相机使用正交投影与 `ScalingMode::AutoMin { min_width: 1600.0, min_height: 900.0 }`，完整容纳 1600×900 设计空间并按容器等比缩放、居中，多余区域留黑边。
- 控件使用 `Mesh2d`、`ColorMaterial` 和 `Text2d` 绘制。
- 面板使用矩形填充和描边；设备符号由圆形、矩形、三角形和线段组合。
- 管线按折线点生成细长矩形或三角带网格。
- 文本使用内置开源中文字体，不依赖系统字体。
- 选中框、缩放手柄和网格线由独立编辑器覆盖实体绘制，不进入运行态。

### 9.2 2D 拾取与编辑

- 2D 拾取不使用 Bevy UI 或 Sprite picking。
- 使用活动相机把指针转换到设计坐标，按 `z` 从高到低对节点矩形和管线做命中测试。
- 选中后显示矩形描边和四角缩放手柄。
- 拖动主体提交移动预览，拖动四角提交尺寸预览；鼠标释放时各提交一次命令。
- 画布支持中键或空格拖拽平移，滚轮缩放，`F` 聚焦选中对象。

### 9.3 3D 画面

- 使用透视相机和低多边形 PBR 材质。
- 设备由 Bevy 基础几何体组合；GLB 异步加载后挂到场景根节点。
- 风机、水泵和 AHU 内部转子由 Tag 驱动旋转。
- 设备状态通过材质颜色和状态灯表现。
- 选中高亮使用地面光环或描边辅助实体，不改变模型资产。
- 轨道相机支持旋转、缩放和平移，俯仰角和距离有上下限。
- 设备标签使用面向相机的 `Text2d` 广告牌。

### 9.4 场景同步

- 活动画面切换、工程结构变化或编辑模式切换时，销毁并重建当前场景。
- 仅 Tag 更新时不重建场景，只应用 `VisualPatch`。
- `VisualPatch` 通过实体上的 `HmiNodeId` 定位目标。
- 场景实体统一挂到 `HmiSceneRoot` 下，切换画面时整棵子树销毁。
- 缺失 GLB 或字体时生成占位图形并显示非阻塞警告。

## 10. 编辑器界面

### 10.1 布局

- 目标分辨率：1440×900；最低 1280×720。
- 顶栏：品牌、工程名、Undo/Redo、保存、导入/导出、编辑/运行切换、WebGPU 状态。
- 左栏：画面树、组件面板、Tag 面板三段式。
- 中栏：Bevy 视口、画面页签、视图控制。
- 右栏：属性检查器和数据绑定检查器。
- 底部状态条：选中对象、缩放比例、Tag 仿真状态、保存状态。

### 10.2 交互要求

- 点击场景对象同步左侧画面树选中和右侧检查器。
- 点击画面树节点同步场景选中并聚焦。
- 添加控件时放到当前视口中心，若重叠则偏移 24 像素。
- 属性输入使用本地草稿，Enter 或失焦提交，避免每次按键重建场景。
- 删除键只删除当前选中且未锁定的对象；输入框聚焦时不响应场景快捷键。
- 撤销/重做只作用于工程命令，不改变 Tag 运行时值和选中状态。
- 删除被绑定引用的 Tag 时阻止操作，并列出引用节点。
- 工程中至少保留一个画面；删除最后一个画面时阻止操作。

### 10.3 视觉方向

- 工业深色界面，青色作为主强调色，琥珀色表示警告，红色表示故障。
- Dioxus 外壳采用清晰的信息层级、紧凑的行高和明显选中态。
- 2D 画面使用高对比设备符号和简洁管线，不使用装饰性动画。
- 3D 画面使用冷色环境、清晰轮廓和少量发光状态色，避免游戏化效果。
- 中文为第一语言，英文仅用于技术缩写和短标签。

## 11. 运行器

- 运行态全屏显示当前画面，隐藏编辑面板和网格。
- 顶部运行栏显示站点名、当前时间、画面切换、仿真状态和返回编辑。
- Tag 仿真默认每 250 ms 推进一次。
- 编辑模式也运行 Tag 仿真，便于实时预览绑定效果。
- 点击 2D 设备符号或 3D 设备显示设备详情浮层，包含名称、状态、主要 Tag 值和绑定摘要。
- 运行态按钮调用 `ButtonAction`，只写可写 Tag 的仿真值。
- 运行态不修改工程、不写撤销栈、不触发自动保存。

## 12. 持久化与撤销

### 12.1 持久化

- localStorage key：`mox-hmi.project.v1`。
- 自动保存延迟 500 ms，只在工程命令成功后执行。
- 启动顺序：
  1. 读取并校验 localStorage。
  2. 成功则加载保存工程。
  3. 失败则加载内置 BAS 示例并显示警告。
- 导出文件名：`<project-slug>.moxhmi.json`。
- 导入只接受 schema v1，最大 5 MB，先校验再替换当前工程。
- 未知 schema 版本、非法 JSON、引用不完整的绑定都不覆盖当前工程。

### 12.2 撤销

- 使用工程快照方式实现，最多保留 50 步。
- 每次新命令清空 redo 栈。
- 撤销范围包括节点、画面、Tag 和绑定变更。
- 撤销范围不包括选中状态、相机、运行模式和 Tag 运行时值。
- 撤销后自动保存最新快照。

## 13. 错误处理与兼容性

### 13.1 启动

- Dioxus 在启动 Bevy 前检测 `navigator.gpu` 和 adapter。
- 不支持 WebGPU 时显示兼容性页面，说明需要 Chromium 120+、HTTPS/localhost 和可用 GPU。
- Bevy 初始化超时或失败时显示错误覆盖层和重新加载按钮。
- `console_error_panic_hook` 与 DOM 错误覆盖层保证 panic 不只表现为白屏。

### 13.2 日志

- 只初始化一个 tracing subscriber。
- 使用 Bevy `LogPlugin` 作为统一日志入口，不初始化 Dioxus logger。
- Dioxus 业务错误通过桥接通知写入日志和 UI notice。

### 13.3 资源与数据错误

- 字体加载失败：使用 Bevy 默认字体并显示警告；中文可能显示缺字。
- GLB 加载失败：显示线框占位盒并记录路径。
- 绑定无效：保留绑定数据，场景使用默认外观，检查器标红。
- 自动保存失败：保留内存工程，显示“未保存”状态。
- 导入失败：显示具体校验错误，当前工程不变。

### 13.4 浏览器与部署要求

- Chromium 内核 Chrome/Edge 120+。
- WebGPU 已启用且 GPU 未被策略禁用。
- 生产环境必须通过 HTTPS 或 localhost 提供页面。
- 页面 CSP 必须允许 WebAssembly 加载，至少允许 `wasm-unsafe-eval`。
- 静态站点需要支持自定义 base path，以便部署在子路径。

## 14. 性能与体积预算

### 14.1 场景预算

- 单个 2D 画面最多 200 个节点。
- 单个 3D 画面最多 80 个设备节点。
- 单个工程最多 300 个 Tag。
- 3D 模型单文件不超过 20 MB。
- 目标帧率：1440×900 下 45–60 FPS。
- Tag 更新不触发全场景重建。

### 14.2 构建预算

- 关闭 Bevy audio、gamepad、UI 和不使用的格式特性。
- Release WASM 目标不超过 30 MB，优先压到 25 MB 以内。
- 静态资源使用 Brotli/gzip 传输；`wasm-opt -Oz` 作为发布构建步骤。
- 中文字体使用 OFL 字体，并保留许可证文件。

## 15. 测试与验收

### 15.1 单元测试

- 工程 JSON 序列化/反序列化 round-trip。
- schema 版本、重复 ID、缺失画面、非法绑定校验。
- Tag 仿真在固定时间步下的确定性输出。
- 数值映射、布尔颜色、阈值颜色和格式化。
- 按钮动作写入可写 Tag，不可写 Tag 返回错误。
- 撤销/重做、删除节点、复制节点、删除 Tag 的引用保护。
- GLB 路径和绑定目标的校验。

### 15.2 浏览器端到端测试

使用 `xtask` 内置的 Rust Chrome DevTools 驱动和启用 WebGPU 的 Chrome for Testing。测试至少覆盖：

1. 页面加载成功，`navigator.gpu` 可用，日志包含 `BrowserWebGpu`。
2. 默认 BAS 示例可见，2D 画面和 3D 画面均可切换。
3. 选择 2D 控件后检查器显示对应属性。
4. 修改位置和 Tag 绑定后视口更新，撤销可恢复。
5. 添加数值卡、保存、刷新后工程保留。
6. 切换到 3D，点击设备后选中状态同步到 Dioxus。
7. Tag 仿真变化驱动 2D 数值和 3D 风机/水泵动画。
8. 运行模式隐藏编辑面板；按钮写入 Tag 后设备状态变化。
9. 导入非法 JSON 时显示错误且当前工程不丢失。

### 15.3 验收标准

MVP 只有在以下条件全部满足时视为完成：

- 演示工程可在目标 Chrome/Edge 环境稳定打开。
- 2D 与 3D 画面均能显示、切换和响应 Tag 数据。
- 编辑器完成添加、选择、移动、缩放、属性编辑、绑定、复制、删除、撤销/重做。
- 工程能自动保存、导出、导入，并在刷新后恢复。
- 运行模式可交互，且不修改工程数据。
- 单元测试和 `cargo x e2e` / `cargo x control-e2e` 端到端测试通过。
- 无未处理的控制台错误；WebGPU 不支持时有明确提示。
- 1440×900 视觉检查通过，界面达到可演示质量。

## 16. 风险与应对

| 风险 | 影响 | 应对 |
| --- | --- | --- |
| Dioxus 与 Bevy 事件循环升级不兼容 | 阻塞整体方案 | 固定版本；桥接层隔离；E2E 覆盖启动和双向同步 |
| WebGPU 在部分现场环境不可用 | 无法启动 | 启动预检、明确兼容性页面、锁定目标浏览器策略 |
| WASM 体积过大 | 首屏慢 | 显式 feature 集、关闭 audio/UI、wasm-opt、资源压缩 |
| 中文正文字体体积大 | 增加下载量 | 选择 OFL 字体、评估常用字符子集、保留许可证 |
| 2D 文本和符号绘制工作量 | 编辑器延期 | 优先使用矩形/圆/线段组合；不引入图片编辑器 |
| Bevy 2D 拾取复杂 | 编辑交互不稳定 | 自定义设计坐标命中测试，不依赖 Bevy UI |
| GLB 资源路径在不同部署 base path 下失效 | 3D 资产不显示 | 集中 AssetResolver；E2E 覆盖子路径构建 |
| Tag 数据层后续接入协议差异大 | 返工 | 领域层只依赖 TagSnapshot 和 VisualPatch，协议适配器独立 |

## 17. 已确认边界

- 2D 工艺画面与 3D 数字孪生是两张可切换画面，不在同一画布混排。
- 首期演示场景为地铁车站 BAS 环控系统。
- 首期使用仿真 Tag，不接入真实协议。
- 首期使用 WebGPU，不提供 WebGL2 回退。
- 首期采用 Dioxus 编辑器外壳与 Bevy 场景内核。
