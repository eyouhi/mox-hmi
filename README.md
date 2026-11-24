# MOX HMI Studio

面向申通地铁车站 BAS 场景的 HMI 编辑器与运行器 MVP。编辑器是 Dioxus Native 0.8 的 macOS 原生窗口，内嵌 Bevy WebGPU 2D/3D 场景、触控板触觉反馈和 AI 控制隧道；运行器是独立的 Dioxus Web 0.8 + Bevy WebGPU WASM 站点，可通过 `?runtime=1` 直接进入只读运行态，并在支持 WebMCP 的浏览器中暴露页面内 AI 工具。两端共享 `hmi-ui` 界面层与 Bevy 场景层。

## 已实现能力

- 2D 环控工艺图：设备、管线、数值卡、状态灯、仪表条、按钮和基础图形编辑。
- 2D 管线支持逐折点拖拽、`Shift` 轴锁定，端点吸附到设备边缘；3D 设备拖动吸附场景中心面，并可从检查器一键对齐 X/Y/Z 中心面。
- 可从左侧“画面 +”新建空白 2D 画面或 3D 机房画面；3D 设备库包含 AHU、风机、水泵、风管、水管和阀门。
- 2D 节点拖拽支持画板与节点中心吸附、琥珀色对齐线，以及吸附瞬间的音效和线性马达反馈。
- 2D/3D 删除节点时会召唤吃豆人逐个吃掉设备残影，带咀嚼音效和触觉反馈；可在顶栏“吃豆人”开关中关闭。
- 3D 机房数字孪生：AHU、风机、水泵、风管、水管、阀门，以及 GLB 设备加载入口。
- 2D/3D 均支持鼠标、MacBook 触控板、键盘和 PS5 DualSense；应用内按 `F1` 或手柄 Create 打开完整操作指引，按 Options 打开手柄菜单。
- Tag 数据字典、模拟器、输入/输出映射和画面绑定；编辑态与运行态均以 4 Hz 推进仿真。
- 画面/节点增删改、Tag 增删改、绑定编辑、撤销/重做、JSON 导入导出、localStorage 自动保存。
- 运行态只读画面、Tag 镜像、按钮写 Tag、设备详情卡；运行交互不会进入工程、撤销栈或自动保存。
- 仅 WebGPU，无 WebGL2 回退；Dioxus 与 Bevy 共用稳定的 `#bevy-canvas`，切换编辑/运行不重建画布。
- macOS 原生编辑器内嵌 WebSocket 控制隧道和 MCP streamable HTTP；无需单独启动控制 CLI。
- 中心吸附在 macOS 上通过 `NSHapticFeedbackManager` 触发触控板触觉反馈，非 Mac Web 环境继续使用 Vibration API 或静默降级。

## 可访问性与响应式

- 文档声明 `zh-CN`、深色 `color-scheme` 和首屏启动壳，CSS/字体加载前不会出现白屏闪烁；本应用只提供深色主题。
- 页面提供跳过链接、单一 `main`、按层级组织的标题、命名的侧栏和导航；画面切换使用 `aria-current`，不伪装成 ARIA tabs。
- 所有表单控件都有显式标签或可访问名称；数值字段在失焦/回车校验失败时设置 `aria-invalid`、关联并播报可见错误文本。自定义回车提交会检查 IME 组合状态。
- WebGPU 画布可获得键盘焦点；2D 支持方向键轻推、空白处拖拽平移视图和 `Option/Alt` + 拖拽框选；3D 支持空白处拖拽旋转视角、`Option/Alt` + 拖拽框选、方向键移动、`Q`/`E` 升降、`WASD` 平移观察中心、`Option/Alt` + 方向键旋转视角、`+`/`-` 缩放和 `Home` 重置。运行态同时提供隐藏的完整 Tag 数据列表，避免画布成为唯一信息源。
- 顶栏的“操作指引”按钮和 `F1` 可达完整键盘、鼠标、触控板与手柄映射；手柄连接状态在顶栏实时显示。编辑命令 500 ms 防抖自动保存，因此顶栏不再保留冗余的“保存”按钮。
- 字号使用 `rem`，控件高度随文本增长；支持窄窗单列布局，并为 200% 文本缩放预留重排，同时适配减少动效、增强对比度和系统强制颜色模式。

## 技术栈

- Rust nightly（Edition 2024），开发构建启用并行前端与泛型共享。
- Bevy `0.20.1`，`webgpu`、2D/3D PBR、`bevy_picking`、GLB。
- 编辑器：Dioxus Native `0.8.0-alpha.3` + Blitz，Bevy `0.20.1` 通过共享 wgpu 设备渲染到原生纹理。
- 运行器：Dioxus Web `0.8.0-alpha.3` + Bevy WebGPU，输出静态 WASM 站点。
- `winit` 原生窗口、`wasm32-unknown-unknown` + `wasm-bindgen`。

### 目录与职责

```text
crates/hmi-core/   领域模型、校验、Tag 仿真、命令与撤销、持久化
crates/hmi-ui/     Dioxus UI、桥接信号、编辑器和运行器布局
apps/studio/       Bevy 场景层、网格渲染/拾取、Web 入口
apps/editor/       Dioxus Native 窗口、内嵌 Bevy 画布、控制服务与触觉反馈
apps/control/      控制 Hub、WebSocket、MCP stdio/HTTP 传输
xtask/             统一开发入口：运行、编辑、打包、验证、E2E、字体与体积检查
docs/              设计规格与实施计划
spikes/            历史技术验证，仅作参考，不参与生产构建
```

`hmi-ui` 与 Bevy 保持 crate 边界，避免 UI 依赖和场景依赖互相污染，也让 Web 运行器与原生编辑器共用同一份界面代码。

编辑器采用 **Dioxus Native 0.8 原生外壳**，Blitz 负责 UI，Bevy 通过 `ManualTextureView` 渲染到共享 wgpu 纹理并作为 custom widget 嵌入。运行器保持 **Dioxus Web 0.8 + Bevy WebGPU**，两者共享同一套 UI 和场景层。

## 本地运行

前置：

```bash
cargo install dioxus-cli --version 0.8.0-alpha.3 --locked --force
```

工具链由 `rust-toolchain.toml` 固定为 nightly，并自动安装 `wasm32-unknown-unknown`。`.cargo/config.toml` 启用 nightly 并行前端（`-Zthreads=8`）和跨 crate 泛型共享（`-Zshare-generics=yes`）。日常调试的 dev profile 只保留行号调试信息，第三方依赖使用 `opt-level=1`，避免 Bevy 全量优化拖慢增量编译；release 打包仍使用体积优化。

浏览器验收会优先使用 `CHROME_PATH`，否则查找 `/tmp/chrome-dl/.../Google Chrome for Testing` 或本机 Google Chrome；需要 Chrome 120+。`check-wasm-size` 额外需要 Binaryen 的 `wasm-opt`。

开发命令：

```bash
cargo x edit
cargo x run
cargo x bundle
cargo x verify
cargo x clean
```

- `cargo x edit`：以快速 debug 构建启动 macOS 原生编辑器，带触觉反馈和控制隧道；`cargo x edit --release` 可预览接近打包版本的效果。
- `cargo x run`：只启动浏览器运行器，强制进入 `?runtime=1`，不显示编辑器面板、AI 隧道或退出运行按钮。
- `cargo x bundle`：生成 `dist/MOX HMI Studio.dmg` 和 `dist/runner-web/`，两者都使用优化后的 release 构建，不与开发构建混用。
- `cargo x verify`：依次执行格式检查、workspace 测试、WASM 编译检查和两套浏览器验收；浏览器验收默认走快速 debug 构建，`--quick` 可跳过浏览器部分。
- `cargo x clean`：删除 workspace 下的 `target/` 和 `dist/` 构建产物，不修改源码与工程数据。

常用参数：

```bash
cargo x edit --port 17900
cargo x edit --release
cargo x run --port 9000 --no-open
cargo x run --release
cargo x e2e
cargo x e2e --release
cargo x bundle
```

项目内的 `.cargo/config.toml` 将 `cargo x` 映射到 workspace 的 `xtask`。`cargo x run` 默认打开 `http://127.0.0.1:8088/?runtime=1`。

需要修改 Web UI 并使用热更新时，仍可直接使用 Dioxus CLI：

```bash
cd apps/studio
dx serve --web --port 8088
```

浏览器需为桌面版 Chrome/Edge 120+，并在可用 GPU 上启用 WebGPU；生产环境必须使用 HTTPS。Safari、Firefox 和移动端不在本期支持范围内。窄窗口会切换为纵向布局，但触屏和平板不是验收目标。

触控板线性马达反馈只在 `cargo x edit` 启动的 macOS 原生编辑器中可用；`cargo x run` 是浏览器运行器。PS5 DualSense 在原生编辑器优先通过 Apple GameController 框架读取，并通过 GameController 暴露的 CoreHaptics 引擎触发短振；无法使用时回退到 Bevy `bevy_gilrs` rumble。Web 运行器通过 Gamepad API 的 `vibrationActuator` 尽力触发振动。不支持时静默降级。

中心吸附音效使用 Web Audio，并在拖拽开始的手势内初始化；震动反馈通过 Vibration API 尽力触发，macOS 桌面 Chrome/Edge 通常不暴露该能力。

### macOS 原生编辑器

构建带应用图标的 `.app` 与 DMG：

```bash
cargo x bundle
open "target/MOX HMI Studio.app"
```

原生编辑器使用 Dioxus Native + 共享 wgpu 设备，不依赖 WebView；编辑界面直接由原生窗口承载，本地控制服务默认监听 `127.0.0.1:17880`，并提供：

- WebSocket 控制隧道：`ws://127.0.0.1:17880/control`
- MCP streamable HTTP：`http://127.0.0.1:17880/mcp`
- 触觉反馈：`POST http://127.0.0.1:17880/haptic`
- 健康检查：`GET http://127.0.0.1:17880/health`

可用 `--port` 和 `--token` 覆盖默认值。触觉反馈在支持 Force Touch 的 MacBook 触控板上生效；外接普通鼠标或没有线性马达的设备会静默降级。

应用图标源位于 `apps/editor/assets/AppIcon.png`，打包时自动生成 `AppIcon.icns` 并写入应用包。

### Web 运行器

运行器使用独立的 Dioxus Web 0.8 构建，可直接作为静态站点发布：

```text
http://127.0.0.1:8088/?runtime=1
```

该入口强制进入只读运行态，隐藏编辑器面板、AI 隧道和“退出运行”按钮。后续将增加 Dioxus Fullstack 服务端，用工程 ID、版本和权限控制运行器的数据来源；当前 MVP 仍从同源 localStorage 或内置示例加载工程。

`cargo x bundle` 会输出 `dist/runner-web/`。该目录的 `index.html` 已内置运行态启动标记，直接部署这个目录即可，不需要再追加查询参数。

## 操作指引

应用内点击顶栏“操作指引”、画布右下角“操作指引 · F1”，或按 `F1` 打开分章节指引。手柄连接后按 Create 也可以打开；指引内用 `L1` / `R1` 或方向键 `←` / `→` 切换章节，用方向键 `↑` / `↓` 或左摇杆滚动正文，Circle 或再次按 Create 关闭。点击画布后键盘快捷键生效；输入框、下拉框或可编辑文本获得焦点时，场景快捷键和手柄编辑会暂停。

### 新建和编辑画面

1. 左侧“画面”标题栏点击 `+`，选择“空白 2D 画面”或“3D 机房画面”。
2. 2D 从“2D 图元”添加面板、文本、数值卡、状态灯、仪表条、按钮、管线和设备符号；3D 从“3D 设备”添加 AHU、风机、水泵、风管、水管和阀门。
3. 在画布或左侧结构树选择节点。2D 可直接拖动、缩放和旋转；3D 用键盘、手柄或右侧属性检查器修改位置，旋转、缩放和 GLB 路径在检查器中编辑。
4. 工程自动保存；顶栏“运行”进入只读运行态。3D 画面由 Bevy 自动生成单个地面和灯光，项目 JSON 只保存设备节点。

### 手柄菜单

连接手柄后按 `Options` 打开手柄菜单。菜单把高频操作分成清晰的层级：

- “编辑所选”：复制、剪切、粘贴、原地复制、全选和删除；危险操作不再绑定单个功能键。
- 根菜单直接提供撤销、重做。
- “添加组件”：添加当前画面的 2D 图元或 3D 设备；新节点在当前视野附近自动错开摆放并选中，避免同类设备叠在一起，随后用左摇杆拖动、方向键微调。
- “数据字典”：浏览 Tag 和实时值。
- “画面”：切换上一张 / 下一张，或新建 2D / 3D 画面。
- “手柄操作说明”：打开完整键位指引。

十字键 `↑` / `↓` 选择菜单项，Cross 执行，Circle 返回上一级，再按一次 Options 关闭。画布中按 `△` 可直接打开添加菜单；删除可连按两次 `L3`，第一次会提示确认，2 秒内再次按下才执行。手柄菜单与 Create 帮助页互不抢占按键：菜单打开时 Create 不切换页面。

### 画布通用快捷键

画布获得焦点后使用标准 macOS 编辑快捷键；输入框、下拉框、文本域或可编辑元素获得焦点时，快捷键自动交还给表单。

| 快捷键 | 操作 |
| --- | --- |
| `⌘A` | 全选当前画面的可见节点 |
| `⌘C` / `⌘V` / `⌘X` | 复制 / 粘贴 / 剪切；粘贴生成的 ID 唯一，并保留相对位置 |
| `⌘D` | 原地复制，不改写剪贴板 |
| `⌘Z` / `⌘⇧Z` / `Ctrl+Y` | 撤销 / 重做 |
| `⌘S` | 立即刷写自动保存（日常无需手动操作） |
| `Delete` / `Backspace` | 删除所选节点；锁定节点不会参与组删除 |
| `Esc` | 关闭指引，或取消选择 |

点击选择；`⇧` 或 `⌘` 点击切换多选。空白处普通拖拽平移/旋转视图，`Option/Alt` + 拖拽框选，`⇧` + 框选追加选择。组移动、组轻推、剪切、粘贴和删除都会合并为一条撤销记录。

### 2D 操作

| 输入 | 操作 |
| --- | --- |
| 鼠标 | 左键选择；拖拽已选节点移动整组；单选管线后拖圆形折点逐点编辑，按住 `Shift` 锁定水平/垂直方向；空白处拖拽平移视图；`Option/Alt` + 拖拽框选；拖角点缩放单选节点（管线改为折点手柄）；中键或 `Space` + 左键平移；滚轮缩放。 |
| 触控板 | 单指拖拽已选节点移动整组；空白处单指拖拽平移视图；`Option/Alt` + 单指拖拽框选；双指滚动平移；捏合或 `Cmd(⌘)` + 滚动缩放。 |
| 键盘 | 方向键整组轻推 1 单位，`Shift` + 方向键轻推 10；`Q`/`E` 单选旋转 1°，`Shift` 加速到 15°；`F` 聚焦全部所选节点。复制、删除、撤销等见通用快捷键。 |
| DualSense | 左摇杆移动整组；右摇杆平移；L2/R2 缩放；方向键整组精确轻推（靠近中心线时会正常脱离吸附，不会被吸住）；L1/R1 单选切换；Cross 聚焦；Circle 取消选择；Square 原地复制；Triangle 直接打开添加菜单；L3 连按两次删除所选；Options 打开菜单（编辑、撤销/重做、添加、数据字典、画面）；Create 打开指引；触摸板按下进入参数模式。 |

2D 的组拖动、键盘轻推、方向键和手柄移动都会以选择包围盒中心吸附画板/节点中心线，显示琥珀色对齐线并播放轻提示音。管线折点可独立拖动，端点靠近设备约 10 屏幕像素或进入设备范围时会吸附到最近的设备边缘。macOS 原生编辑器优先触发 DualSense CoreHaptics 短振，同时保留触控板线性马达反馈，Web 端则尝试 `vibrationActuator`。微调时若起点已经落在吸附范围内，会直接采用原始位移，避免“刚离开中心又被吸回”的卡住感。悬停未选节点会显示微弱的多层发光轮廓。

触控板捏合由浏览器 `wheel` 事件中的 `ctrlKey`/`metaKey` 识别，双指平移仍使用普通滚动；不按修饰键时不会被误判为缩放。

### 3D 操作

| 输入 | 操作 |
| --- | --- |
| 鼠标 | 左键单击选择设备；`⇧`/`⌘` + 单击多选；拖拽设备沿地面移动整组，组中心靠近场景中心时吸附两个垂直中心面，拖拽时按 `Shift` 锁定主导地面轴；空白处左键拖拽旋转视角；`Option/Alt` + 左键拖拽框选多选；右键拖拽旋转视角；中键拖拽平移；滚轮拉近/拉远。 |
| 触控板 | 拖拽设备移动整组；空白处单指拖拽旋转视角；`Option/Alt` + 单指拖拽框选多选；双指滚动平移；捏合或 `Cmd(⌘)` + 滚动缩放。 |
| 键盘 | 方向键沿地面 X/Y 整组轻推，`Q`/`E` 调整高度 Z，`Shift` 将步长从 0.1 提升到 1.0；`Option/Alt` + 方向键旋转视角；`WASD` 相对当前视角平移观察中心；`+`/`-` 缩放；`Home` 重置；`F` 聚焦全部所选设备。复制、删除、撤销等见通用快捷键。 |
| DualSense | 左摇杆相对当前镜头方向沿地面移动整组设备，左右不会因视角旋转而颠倒；右摇杆旋转视角；L2/R2 缩放；方向键 `←`/`→` 精确轻推 X，`↑`/`↓` 调整高度 Z；L1/R1 单选切换；Cross 聚焦；Circle 取消选择；R3 重置视角；Square 原地复制；Triangle 直接打开添加菜单；L3 连按两次删除所选；Options 打开菜单（编辑、撤销/重做、添加、数据字典、画面）；Create 打开指引；触摸板按下进入参数模式。 |

摇杆移动采用预览 + 松手提交，一次连续移动只产生一条撤销记录。3D 的编辑器坐标统一按 X 左右、Y 前后、Z 垂直地面显示，画布左下角提供随镜头实时旋转的 XYZ 方位标识。地面拖动会按所选组中心吸附 X=0 / Y=0 两个垂直中心面，并显示琥珀色中心面；右侧“3D 变换”还提供垂直中心面 X=0、垂直中心面 Y=0 和水平中心面 Z=0 的一键对齐，多选时按组中心整体平移。框选使用 `Option/Alt` + 单指/鼠标拖拽，并按设备 8 个包围盒角点的屏幕投影判断命中；鼠标/触控板拖拽整组设备同样只产生一条撤销记录。悬停未选设备会显示微弱的多层包围盒发光轮廓，GLB 子网格也会参与拾取。当前 3D 编辑采用“选择 + 变换 + 属性”的工作流，不提供工业建模软件式多轴 Gizmo；旋转和缩放请使用右侧属性检查器或手柄参数模式。参数模式要求单选且未锁定。

### 手柄连接

在 macOS 蓝牙设置中配对 DualSense，按一下 PS 键唤醒后按任意键；顶栏会显示连接状态。原生编辑器优先通过 Apple GameController 框架读取手柄，并通过该框架为 DualSense 创建 CoreHaptics 引擎，吸附时触发手柄短振；macOS 26+ 的蓝牙 DualSense 不需要“输入监控”权限。框架或控制器不支持触觉反馈时才回退到 Bevy `bevy_gilrs` rumble。Web 运行器通过标准 Gamepad API 读取，并尽力使用 `vibrationActuator`。振动不可用时不影响其余键鼠/触控板操作。

## AI 控制隧道

MOX HMI 提供一条本地控制隧道，让支持 MCP 的 AI 客户端读取工程并执行编辑命令。浏览器仍然是唯一执行端：AI 只能通过桥接层提交 `EditorCommand`，因此现有校验、撤销/重做和自动保存规则不会被绕过。

每次 AI 工具调用都会显示在页面左下角的“AI 操作隧道”面板中，包括操作说明、耗时、成功/失败状态和作用节点。

### 启动控制服务

macOS 编辑器已内嵌控制服务。MCP 默认关闭；启动编辑器后，在“AI 操作隧道”面板中打开 MCP 开关。开关状态会保存在 `~/Library/Application Support/MOX HMI Studio/control-tunnel.toml`，下次启动沿用上次选择。

开启后，面板会显示当前可执行的 Codex 注册命令，例如：

```bash
codex mcp add mox-hmi --url http://127.0.0.1:17880/mcp
```

点击“复制命令”，在终端执行后新开一个 Codex 会话即可连接。若使用 `--token`，面板生成的 URL 会追加经过编码的 `?token=...`。关闭开关后 `/mcp` 和 `/control` 会立即返回不可用。

以下独立控制服务仅用于纯 Web 开发环境或没有原生壳的部署。

先构建一次控制服务：

```bash
cargo build -p mox-hmi-control
```

然后在 MCP 客户端中把控制服务配置为 stdio server。例如：

```json
{
  "mcpServers": {
    "mox-hmi": {
      "command": "/absolute/path/to/mox-hmi/target/debug/mox-hmi-control",
      "args": ["--token", "change-me"]
    }
  }
}
```

Codex CLI 不会自动扫描 Cargo workspace，需要显式注册一次：

```bash
cargo build -p mox-hmi-control
codex mcp add mox-hmi -- "$PWD/target/debug/mox-hmi-control" --token change-me
codex mcp list
```

注册后重启 Codex CLI 或开启新会话，`mox_hmi_*` 工具才会出现在当前会话中。若二进制路径或令牌发生变化，先执行 `codex mcp remove mox-hmi`，再重新注册。

控制服务默认监听 `ws://127.0.0.1:17880/control`。控制隧道默认关闭，需要显式启用；使用默认端口和令牌时，在页面 URL 中追加：

```text
http://127.0.0.1:8088/?control=1&control_token=change-me
```

也可以显式指定隧道地址：

```text
http://127.0.0.1:8088/?control=ws://127.0.0.1:17880/control&control_token=change-me
```

### MCP 工具

- `mox_hmi_status`：查看浏览器连接状态、当前画面、选择、模式和修订号。
- `mox_hmi_list_commands`：列出控制动作与 `EditorCommand` 类型。
- `mox_hmi_inspect_project`：读取完整工程或指定画面。
- `mox_hmi_execute_command`：执行选择、切换模式、编辑节点、写 Tag、撤销/重做、保存等动作。

支持的主要动作：`select_screen`、`select_node`、`set_mode`、`execute_editor_command`、`write_tag`、`undo`、`redo`、`save`、`reset_camera_3d`。

### WebMCP（Web 运行器可选增强）

Web 运行器还会检测 `document.modelContext`，存在时注册同一组 `mox_hmi_*` 工具。WebMCP 不是 MCP 的替代品：它在页面打开期间存在，只支持 tools，不提供 MCP 的 resources/prompts；原生 macOS 编辑器没有 DOM，继续使用上面的 MCP 控制服务。

WebMCP 是 Chrome 的实验能力。本地验收可使用 Chrome 149+ 的源试用，或在启动参数中加入：

```text
--enable-blink-features=WebMCP
```

页面无需额外 URL 参数；打开编辑器或运行器后，可在 DevTools 中检查：

```js
await document.modelContext.getTools()
```

注册的工具与 MCP 一致：

- `mox_hmi_status`、`mox_hmi_list_commands`、`mox_hmi_inspect_project` 标记为只读；工程读取结果标记为不可信内容。
- `mox_hmi_execute_command` 标记为有实际后果的操作，浏览器或智能体可据此要求用户确认。

WebMCP 工具调用仍进入 `run_action` → `EditorCommand` → 校验/撤销栈/自动保存这条链路，不会直接修改文件。页面内的操作同样会显示在“AI 操作隧道”面板中。

控制服务当前只提供明文 WebSocket。生产环境应部署在带 TLS 的反向代理后，使用 `wss://` 并强制配置 `--token`；不要将无令牌端口暴露到非受信网络。

构建静态开发包：

```bash
cd apps/studio
dx build --web
```

输出位于 `target/dx/mox-hmi-studio/debug/web/public`。

## 工程数据

- 固定设计坐标：`1600 x 900`，左上原点，Y 轴向下。
- JSON `schema_version` 为 `1`，字段使用 `snake_case`，拒绝未知字段。
- localStorage key：`mox-hmi.project.v1`。
- 工程命令后 500 ms 防抖自动保存；导入上限 5 MB，校验通过后才替换当前工程。
- 导入失败、未知版本、非法绑定或损坏的自动保存数据均不会覆盖当前有效工程。

## 测试与验收

完整验证：

```bash
cargo x verify
```

该命令依次执行格式检查、workspace 单元测试、WASM 编译检查、21 条浏览器交互验收，以及 MCP 控制隧道验收。浏览器套件会自行启动 `dx serve`，使用 Chrome for Testing 的 WebGPU 参数，并在结束时清理服务器。流程覆盖 WebGPU 启动、2D/3D 编辑、鼠标/触控板/键盘/DualSense、框选、多选、剪贴板、组拖拽、触觉路径、可访问性和窄窗重排。

检查潜在 panic 路径：

```bash
cargo cga check --config-path cga.toml
```

`cga.toml` 只放行标准库、异步运行时、渲染框架和测试工具自身的分配、析构与锁实现；`mox-hmi-*` 业务 crate 不在白名单内。

单独运行 E2E：

```bash
cargo x e2e
```

单独运行控制隧道验收：

```bash
cargo x control-e2e
```

可选环境变量：

- `CHROME_PATH`：Chrome for Testing 可执行文件路径。
- `MOX_HMI_PORT`：`dx serve` 端口。
- `MOX_HMI_URL=http://host/mox-hmi/ cargo x e2e --url-from-env`：验证已有部署或子路径构建。
- `MOX_HMI_URL=http://host/mox-hmi/ cargo x control-e2e --url-from-env`：对已有部署或子路径构建执行 MCP 控制隧道验收。

验收截图输出：

- `/tmp/mox-hmi-2d.png`
- `/tmp/mox-hmi-3d.png`
- `/tmp/mox-hmi-runtime.png`
- `/tmp/mox-hmi-ai-control.png`

失败时生成 `/tmp/mox-hmi-failure.png` 并打印最后 100 行浏览器日志。

## Release 构建与部署

生成可分发产物：

```bash
cargo x bundle
```

输出：

- `dist/MOX HMI Studio.dmg`：带应用图标的 macOS 安装镜像，包含应用和 Applications 快捷方式。
- `dist/runner-web/`：默认以运行态启动的静态 Web 运行器。

构建带自定义子路径的发布包：

```bash
cd apps/studio
dx build --web --release --base-path /mox-hmi/
cd ../..
cargo x check-wasm-size
```

发布输出位于 `target/dx/mox-hmi-studio/release/web/public`。`cargo x check-wasm-size` 使用 `wasm-opt -Oz --strip-debug` 压缩 bundle 中的 `*_bg.wasm`，硬性上限为 30 MiB；需要安装 Binaryen：

```bash
brew install binaryen
```

静态站点将 `public` 内容映射到 `/mox-hmi/`，并满足：

- `.wasm` 返回 `application/wasm`。
- 开启 Brotli 或 gzip（至少压缩 `.wasm`、`.js`、`.css`）。
- CSP 至少允许 `script-src 'self' 'wasm-unsafe-eval'`；Dioxus 若受严格 CSP 限制，还需允许其内联样式。
- 对未知前端路由回退到 `/mox-hmi/index.html`。

资源路径会跟随 `--base-path`；Bevy `AssetPlugin` 也从 Dioxus 运行时读取同一 base path。

## 当前非目标

- Modbus、BACnet、OPC UA 等真实工业协议接入。
- 用户权限、审计、报警管理、脚本引擎、历史库和多用户协作。
- Safari、Firefox、移动端适配及 WebGL2 兼容层。
- 工业级 3D 模型编辑；本期仅提供设备参数和 GLB 加载入口。

## 字体许可

画面与 UI 统一使用完整的 Noto Serif CJK SC（Regular）：Bevy 场景和 macOS 原生 UI 读取 `NotoSerifCJKsc-Regular.otf`，Web UI 使用同款 `NotoSerifCJKsc-Regular.woff2`。完整字体覆盖 20,992 个 CJK 统一表意文字，新增中文文案或用户输入不再缺字。许可文件位于 `apps/studio/public/assets/fonts/OFL.txt`。如需重新下载并生成 Web 字体：

```bash
cargo x font-sync
```
