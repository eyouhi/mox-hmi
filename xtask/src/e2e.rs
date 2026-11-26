use crate::server::{DevServer, choose_port};
use headless_chrome::{
    Browser, LaunchOptions, Tab,
    browser::tab::ModifierKey,
    protocol::cdp::{Emulation, Input, Page},
};
use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, RwLock},
    thread,
    time::{Duration, Instant},
};

const TOTAL_FLOWS: usize = 21;
const DEFAULT_CHROME: &str = "/tmp/chrome-dl/app/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing";
const BROWSER_ARGS: &[&str] = &[
    "--enable-unsafe-webgpu",
    "--enable-features=WebGPU",
    "--enable-blink-features=WebMCP",
    "--ignore-gpu-blocklist",
    "--disable-gpu-sandbox",
    "--disable-background-timer-throttling",
    "--disable-renderer-backgrounding",
    "--disable-backgrounding-occluded-windows",
    "--disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling",
];

const GAMEPAD_MOCK: &str = r#"
(() => {
  const makeButton = () => {
    const button = Object.create(GamepadButton.prototype);
    Object.defineProperties(button, {
      pressed: { value: false, writable: true, configurable: true },
      touched: { value: false, writable: true, configurable: true },
      value: { value: 0, writable: true, configurable: true },
    });
    return button;
  };
  const state = {
    axes: [0, 0, 0, 0],
    buttons: Array.from({ length: 18 }, makeButton),
    lastHaptic: null,
  };
  const pad = Object.create(Gamepad.prototype);
  Object.defineProperties(pad, {
    id: { value: 'DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)' },
    index: { value: 0 },
    connected: { value: true },
    mapping: { value: 'standard' },
    timestamp: { value: performance.now() },
    axes: { get: () => state.axes },
    buttons: { get: () => state.buttons },
    vibrationActuator: { value: {
      playEffect: (type, options) => {
        state.lastHaptic = { type, options };
        return Promise.resolve('complete');
      },
      reset: () => Promise.resolve('complete'),
    } },
  });
  Object.defineProperty(navigator, 'getGamepads', { value: () => [pad], configurable: true });
  window.__moxPadMock = state;
})();
"#;

const LOG_HOOK: &str = r#"
(() => {
  try {
    let logs = [];
    try { logs = JSON.parse(sessionStorage.getItem('__moxE2eLogs') || '[]'); } catch (_) {}
    const format = (value) => {
      if (typeof value === 'string') return value;
      try { return JSON.stringify(value); } catch (_) { return String(value); }
    };
    const push = (kind, values) => {
      try {
        logs.push('[' + kind + '] ' + values.map(format).join(' '));
        if (logs.length > 2000) logs.splice(0, logs.length - 2000);
        sessionStorage.setItem('__moxE2eLogs', JSON.stringify(logs));
      } catch (_) {}
    };
    for (const kind of ['log', 'info', 'warn', 'error']) {
      const original = console[kind].bind(console);
      console[kind] = (...values) => { push(kind, values); original(...values); };
    }
    window.addEventListener('error', (event) => {
      push('pageerror', [event.message + ' @ ' + event.filename + ':' + event.lineno]);
    });
    window.addEventListener('unhandledrejection', (event) => {
      push('pageerror', [String(event.reason)]);
    });
  } catch (_) {}
})();
"#;

macro_rules! ensure {
    ($condition:expr, $($arg:tt)*) => {
        let passed = $condition;
        if !passed {
            return Err(format!($($arg)*));
        }
    };
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

pub struct Harness {
    tab: RwLock<Arc<Tab>>,
    browser: Browser,
    current_flow: usize,
    only_flow: Option<usize>,
    from_flow: Option<usize>,
}

impl Harness {
    fn flow_accessibility_and_reflow(&self) -> Result<(), String> {
        ensure!(
            self.eval_string("document.documentElement.lang")? == "zh-CN",
            "页面 lang 不是 zh-CN"
        );
        self.wait_until("#boot-shell 隐藏", Duration::from_secs(5), || {
            Ok(!self.visible_selector("#boot-shell")?)
        })?;

        self.click("screen-tab-screen-bas-2d")?;
        self.click("node-row-node-ahu")?;
        self.wait_visible("node-name", Duration::from_secs(5))?;

        let audit = self.eval(
            r#"(() => {
              const isVisible = (el) => {
                const style = getComputedStyle(el);
                return style.display !== 'none' && style.visibility !== 'hidden' &&
                  el.getClientRects().length > 0;
              };
              const headings = [...document.querySelectorAll('h1,h2,h3,h4,h5,h6')]
                .filter(isVisible)
                .map((el) => Number(el.tagName.slice(1)));
              const controls = [...document.querySelectorAll('input, select, textarea')]
                .filter((el) => el.type !== 'hidden' && !el.hidden && isVisible(el));
              const badControls = controls
                .filter((el) => !el.labels?.length && !el.getAttribute('aria-label') &&
                  !el.getAttribute('aria-labelledby'))
                .map((el) => el.getAttribute('data-testid') || el.id || el.tagName);
              const buttons = [...document.querySelectorAll('button')].filter(isVisible);
              const badButtons = buttons
                .filter((el) => {
                  const name = (el.getAttribute('aria-label') || el.textContent || '').trim();
                  return name.length < 2;
                })
                .map((el) => el.getAttribute('data-testid') || el.textContent.trim());
              return {
                headings,
                badControls,
                badButtons,
                visibleH1: [...document.querySelectorAll('h1')].filter(isVisible).length,
                mains: document.querySelectorAll('main').length,
                tablists: document.querySelectorAll('[role="tablist"]').length,
              };
            })()"#,
        )?;
        ensure!(
            audit["visibleH1"].as_u64() == Some(1),
            "可见 h1 数量不是 1: {audit}"
        );
        ensure!(
            audit["mains"].as_u64() == Some(1),
            "main 数量不是 1: {audit}"
        );
        ensure!(
            audit["tablists"].as_u64() == Some(0),
            "不应使用 ARIA tablist: {audit}"
        );
        let headings: Vec<u64> = audit["headings"]
            .as_array()
            .map(|items| items.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default();
        let mut previous = 0;
        for level in headings {
            ensure!(level <= previous + 1, "标题层级跳级: {audit}");
            previous = level;
        }
        ensure!(
            audit["badControls"].as_array().is_none_or(Vec::is_empty),
            "存在无可访问名称的控件: {audit}"
        );
        ensure!(
            audit["badButtons"].as_array().is_none_or(Vec::is_empty),
            "存在没有可访问名称的按钮: {audit}"
        );

        self.fill("node-x", "not-a-number")?;
        self.press("Enter")?;
        ensure!(
            self.attr("node-x", "aria-invalid")? == "true",
            "非法数字没有设置 aria-invalid"
        );
        self.wait_visible("error-node-x", Duration::from_secs(5))?;
        self.wait_text("field-error-live", "请输入有效数值", Duration::from_secs(5))?;
        self.fill("node-x", "650")?;
        ensure!(
            self.attr("node-x", "aria-invalid")? == "false",
            "恢复合法数字后 aria-invalid 仍为 true"
        );
        self.wait_until("实时错误信息清空", Duration::from_secs(5), || {
            Ok(self.text("field-error-live")?.is_empty())
        })?;
        self.focus("node-x")?;
        self.press("Escape")?;

        self.set_viewport(720, 900)?;
        self.wait_until("切换到 720x900 视口", Duration::from_secs(5), || {
            self.eval_bool("window.innerWidth === 720 && window.innerHeight === 900")
        })?;
        let _ = self.stable_canvas_rect()?;
        self.wait_visible("left-panel", Duration::from_secs(5))?;
        self.wait_visible("inspector", Duration::from_secs(5))?;
        ensure!(self.no_overflow()?, "720px 视口出现横向溢出");
        self.set_viewport(1440, 900)?;
        self.wait_until("恢复到 1440x900 视口", Duration::from_secs(5), || {
            self.eval_bool("window.innerWidth === 1440 && window.innerHeight === 900")
        })?;
        let _ = self.stable_canvas_rect()?;
        Ok(())
    }

    fn flow_selection_and_clipboard_2d(&self) -> Result<(), String> {
        self.click("screen-tab-screen-bas-2d")?;
        self.wait_text("scene-mode", "2D", Duration::from_secs(5))?;
        thread::sleep(Duration::from_millis(500));

        self.marquee_design((295.0, 235.0), (455.0, 355.0), false)?;
        self.wait_text("selection-id", "node.valve_fresh", Duration::from_secs(5))?;
        self.marquee_design((1180.0, 245.0), (1260.0, 355.0), true)?;
        ensure!(
            self.selection_count()? == 2,
            "Shift + Option 框选没有追加第二个节点"
        );
        self.wait_visible("multi-inspector", Duration::from_secs(5))?;

        let (start_x, start_y) = self.design_to_page(375.0, 300.0)?;
        self.drag(
            (start_x, start_y),
            (start_x + 30.0, start_y + 15.0),
            12,
            None,
        )?;
        thread::sleep(Duration::from_millis(300));
        ensure!(self.selection_count()? == 2, "组拖拽丢失了多选");
        self.focus_selector("#bevy-canvas")?;
        self.press_meta("z")?;
        thread::sleep(Duration::from_millis(250));

        self.click("node-row-node-valve-fresh")?;
        self.wait_value("node-x", "330", Duration::from_secs(5))?;
        self.wait_value("node-y", "260", Duration::from_secs(5))?;
        self.click("node-row-node-valve-return")?;
        self.wait_value("node-x", "1160", Duration::from_secs(5))?;
        self.wait_value("node-y", "260", Duration::from_secs(5))?;

        self.click("node-row-node-valve-fresh")?;
        self.focus_selector("#bevy-canvas")?;
        self.press_meta("c")?;
        self.press_meta("v")?;
        self.wait_text(
            "selection-id",
            "node.valve_fresh.copy1",
            Duration::from_secs(5),
        )?;
        self.wait_visible("node-row-node-valve-fresh-copy1", Duration::from_secs(5))?;
        self.press_meta("z")?;
        self.wait_absent("node-row-node-valve-fresh-copy1", Duration::from_secs(5))?;

        self.click("node-row-node-valve-fresh")?;
        self.focus_selector("#bevy-canvas")?;
        self.press_meta("x")?;
        self.wait_absent("node-row-node-valve-fresh", Duration::from_secs(5))?;
        self.wait_text("selection-id", "未选择", Duration::from_secs(5))?;
        self.press_meta("z")?;
        self.wait_visible("node-row-node-valve-fresh", Duration::from_secs(5))?;

        self.focus_selector("#bevy-canvas")?;
        self.press_meta("a")?;
        ensure!(self.selection_count()? > 10, "Command+A 没有全选可见节点");
        self.press("Escape")?;
        ensure!(self.selection_count()? == 0, "Escape 没有清空选择");

        self.focus_selector("#bevy-canvas")?;
        let rect = self.stable_canvas_rect()?;
        self.drag(
            (rect.x + 20.0, rect.y + 20.0),
            (rect.x + rect.width * 0.82, rect.y + rect.height * 0.85),
            12,
            None,
        )?;
        thread::sleep(Duration::from_millis(250));
        ensure!(
            self.selection_count()? == 0,
            "2D 空白处普通拖拽不应触发框选"
        );
        Ok(())
    }

    fn marquee_design(
        &self,
        start: (f64, f64),
        end: (f64, f64),
        shift: bool,
    ) -> Result<(), String> {
        let rect = self.stable_canvas_rect()?;
        let start = self.design_to_page_with(rect, start.0, start.1);
        let end = self.design_to_page_with(rect, end.0, end.1);
        if std::env::var_os("MOX_E2E_DEBUG").is_some() {
            eprintln!("marquee canvas={rect:?} start={start:?} end={end:?} shift={shift}");
        }
        self.focus_selector("#bevy-canvas")?;
        self.set_alt_held(true)?;
        if shift {
            self.set_shift_held(true, true)?;
        }
        let modifiers = 1 | if shift { 8 } else { 0 };
        let result = self.drag(start, end, 12, Some(modifiers));
        if shift {
            self.set_shift_held(false, true)?;
        }
        self.set_alt_held(false)?;
        if std::env::var_os("MOX_E2E_DEBUG").is_some() && result.is_ok() {
            eprintln!("marquee selection={:?}", self.selection_count());
        }
        result
    }

    fn stable_canvas_rect(&self) -> Result<Rect, String> {
        let mut previous = self.canvas_rect()?;
        self.wait_until("画布布局稳定", Duration::from_secs(5), || {
            thread::sleep(Duration::from_millis(50));
            let current = self.canvas_rect()?;
            let stable = (current.x - previous.x).abs() < 0.5
                && (current.y - previous.y).abs() < 0.5
                && (current.width - previous.width).abs() < 0.5
                && (current.height - previous.height).abs() < 0.5;
            previous = current;
            Ok(stable)
        })?;
        Ok(previous)
    }

    fn flow_selection_and_group_drag_3d(&self) -> Result<(), String> {
        self.click("screen-tab-screen-bas-3d")?;
        self.wait_text("scene-mode", "3D", Duration::from_secs(5))?;
        thread::sleep(Duration::from_millis(1_200));
        let rect = self.canvas_rect()?;

        let gizmo_before = self.axis_gizmo_x_position()?;
        let orbit_start = (rect.x + 20.0, rect.y + 20.0);
        let orbit_end = (rect.x + 110.0, rect.y + 75.0);
        self.drag(orbit_start, orbit_end, 10, None)?;
        thread::sleep(Duration::from_millis(300));
        let gizmo_after = self.axis_gizmo_x_position()?;
        ensure!(
            (gizmo_after.0 - gizmo_before.0).abs() + (gizmo_after.1 - gizmo_before.1).abs() > 1.0,
            "3D 空白处单指拖拽没有旋转视角"
        );

        self.focus_selector("#bevy-canvas")?;
        self.press("Home")?;
        thread::sleep(Duration::from_millis(300));

        let marquee_start = (rect.x + 20.0, rect.y + 20.0);
        let marquee_end = (rect.x + rect.width * 0.82, rect.y + rect.height * 0.85);
        self.set_alt_held(true)?;
        let marquee = self.drag(marquee_start, marquee_end, 16, Some(1));
        self.set_alt_held(false)?;
        marquee?;
        thread::sleep(Duration::from_millis(400));
        ensure!(self.selection_count()? >= 2, "3D 框选没有选中设备");
        self.wait_visible("multi-inspector", Duration::from_secs(5))?;
        self.wait_text("multi-inspector", "AHU-01", Duration::from_secs(5))?;
        self.wait_text("multi-inspector", "SF-01", Duration::from_secs(5))?;

        let start = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        self.drag(start, (start.0 + 90.0, start.1 + 28.0), 12, None)?;
        thread::sleep(Duration::from_millis(500));
        self.click("node-row-node-3d-ahu")?;
        let x_after = self
            .input_value("node-position-x")?
            .parse::<f64>()
            .unwrap_or(0.0);
        ensure!(x_after != 0.0, "拖拽选中的 3D 设备没有移动整组");
        self.click("undo-button")?;
        self.wait_value("node-position-x", "0", Duration::from_secs(5))?;

        self.click("node-row-node-3d-supply-fan")?;
        self.wait_value("node-position-x", "-4.2", Duration::from_secs(5))?;
        Ok(())
    }

    fn flow_shortcut_isolation_in_text_fields(&self) -> Result<(), String> {
        self.click("screen-tab-screen-bas-2d")?;
        self.wait_text("scene-mode", "2D", Duration::from_secs(5))?;
        thread::sleep(Duration::from_millis(500));
        self.click("node-row-node-ahu")?;
        self.focus_selector("#bevy-canvas")?;
        self.press_meta("c")?;

        self.focus("node-x")?;
        self.press_meta("a")?;
        ensure!(
            self.selection_count()? == 1,
            "输入框内 Command+A 选中了画布"
        );
        self.press_meta("d")?;
        ensure!(
            self.count("node-row-node-ahu-copy1")? == 0,
            "输入框内 Command+D 复制了节点"
        );
        self.press_meta("v")?;
        ensure!(
            self.count("node-row-node-ahu-copy1")? == 0,
            "输入框内 Command+V 粘贴了节点"
        );
        self.press("Backspace")?;
        self.wait_visible("node-row-node-ahu", Duration::from_secs(5))?;
        ensure!(self.selection_count()? == 1, "编辑输入框清空了画布选择");
        self.fill("node-x", "650")?;
        self.focus_selector("#bevy-canvas")?;
        Ok(())
    }

    fn flow_input_capability_audit(&self) -> Result<(), String> {
        self.click("screen-tab-screen-bas-2d")?;
        self.wait_text("scene-mode", "2D", Duration::from_secs(5))?;
        thread::sleep(Duration::from_millis(600));
        self.click("node-row-node-ahu")?;
        self.wait_text("selection-id", "node.ahu", Duration::from_secs(5))?;
        ensure!(
            self.eval_string("document.activeElement?.id")? == "bevy-canvas",
            "从场景树选择节点后画布没有获得焦点"
        );

        self.press_meta("d")?;
        self.wait_text("selection-id", "node.ahu.copy1", Duration::from_secs(5))?;
        self.wait_visible("node-row-node-ahu-copy1", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.wait_absent("node-row-node-ahu-copy1", Duration::from_secs(5))?;
        self.click("node-row-node-ahu")?;

        let zoom_before = self.text("zoom-value")?;
        self.dispatch_wheel(-120.0, 4)?;
        self.wait_text_ne("zoom-value", &zoom_before, Duration::from_secs(2))?;
        let zoom_after_wheel = self.text("zoom-value")?;
        self.dispatch_safari_pinch(1.12, 4)?;
        self.wait_text_ne("zoom-value", &zoom_after_wheel, Duration::from_secs(2))?;

        self.press_gamepad_button(2)?;
        self.wait_text("selection-id", "node.ahu.copy1", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.click("node-row-node-ahu")?;

        self.open_gamepad_menu()?;
        self.gamepad_menu_down(1)?;
        self.gamepad_menu_confirm()?;
        self.gamepad_menu_down(5)?;
        self.gamepad_menu_confirm()?;
        self.wait_text("selection-id", "未选择", Duration::from_secs(5))?;
        self.wait_absent("node-row-node-ahu", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.click("node-row-node-ahu")?;
        self.wait_text("selection-id", "node.ahu", Duration::from_secs(5))?;

        self.focus_selector("#bevy-canvas")?;
        self.press("ArrowRight")?;
        thread::sleep(Duration::from_millis(150));
        self.wait_value("node-x", "651", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.wait_value("node-x", "650", Duration::from_secs(5))?;
        self.open_gamepad_menu()?;
        self.gamepad_menu_down(3)?;
        self.gamepad_menu_confirm()?;
        self.wait_value("node-x", "651", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.wait_value("node-x", "650", Duration::from_secs(5))?;

        self.open_gamepad_menu()?;
        self.gamepad_menu_down(6)?;
        self.gamepad_menu_confirm()?;
        self.gamepad_menu_down(1)?;
        self.gamepad_menu_confirm()?;
        self.wait_text("scene-mode", "3D", Duration::from_secs(5))?;
        self.wait_absent("screen-loading", Duration::from_secs(5))?;
        thread::sleep(Duration::from_millis(1_500));
        self.click("node-row-node-3d-ahu")?;
        self.wait_text("selection-id", "node.3d.ahu", Duration::from_secs(5))?;
        let gizmo_before = self.eval(
            "(() => { const el = document.querySelector('.axis-gizmo-label.is-x'); if (!el) return null; const r = el.getBoundingClientRect(); return { x: r.x, y: r.y }; })()",
        )?;
        let position_before = self.input_value("node-position-x")?;
        self.set_gamepad_axis(2, 0.7)?;
        thread::sleep(Duration::from_millis(700));
        self.set_gamepad_axis(2, 0.0)?;
        thread::sleep(Duration::from_millis(400));
        let gizmo_after = self.eval(
            "(() => { const el = document.querySelector('.axis-gizmo-label.is-x'); if (!el) return null; const r = el.getBoundingClientRect(); return { x: r.x, y: r.y }; })()",
        )?;
        ensure!(
            !gizmo_before.is_null() && !gizmo_after.is_null(),
            "3D 坐标方位标识没有布局区域"
        );
        let gizmo_dx =
            gizmo_after["x"].as_f64().unwrap_or(0.0) - gizmo_before["x"].as_f64().unwrap_or(0.0);
        let gizmo_dy =
            gizmo_after["y"].as_f64().unwrap_or(0.0) - gizmo_before["y"].as_f64().unwrap_or(0.0);
        ensure!(
            gizmo_dx.abs() + gizmo_dy.abs() > 1.0,
            "DualSense 右摇杆没有旋转 3D 相机"
        );
        ensure!(
            self.input_value("node-position-x")? == position_before,
            "3D 相机平移移动了选中设备"
        );
        self.press_meta("d")?;
        self.wait_text("selection-id", "node.3d.ahu.copy1", Duration::from_secs(5))?;
        self.wait_value("node-position-x", "0.5", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.wait_text("selection-id", "未选择", Duration::from_secs(5))?;
        self.wait_absent("node-row-node-3d-ahu-copy1", Duration::from_secs(5))?;

        self.open_gamepad_menu()?;
        self.gamepad_menu_down(6)?;
        self.gamepad_menu_confirm()?;
        self.gamepad_menu_confirm()?;
        self.wait_text("scene-mode", "2D", Duration::from_secs(5))?;
        self.wait_absent("screen-loading", Duration::from_secs(5))?;
        self.click("node-row-node-ahu")?;
        self.wait_text("selection-id", "node.ahu", Duration::from_secs(5))?;
        Ok(())
    }
}

impl Harness {
    fn flow_add_and_reload(&self) -> Result<(), String> {
        self.click("add-value-card")?;
        self.wait_text("selection-id", "node.value-card.", Duration::from_secs(5))?;
        let node_id = self.text("selection-id")?;
        let row_testid = format!("node-row-{}", sanitize_test_id(&node_id));
        self.wait_visible(&row_testid, Duration::from_secs(5))?;
        self.wait_text("save-status", "已保存", Duration::from_secs(10))?;
        let saved = self.eval_string("localStorage.getItem('mox-hmi.project.v1')")?;
        ensure!(saved.contains(&node_id), "自动保存内容缺少新节点 {node_id}");

        self.reload()?;
        self.wait_ready()?;
        self.wait_text("scene-mode", "2D", Duration::from_secs(5))?;
        self.wait_visible(&row_testid, Duration::from_secs(5))?;
        let persisted = self.eval_string("localStorage.getItem('mox-hmi.project.v1')")?;
        ensure!(
            persisted.contains(&node_id),
            "刷新后持久化项目缺少 {node_id}"
        );
        Ok(())
    }

    fn flow_select_3d(&self) -> Result<(), String> {
        self.click("screen-tab-screen-bas-3d")?;
        self.wait_text("scene-mode", "3D", Duration::from_secs(5))?;
        thread::sleep(Duration::from_millis(1_500));
        let rect = self.canvas_rect()?;
        self.click_at(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)?;
        self.wait_text("selection-id", "node.3d.ahu", Duration::from_secs(5))?;
        self.wait_text("scene-mode", "3D", Duration::from_secs(5))?;
        self.wait_visible("axis-gizmo", Duration::from_secs(5))?;
        Ok(())
    }

    fn flow_keyboard_move_3d(&self) -> Result<(), String> {
        self.focus_selector("#bevy-canvas")?;
        self.press("ArrowRight")?;
        thread::sleep(Duration::from_millis(150));
        self.wait_value("node-position-x", "0.1", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.wait_value("node-position-x", "0", Duration::from_secs(5))?;
        Ok(())
    }

    fn flow_gamepad_3d(&self) -> Result<(), String> {
        self.wait_text("selection-id", "node.3d.ahu", Duration::from_secs(5))?;
        thread::sleep(Duration::from_millis(250));
        self.set_gamepad_axis(0, 0.6)?;
        thread::sleep(Duration::from_millis(900));
        self.set_gamepad_axis(0, 0.0)?;
        thread::sleep(Duration::from_millis(500));
        let moved = self
            .input_value("node-position-x")?
            .parse::<f64>()
            .unwrap_or(0.0);
        ensure!(moved != 0.0, "DualSense 左摇杆没有移动 3D 设备");
        self.click("undo-button")?;
        self.wait_value("node-position-x", "0", Duration::from_secs(5))?;
        Ok(())
    }

    fn flow_trackpad_move_3d(&self) -> Result<(), String> {
        self.wait_text("selection-id", "node.3d.ahu", Duration::from_secs(5))?;
        self.focus_selector("#bevy-canvas")?;
        let rect = self.canvas_rect()?;
        let start = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        self.drag(start, (start.0 + 96.0, start.1 + 24.0), 12, None)?;
        thread::sleep(Duration::from_millis(450));
        let moved = self
            .input_value("node-position-x")?
            .parse::<f64>()
            .unwrap_or(0.0);
        ensure!(moved != 0.0, "3D 设备拖拽没有移动选中对象");
        self.click("undo-button")?;
        self.wait_value("node-position-x", "0", Duration::from_secs(5))?;

        self.click("node-row-node-3d-valve-chilled")?;
        self.wait_text(
            "selection-id",
            "node.3d.valve_chilled",
            Duration::from_secs(5),
        )?;
        let mut label = Value::Null;
        let mut probe = Value::Null;
        self.wait_until("冷冻水阀标签可见", Duration::from_secs(5), || {
            probe = self.eval(
                r#"(() => {
                    const info = (el) => {
                        if (!el) return null;
                        const rect = el.getBoundingClientRect();
                        const style = getComputedStyle(el);
                        const layer = el.closest('.device-label-layer')?.getBoundingClientRect();
                        const pointX = Number(el.dataset.pointX);
                        const pointY = Number(el.dataset.pointY);
                        return {
                            testid: el.getAttribute('data-testid'),
                            text: el.textContent,
                            classes: el.className,
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height,
                            clientRects: el.getClientRects().length,
                            display: style.display,
                            visibility: style.visibility,
                            opacity: style.opacity,
                            point: layer && Number.isFinite(pointX) && Number.isFinite(pointY)
                                ? {
                                    x: layer.x + pointX / 100 * layer.width,
                                    y: layer.y + pointY / 100 * layer.height,
                                }
                                : null,
                        };
                    };
                    const target = document.querySelector('[data-testid="device-label-node-3d-valve-chilled"]');
                    return {
                        target: info(target),
                        labels: [...document.querySelectorAll('.device-label')].map(info),
                    };
                })()"#,
            )?;
            label = probe["target"].clone();
            Ok(label["width"].as_f64().unwrap_or(0.0) > 0.0
                && label["height"].as_f64().unwrap_or(0.0) > 0.0
                && !label["classes"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("is-hidden")
                && !label["point"].is_null())
        })
        .map_err(|error| format!("{error}; DOM={probe}"))?;
        ensure!(!label.is_null(), "冷冻水阀标签没有布局区域");
        ensure!(!label["point"].is_null(), "冷冻水阀标签没有设备投影点");
        let x_before = self.input_value("node-position-x")?;
        let y_before = self.input_value("node-position-y")?;
        let valve_x = label["point"]["x"].as_f64().unwrap_or(0.0);
        let valve_y = label["point"]["y"].as_f64().unwrap_or(0.0);
        self.drag(
            (valve_x, valve_y),
            (valve_x + 64.0, valve_y + 12.0),
            12,
            None,
        )?;
        thread::sleep(Duration::from_millis(450));
        self.wait_text(
            "selection-id",
            "node.3d.valve_chilled",
            Duration::from_secs(5),
        )?;
        let x_after = self.input_value("node-position-x")?;
        let y_after = self.input_value("node-position-y")?;
        ensure!(
            (x_after.as_str(), y_after.as_str()) != (x_before.as_str(), y_before.as_str()),
            "从场景树选中的冷冻水阀没有移动"
        );
        self.click("undo-button")?;
        self.wait_value("node-position-x", "0", Duration::from_secs(5))?;
        self.wait_value("node-position-y", "1.9", Duration::from_secs(5))?;
        Ok(())
    }

    fn flow_tag_simulation(&self) -> Result<(), String> {
        self.click("run-button")?;
        self.wait_visible("run-shell", Duration::from_secs(5))?;
        let before = (
            self.text("runtime-tag-bas-ahu-supply-temp")?,
            self.text("runtime-tag-bas-supply-fan-freq")?,
        );
        thread::sleep(Duration::from_millis(1_500));
        let after = (
            self.text("runtime-tag-bas-ahu-supply-temp")?,
            self.text("runtime-tag-bas-supply-fan-freq")?,
        );
        ensure!(before != after, "1.5 秒内模拟 Tag 数值没有变化");
        self.click("run-exit")?;
        self.wait_visible("inspector", Duration::from_secs(5))?;
        Ok(())
    }

    fn flow_runtime_write_isolation(&self) -> Result<(), String> {
        self.wait_text("save-status", "已保存", Duration::from_secs(10))?;
        let before = self.eval_string("localStorage.getItem('mox-hmi.project.v1')")?;
        ensure!(!before.is_empty(), "保存项目为空");
        self.click("run-button")?;
        self.wait_visible("run-shell", Duration::from_secs(5))?;
        self.wait_absent("inspector", Duration::from_secs(5))?;
        self.click("run-screen-2d")?;
        thread::sleep(Duration::from_millis(600));
        let fan_before = self.text("supply-fan-state")?;
        self.click("button-supply-fan")?;
        self.wait_text_ne("supply-fan-state", &fan_before, Duration::from_secs(5))?;
        self.click("run-exit")?;
        self.wait_visible("inspector", Duration::from_secs(5))?;
        let after = self.eval_string("localStorage.getItem('mox-hmi.project.v1')")?;
        ensure!(after == before, "运行态交互修改了持久化项目");
        Ok(())
    }

    fn flow_invalid_import(&self) -> Result<(), String> {
        let project_name = self.text("project-name")?;
        self.upload_invalid_import()?;
        self.wait_until("导入错误提示可见", Duration::from_secs(10), || {
            self.eval_bool(
                "!!document.querySelector('[data-testid=\"notice\"][data-level=\"error\"]')",
            )
        })?;
        ensure!(
            self.text("project-name")? == project_name,
            "非法导入改变了项目名称"
        );
        self.wait_text("webgpu-status", "WebGPU ready", Duration::from_secs(5))?;
        Ok(())
    }
}

impl Harness {
    pub fn launch() -> Result<Self, String> {
        let chrome = chrome_path()?;
        let args: Vec<&OsStr> = BROWSER_ARGS.iter().map(OsStr::new).collect();
        let options = LaunchOptions::default_builder()
            .path(Some(chrome))
            .headless(true)
            .sandbox(false)
            .enable_gpu(true)
            .window_size(Some((1440, 900)))
            .idle_browser_timeout(Duration::from_secs(60))
            .args(args)
            .build()
            .map_err(|error| format!("无法创建 Chrome 启动参数: {error}"))?;
        let browser = Browser::new(options).map_err(|error| format!("无法启动 Chrome: {error}"))?;
        let tab = Self::open_tab(&browser)?;
        Ok(Self {
            tab: RwLock::new(tab),
            browser,
            current_flow: 0,
            only_flow: None,
            from_flow: None,
        })
    }

    fn open_tab(browser: &Browser) -> Result<Arc<Tab>, String> {
        let tab = browser
            .new_tab()
            .map_err(|error| format!("无法创建浏览器标签页: {error}"))?;
        tab.activate()
            .map_err(|error| format!("无法激活浏览器标签页: {error}"))?;
        tab.call_method(Page::AddScriptToEvaluateOnNewDocument {
            source: GAMEPAD_MOCK.to_string(),
            world_name: None,
            include_command_line_api: None,
            run_immediately: None,
        })
        .map_err(|error| format!("无法注入手柄模拟: {error}"))?;
        tab.call_method(Page::AddScriptToEvaluateOnNewDocument {
            source: LOG_HOOK.to_string(),
            world_name: None,
            include_command_line_api: None,
            run_immediately: None,
        })
        .map_err(|error| format!("无法注入控制台日志钩子: {error}"))?;
        Ok(tab)
    }

    fn tab(&self) -> Result<Arc<Tab>, String> {
        self.tab
            .read()
            .map(|tab| Arc::clone(&tab))
            .map_err(|_| "浏览器标签页状态锁已损坏".to_string())
    }

    pub fn navigate(&self, url: &str) -> Result<(), String> {
        self.tab()?
            .navigate_to(url)
            .map_err(|error| format!("无法打开 {url}: {error}"))?;
        Ok(())
    }

    pub fn reload(&self) -> Result<(), String> {
        self.tab()?
            .reload(true, None)
            .map_err(|error| format!("无法刷新浏览器标签页: {error}"))?;
        thread::sleep(Duration::from_millis(250));
        self.browser
            .get_version()
            .map_err(|error| format!("刷新后浏览器连接已关闭: {error}"))?;
        Ok(())
    }

    pub fn wait_ready(&self) -> Result<(), String> {
        let mut state = Value::Null;
        self.wait_until("WebGPU 与编辑界面就绪", Duration::from_secs(180), || {
            state = self.eval(&format!(
                "(() => {{ const status = {}; const shell = {}; return {{ url: location.href, status: status?.textContent ?? null, alert: document.querySelector('[role=\"alert\"]')?.textContent ?? null, shell: !!shell, shellHeight: shell?.getBoundingClientRect().height ?? 0 }}; }})()",
                testid_expr("webgpu-status"),
                testid_expr("app-shell")
            ))?;
            Ok(state["status"]
                .as_str()
                .is_some_and(|text| text.contains("WebGPU ready"))
                && state["shellHeight"].as_f64().unwrap_or(0.0) > 0.0)
        })
        .map_err(|error| format!("{error}: {state}"))
    }

    pub fn eval(&self, expression: &str) -> Result<Value, String> {
        let wrapped = format!("JSON.stringify(({expression}))");
        let object = self
            .tab()?
            .evaluate(&wrapped, true)
            .map_err(|error| format!("执行页面脚本失败: {error}"))?;
        match object.value {
            Some(Value::String(serialized)) => serde_json::from_str(&serialized)
                .map_err(|error| format!("页面返回值不是合法 JSON: {error}")),
            Some(value) => Ok(value),
            None => Ok(Value::Null),
        }
    }

    pub(crate) fn eval_bool(&self, expression: &str) -> Result<bool, String> {
        Ok(self.eval(expression)?.as_bool().unwrap_or(false))
    }

    fn eval_string(&self, expression: &str) -> Result<String, String> {
        Ok(self
            .eval(expression)?
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    pub fn wait_until(
        &self,
        description: &str,
        timeout: Duration,
        mut condition: impl FnMut() -> Result<bool, String>,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            if condition()? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!("等待超时: {description}"));
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    pub(crate) fn wait_text(
        &self,
        testid: &str,
        expected: &str,
        timeout: Duration,
    ) -> Result<(), String> {
        let mut current = String::new();
        let mut count = 0;
        self.wait_until(&format!("{testid} 包含 `{expected}`"), timeout, || {
            count = self.count(testid)?;
            current = self.text(testid)?;
            Ok(current.contains(expected))
        })
        .map_err(|error| format!("{error}; {testid} 数量={count}, 当前文本={current:?}"))
    }

    fn wait_text_ne(&self, testid: &str, previous: &str, timeout: Duration) -> Result<(), String> {
        self.wait_until(&format!("{testid} 文本发生变化"), timeout, || {
            Ok(self.text(testid)? != previous)
        })
    }

    fn wait_value(&self, testid: &str, expected: &str, timeout: Duration) -> Result<(), String> {
        self.wait_until(&format!("{testid} 值等于 `{expected}`"), timeout, || {
            Ok(self.input_value(testid)? == expected)
        })
    }

    pub(crate) fn wait_visible(&self, testid: &str, timeout: Duration) -> Result<(), String> {
        self.wait_until(&format!("{testid} 可见"), timeout, || {
            self.visible_testid(testid)
        })
    }

    fn wait_absent(&self, testid: &str, timeout: Duration) -> Result<(), String> {
        self.wait_until(&format!("{testid} 消失"), timeout, || {
            Ok(!self.visible_testid(testid)?)
        })
    }

    pub(crate) fn visible_testid(&self, testid: &str) -> Result<bool, String> {
        self.eval_bool(&format!(
            "(() => {{ const el = {}; if (!el) return false; const style = getComputedStyle(el); return style.display !== 'none' && style.visibility !== 'hidden' && el.getClientRects().length > 0; }})()",
            testid_expr(testid)
        ))
    }

    pub(crate) fn visible_selector(&self, selector: &str) -> Result<bool, String> {
        self.eval_bool(&format!(
            "(() => {{ const el = document.querySelector({}); if (!el) return false; const style = getComputedStyle(el); return style.display !== 'none' && style.visibility !== 'hidden' && el.getClientRects().length > 0; }})()",
            json_string(selector)
        ))
    }

    fn count(&self, testid: &str) -> Result<usize, String> {
        Ok(self
            .eval(&format!(
                "document.querySelectorAll('[data-testid=\"' + {} + '\"]').length",
                json_string(testid)
            ))?
            .as_u64()
            .unwrap_or(0) as usize)
    }

    fn count_selector(&self, selector: &str) -> Result<usize, String> {
        Ok(self
            .eval(&format!(
                "document.querySelectorAll({}).length",
                json_string(selector)
            ))?
            .as_u64()
            .unwrap_or(0) as usize)
    }

    pub(crate) fn text(&self, testid: &str) -> Result<String, String> {
        self.eval_string(&format!(
            "(() => {{ const el = {}; return el ? el.textContent : ''; }})()",
            testid_expr(testid)
        ))
    }

    fn input_value(&self, testid: &str) -> Result<String, String> {
        self.eval_string(&format!(
            "(() => {{ const el = {}; return el ? String(el.value ?? '') : ''; }})()",
            testid_expr(testid)
        ))
    }

    fn attr(&self, testid: &str, name: &str) -> Result<String, String> {
        self.eval_string(&format!(
            "(() => {{ const el = {}; return el ? String(el.getAttribute({}) ?? '') : ''; }})()",
            testid_expr(testid),
            json_string(name)
        ))
    }

    pub(crate) fn click(&self, testid: &str) -> Result<(), String> {
        let clicked = self.eval_bool(&format!(
            "(() => {{ const el = {}; if (!el) return false; el.click(); return true; }})()",
            testid_expr(testid)
        ))?;
        ensure!(clicked, "找不到可点击元素: {testid}");
        Ok(())
    }

    pub(crate) fn click_selector(&self, selector: &str) -> Result<(), String> {
        let clicked = self.eval_bool(&format!(
            "(() => {{ const el = document.querySelector({}); if (!el) return false; el.click(); return true; }})()",
            json_string(selector)
        ))?;
        ensure!(clicked, "找不到可点击元素: {selector}");
        Ok(())
    }

    pub(crate) fn click_by_label(&self, label: &str) -> Result<(), String> {
        let clicked = self.eval_bool(&format!(
            "(() => {{ const el = [...document.querySelectorAll('button')].find((button) => (button.getAttribute('aria-label') || button.textContent || '').trim() === {}); if (!el) return false; el.click(); return true; }})()",
            json_string(label)
        ))?;
        ensure!(clicked, "找不到按钮: {label}");
        Ok(())
    }

    fn focus(&self, testid: &str) -> Result<(), String> {
        let focused = self.eval_bool(&format!(
            "(() => {{ const el = {}; if (!el) return false; el.focus(); return document.activeElement === el; }})()",
            testid_expr(testid)
        ))?;
        ensure!(focused, "无法聚焦元素: {testid}");
        Ok(())
    }

    fn focus_selector(&self, selector: &str) -> Result<(), String> {
        let focused = self.eval_bool(&format!(
            "(() => {{ const el = document.querySelector({}); if (!el) return false; el.focus(); return document.activeElement === el; }})()",
            json_string(selector)
        ))?;
        ensure!(focused, "无法聚焦元素: {selector}");
        Ok(())
    }

    fn fill(&self, testid: &str, value: &str) -> Result<(), String> {
        self.focus(testid)?;
        let filled = self.eval_bool(&format!(
            "(() => {{ const el = {}; if (!el) return false; const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set; setter.call(el, {}); el.dispatchEvent(new Event('input', {{ bubbles: true }})); el.dispatchEvent(new Event('change', {{ bubbles: true }})); return true; }})()",
            testid_expr(testid),
            json_string(value)
        ))?;
        ensure!(filled, "无法填写输入框: {testid}");
        thread::sleep(Duration::from_millis(50));
        Ok(())
    }

    fn press(&self, key: &str) -> Result<(), String> {
        self.tab()?
            .press_key(key)
            .map_err(|error| format!("无法发送按键 {key}: {error}"))?;
        self.flush_frame()
    }

    fn press_meta(&self, key: &str) -> Result<(), String> {
        self.press_modifier("Meta", "MetaLeft", 91, 4, true)?;
        self.tab()?
            .press_key_with_modifiers(key, Some(&[ModifierKey::Meta]))
            .map_err(|error| format!("无法发送 Command+{key}: {error}"))?;
        self.flush_frame()?;
        self.press_modifier("Meta", "MetaLeft", 91, 4, false)?;
        self.flush_frame()
    }

    fn press_shift(&self, key: &str) -> Result<(), String> {
        // Chrome's CDP synthesizes a Shift keyup before the modified key arrives,
        // so send DOM keyboard events in separate frames to exercise the same
        // winit path while keeping the modifier held.
        let dispatch = |event: &str, key: &str, code: &str, shift: bool| {
            self.eval_bool(&format!(
                "(() => {{ const el = document.querySelector('#bevy-canvas'); if (!el) return false; el.dispatchEvent(new KeyboardEvent({}, {{ key: {}, code: {}, shiftKey: {}, bubbles: true, cancelable: true }})); return true; }})()",
                json_string(event),
                json_string(key),
                json_string(code),
                shift
            ))
        };
        ensure!(
            dispatch("keydown", "Shift", "ShiftLeft", true)?,
            "无法发送 Shift 按下"
        );
        self.flush_frame()?;
        ensure!(
            dispatch("keydown", key, key, true)?,
            "无法发送 Shift+键按下"
        );
        self.flush_frame()?;
        ensure!(dispatch("keyup", key, key, true)?, "无法发送 Shift+键抬起");
        ensure!(
            dispatch("keyup", "Shift", "ShiftLeft", false)?,
            "无法发送 Shift 抬起"
        );
        self.flush_frame()
    }

    fn canvas_rect(&self) -> Result<Rect, String> {
        let value = self.eval(
            "(() => { const el = document.querySelector('#bevy-canvas'); if (!el) return null; const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; })()",
        )?;
        ensure!(!value.is_null(), "Bevy 画布不存在");
        Ok(Rect {
            x: value["x"].as_f64().unwrap_or(0.0),
            y: value["y"].as_f64().unwrap_or(0.0),
            width: value["width"].as_f64().unwrap_or(0.0),
            height: value["height"].as_f64().unwrap_or(0.0),
        })
    }

    fn design_to_page(&self, x: f64, y: f64) -> Result<(f64, f64), String> {
        let rect = self.canvas_rect()?;
        Ok(self.design_to_page_with(rect, x, y))
    }

    fn design_to_page_with(&self, rect: Rect, x: f64, y: f64) -> (f64, f64) {
        let scale = (rect.width / 1600.0).min(rect.height / 900.0);
        let draw_width = 1600.0 * scale;
        let draw_height = 900.0 * scale;
        let offset_x = rect.x + (rect.width - draw_width) / 2.0;
        let offset_y = rect.y + (rect.height - draw_height) / 2.0;
        (offset_x + x * scale, offset_y + y * scale)
    }

    fn mouse_event(
        &self,
        event_type: Input::DispatchMouseEventTypeOption,
        x: f64,
        y: f64,
        modifiers: Option<u32>,
        button: Option<Input::MouseButton>,
        buttons: Option<u32>,
    ) -> Result<(), String> {
        if std::env::var_os("MOX_E2E_DEBUG").is_some() {
            eprintln!(
                "mouse {event_type:?} x={x:.1} y={y:.1} modifiers={modifiers:?} button={button:?} buttons={buttons:?}"
            );
        }
        self.tab()?
            .call_method(Input::DispatchMouseEvent {
                Type: event_type.clone(),
                x,
                y,
                modifiers,
                timestamp: None,
                button,
                buttons,
                click_count: Some(1),
                force: None,
                tangential_pressure: None,
                tilt_x: None,
                tilt_y: None,
                twist: None,
                delta_x: None,
                delta_y: None,
                pointer_Type: None,
            })
            .map_err(|error| format!("鼠标事件失败: {error}"))?;
        if std::env::var_os("MOX_E2E_DEBUG").is_some() {
            eprintln!("mouse {event_type:?} ack");
        }
        Ok(())
    }

    fn click_at(&self, x: f64, y: f64) -> Result<(), String> {
        self.mouse_event(
            Input::DispatchMouseEventTypeOption::MouseMoved,
            x,
            y,
            None,
            None,
            None,
        )?;
        self.flush_frame()?;
        self.mouse_event(
            Input::DispatchMouseEventTypeOption::MousePressed,
            x,
            y,
            None,
            Some(Input::MouseButton::Left),
            Some(1),
        )?;
        self.flush_frame()?;
        self.mouse_event(
            Input::DispatchMouseEventTypeOption::MouseReleased,
            x,
            y,
            None,
            Some(Input::MouseButton::Left),
            Some(0),
        )?;
        self.flush_frame()
    }

    fn flush_frame(&self) -> Result<(), String> {
        if std::env::var_os("MOX_E2E_DEBUG").is_some() {
            eprintln!("flush frame start");
        }
        self.tab()?
            .evaluate(
                "new Promise((resolve) => { let done = false; const finish = () => { if (!done) { done = true; resolve(true); } }; requestAnimationFrame(() => requestAnimationFrame(finish)); setTimeout(finish, 100); })",
                true,
            )
            .map_err(|error| format!("等待浏览器动画帧失败: {error}"))?;
        if std::env::var_os("MOX_E2E_DEBUG").is_some() {
            eprintln!("flush frame done");
        }
        thread::sleep(Duration::from_millis(4));
        Ok(())
    }

    fn flush_frames(&self, count: usize) -> Result<(), String> {
        for _ in 0..count {
            self.flush_frame()?;
            thread::sleep(Duration::from_millis(24));
        }
        Ok(())
    }

    fn drag(
        &self,
        start: (f64, f64),
        end: (f64, f64),
        steps: usize,
        modifiers: Option<u32>,
    ) -> Result<(), String> {
        self.mouse_event(
            Input::DispatchMouseEventTypeOption::MouseMoved,
            start.0,
            start.1,
            modifiers,
            None,
            None,
        )?;
        self.flush_frame()?;
        self.mouse_event(
            Input::DispatchMouseEventTypeOption::MousePressed,
            start.0,
            start.1,
            modifiers,
            Some(Input::MouseButton::Left),
            Some(1),
        )?;
        self.flush_frame()?;
        for step in 1..=steps {
            let ratio = step as f64 / steps as f64;
            self.mouse_event(
                Input::DispatchMouseEventTypeOption::MouseMoved,
                start.0 + (end.0 - start.0) * ratio,
                start.1 + (end.1 - start.1) * ratio,
                modifiers,
                None,
                Some(1),
            )?;
            thread::sleep(Duration::from_millis(20));
        }
        self.flush_frame()?;
        self.mouse_event(
            Input::DispatchMouseEventTypeOption::MouseReleased,
            end.0,
            end.1,
            modifiers,
            Some(Input::MouseButton::Left),
            Some(0),
        )?;
        self.flush_frame()
    }

    fn set_shift_held(&self, held: bool, alt_held: bool) -> Result<(), String> {
        let modifiers = 8 | if alt_held { 1 } else { 0 };
        self.press_modifier("Shift", "ShiftLeft", 16, modifiers, held)
    }

    fn set_alt_held(&self, held: bool) -> Result<(), String> {
        self.press_modifier("Alt", "AltLeft", 18, 1, held)
    }

    fn axis_gizmo_x_position(&self) -> Result<(f64, f64), String> {
        let value = self.eval(
            "(() => { const el = document.querySelector('.axis-gizmo-label.is-x'); if (!el) return null; const r = el.getBoundingClientRect(); return { x: r.x, y: r.y }; })()",
        )?;
        ensure!(!value.is_null(), "3D 坐标方位标识没有布局区域");
        Ok((
            value["x"].as_f64().unwrap_or(0.0),
            value["y"].as_f64().unwrap_or(0.0),
        ))
    }

    fn press_modifier(
        &self,
        key: &str,
        code: &str,
        virtual_key: u32,
        modifiers: u32,
        down: bool,
    ) -> Result<(), String> {
        self.tab()?
            .call_method(Input::DispatchKeyEvent {
                Type: if down {
                    Input::DispatchKeyEventTypeOption::RawKeyDown
                } else {
                    Input::DispatchKeyEventTypeOption::KeyUp
                },
                modifiers: down.then_some(modifiers),
                timestamp: None,
                text: None,
                unmodified_text: None,
                key_identifier: None,
                code: Some(code.to_string()),
                key: Some(key.to_string()),
                windows_virtual_key_code: Some(virtual_key),
                native_virtual_key_code: Some(virtual_key),
                auto_repeat: None,
                is_keypad: None,
                is_system_key: None,
                location: None,
                commands: None,
            })
            .map_err(|error| format!("修饰键事件失败: {error}"))?;
        self.flush_frame()
    }

    fn selection_count(&self) -> Result<usize, String> {
        let text = self.text("selection-id")?;
        if text == "未选择" {
            return Ok(0);
        }
        if let Some(number) = text.strip_suffix("个节点") {
            return Ok(number.trim().parse().unwrap_or(0));
        }
        Ok(1)
    }

    fn set_gamepad_axis(&self, index: usize, value: f64) -> Result<(), String> {
        self.eval(&format!(
            "(() => {{ window.__moxPadMock.axes[{index}] = {value}; return true; }})()"
        ))?;
        self.flush_frames(3)?;
        Ok(())
    }

    fn set_gamepad_button(&self, index: usize, pressed: bool) -> Result<(), String> {
        self.eval(&format!(
            "(() => {{ window.__moxPadMock.buttons[{index}].pressed = {pressed}; return true; }})()"
        ))?;
        thread::sleep(Duration::from_millis(180));
        self.flush_frame()
    }

    fn press_gamepad_button(&self, index: usize) -> Result<(), String> {
        self.set_gamepad_button(index, true)?;
        thread::sleep(Duration::from_millis(160));
        self.set_gamepad_button(index, false)?;
        thread::sleep(Duration::from_millis(160));
        Ok(())
    }

    fn open_gamepad_menu(&self) -> Result<(), String> {
        self.press_gamepad_button(9)?;
        self.wait_visible("gamepad-menu", Duration::from_secs(5))
    }

    fn gamepad_menu_down(&self, count: usize) -> Result<(), String> {
        for _ in 0..count {
            self.press_gamepad_button(13)?;
        }
        Ok(())
    }

    fn gamepad_menu_confirm(&self) -> Result<(), String> {
        self.press_gamepad_button(0)
    }

    fn dispatch_wheel(&self, delta_y: f64, times: usize) -> Result<(), String> {
        let rect = self.canvas_rect()?;
        self.mouse_event(
            Input::DispatchMouseEventTypeOption::MouseMoved,
            rect.x + rect.width / 2.0,
            rect.y + rect.height / 2.0,
            None,
            None,
            None,
        )?;
        self.flush_frame()?;
        let prevented = self.eval_bool(&format!(
            "(() => {{ const canvas = document.querySelector('#bevy-canvas'); let prevented = true; for (let i = 0; i < {times}; i += 1) {{ const event = new WheelEvent('wheel', {{ deltaY: {delta_y}, deltaMode: WheelEvent.DOM_DELTA_PIXEL, ctrlKey: true, bubbles: true, cancelable: true }}); canvas.dispatchEvent(event); prevented = prevented && event.defaultPrevented; }} return prevented; }})()"
        ))?;
        ensure!(prevented, "画布滚轮事件未阻止浏览器默认缩放/滚动");
        self.flush_frame()
    }

    fn dispatch_safari_pinch(&self, scale: f64, times: usize) -> Result<(), String> {
        let rect = self.canvas_rect()?;
        self.mouse_event(
            Input::DispatchMouseEventTypeOption::MouseMoved,
            rect.x + rect.width / 2.0,
            rect.y + rect.height / 2.0,
            None,
            None,
            None,
        )?;
        let prevented = self.eval_bool(&format!(
            "(() => {{ const canvas = document.querySelector('#bevy-canvas'); const start = new Event('gesturestart', {{ bubbles: true, cancelable: true }}); Object.defineProperty(start, 'scale', {{ value: 1 }}); canvas.dispatchEvent(start); let prevented = start.defaultPrevented; for (let i = 1; i <= {times}; i += 1) {{ const event = new Event('gesturechange', {{ bubbles: true, cancelable: true }}); Object.defineProperty(event, 'scale', {{ value: Math.pow({scale}, i) }}); canvas.dispatchEvent(event); prevented = prevented && event.defaultPrevented; }} const end = new Event('gestureend', {{ bubbles: true, cancelable: true }}); canvas.dispatchEvent(end); prevented = prevented && end.defaultPrevented; return prevented; }})()"
        ))?;
        ensure!(prevented, "Safari 捏合手势未阻止浏览器默认页面缩放");
        Ok(())
    }

    fn set_viewport(&self, width: u32, height: u32) -> Result<(), String> {
        self.tab()?
            .call_method(Emulation::SetDeviceMetricsOverride {
                width,
                height,
                device_scale_factor: 1.0,
                mobile: false,
                scale: None,
                screen_width: None,
                screen_height: None,
                position_x: None,
                position_y: None,
                dont_set_visible_size: None,
                screen_orientation: None,
                viewport: None,
                display_feature: None,
                device_posture: None,
            })
            .map_err(|error| format!("无法调整浏览器视口: {error}"))?;
        self.tab()?
            .activate()
            .map_err(|error| format!("无法聚焦浏览器标签页: {error}"))?;
        let _ = self.eval_bool(
            "(() => { window.focus(); return document.visibilityState === 'visible'; })()",
        )?;
        Ok(())
    }

    fn upload_invalid_import(&self) -> Result<(), String> {
        let changed = self.eval_bool(&format!(
            "(() => {{ const input = {}; if (!input) return false; const transfer = new DataTransfer(); transfer.items.add(new File([{content}], 'invalid.moxhmi.json', {{ type: 'application/json' }})); input.files = transfer.files; input.dispatchEvent(new Event('change', {{ bubbles: true }})); return true; }})()",
            testid_expr("import-input"),
            content = json_string("{not json")
        ))?;
        ensure!(changed, "无法向导入控件写入文件");
        Ok(())
    }

    fn no_overflow(&self) -> Result<bool, String> {
        self.eval_bool("document.documentElement.scrollWidth <= window.innerWidth")
    }

    pub(crate) fn screenshot(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let bytes = self
            .tab()?
            .capture_screenshot(Page::CaptureScreenshotFormatOption::Png, None, None, true)
            .map_err(|error| format!("截图失败: {error}"))?;
        fs::write(path, bytes)
            .map_err(|error| format!("无法写入截图 {}: {error}", path.display()))?;
        println!("  screenshot: {}", path.display());
        Ok(())
    }

    fn logs(&self) -> Result<Vec<String>, String> {
        let value = self.eval("(() => { try { return JSON.parse(sessionStorage.getItem('__moxE2eLogs') || '[]'); } catch (_) { return []; } })()")?;
        Ok(value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default())
    }

    pub(crate) fn console_errors(&self) -> Result<Vec<String>, String> {
        Ok(self
            .logs()?
            .into_iter()
            .filter(|line| line.starts_with("[pageerror]") || line.starts_with("[error]"))
            .collect())
    }

    fn flow(
        &mut self,
        number: usize,
        title: &str,
        action: fn(&Self) -> Result<(), String>,
    ) -> Result<(), String> {
        self.current_flow = number;
        if self.only_flow.is_some_and(|only| only != number)
            || self.from_flow.is_some_and(|from| number < from)
        {
            return Ok(());
        }
        println!("[{number}/{TOTAL_FLOWS}] {title}");
        action(self)
    }

    pub fn run_flows(&mut self, url: &str) -> Result<(), String> {
        self.navigate(url)?;
        if self.only_flow.is_some() {
            self.eval("localStorage.clear()")?;
            self.reload()?;
            self.wait_ready()?;
        }
        let result = (|| {
            self.flow(1, "WebGPU and startup", Self::flow_startup)?;
            self.flow(2, "Default project and screens", Self::flow_default_project)?;
            self.flow(3, "Add, autosave, reload", Self::flow_add_and_reload)?;
            self.flow(4, "Create a 3D screen", Self::flow_create_3d_screen)?;
            self.flow(5, "Operation guide and F1", Self::flow_operation_guide)?;
            self.flow(6, "2D selection and center snap", Self::flow_select_2d)?;
            self.flow(7, "2D keyboard nudge", Self::flow_keyboard_nudge_2d)?;
            self.flow(
                8,
                "2D DualSense move and haptic path",
                Self::flow_gamepad_2d,
            )?;
            self.flow(9, "Edit and undo", Self::flow_edit_and_undo)?;
            self.flow(10, "3D picking", Self::flow_select_3d)?;
            self.flow(11, "3D keyboard move", Self::flow_keyboard_move_3d)?;
            self.flow(12, "3D DualSense move", Self::flow_gamepad_3d)?;
            self.flow(13, "3D trackpad device move", Self::flow_trackpad_move_3d)?;
            self.flow(14, "Tag simulation", Self::flow_tag_simulation)?;
            self.flow(
                15,
                "Runtime and button write",
                Self::flow_runtime_write_isolation,
            )?;
            self.flow(16, "Invalid import", Self::flow_invalid_import)?;
            self.flow(
                17,
                "Accessibility and reflow",
                Self::flow_accessibility_and_reflow,
            )?;
            self.flow(
                18,
                "2D marquee, clipboard and group undo",
                Self::flow_selection_and_clipboard_2d,
            )?;
            self.flow(
                19,
                "3D marquee and group drag",
                Self::flow_selection_and_group_drag_3d,
            )?;
            self.flow(
                20,
                "Text field shortcut isolation",
                Self::flow_shortcut_isolation_in_text_fields,
            )?;
            self.flow(
                21,
                "Mouse, touchpad, keyboard and DualSense parity",
                Self::flow_input_capability_audit,
            )
        })();
        match result {
            Ok(()) => {
                ensure!(self.no_overflow()?, "页面出现横向溢出");
                let errors = self.console_errors()?;
                ensure!(
                    errors.is_empty(),
                    "浏览器控制台错误:\n{}",
                    errors.join("\n")
                );
                if let Some(flow) = self.only_flow {
                    println!("PASS: flow {flow}/{TOTAL_FLOWS}");
                } else {
                    println!("PASS: {TOTAL_FLOWS}/{TOTAL_FLOWS} browser flows");
                }
                Ok(())
            }
            Err(error) => {
                if let Ok(text) = self.text("selection-id") {
                    eprintln!("selection before screenshot: {text:?}");
                }
                let _ = self.screenshot("/tmp/mox-hmi-failure.png");
                if let Ok(text) = self.text("selection-id") {
                    eprintln!("selection after screenshot: {text:?}");
                }
                eprintln!("FAIL: flow {}: {error}", self.current_flow);
                if let Ok(logs) = self.logs() {
                    eprintln!("Last 100 browser lines:");
                    for line in logs.iter().rev().take(100).rev() {
                        eprintln!("{line}");
                    }
                }
                Err(error)
            }
        }
    }
}

impl Harness {
    fn flow_startup(&self) -> Result<(), String> {
        let mut boot_state = Value::Null;
        self.wait_until(
            "启动 loading spinner 可见",
            Duration::from_secs(30),
            || {
                boot_state = self.eval(
                    r#"(() => {
                  const shell = document.querySelector('#boot-shell');
                  const spinner = document.querySelector('#boot-shell .boot-spinner');
                  const style = spinner ? getComputedStyle(spinner) : null;
                  return {
                    shell: !!shell,
                    spinner: !!spinner,
                    display: style?.display ?? '',
                    visibility: style?.visibility ?? '',
                    animationName: style?.animationName ?? '',
                    rects: spinner?.getClientRects().length ?? 0,
                    mainLength: document.querySelector('#main')?.children.length ?? -1,
                  };
                })()"#,
                )?;
                Ok(boot_state["shell"].as_bool() == Some(true)
                    && boot_state["spinner"].as_bool() == Some(true)
                    && boot_state["visibility"].as_str() != Some("hidden")
                    && boot_state["animationName"]
                        .as_str()
                        .is_some_and(|name| name != "none"))
            },
        )
        .map_err(|error| format!("{error}: {boot_state}"))?;
        thread::sleep(Duration::from_millis(900));
        self.eval("localStorage.clear()")?;
        self.reload()?;
        self.wait_ready()?;
        ensure!(self.eval_bool("!!navigator.gpu")?, "navigator.gpu 不可用");
        let logs = self.logs()?;
        ensure!(
            logs.iter().any(|line| line.contains("BrowserWebGpu")),
            "缺少 BrowserWebGpu 初始化日志"
        );
        ensure!(
            logs.iter()
                .any(|line| line.contains("MOX HMI runtime ready")),
            "缺少 MOX HMI runtime ready 日志"
        );
        ensure!(
            self.count_selector("#bevy-canvas")? == 1,
            "Bevy 画布数量不是 1"
        );
        Ok(())
    }

    fn flow_default_project(&self) -> Result<(), String> {
        ensure!(
            self.text("project-name")?.contains("BAS"),
            "默认工业项目名称缺失"
        );
        ensure!(
            self.count("screen-tab-screen-bas-2d")? == 1,
            "缺少默认 2D 画面"
        );
        ensure!(
            self.count("screen-tab-screen-bas-3d")? == 1,
            "缺少默认 3D 画面"
        );
        self.wait_text("scene-mode", "2D", Duration::from_secs(5))?;

        self.click("screen-tab-screen-bas-3d")?;
        self.wait_visible("screen-loading", Duration::from_secs(1))?;
        self.wait_text("scene-mode", "3D", Duration::from_secs(5))?;
        self.wait_absent("screen-loading", Duration::from_secs(2))?;

        self.click("screen-tab-screen-bas-2d")?;
        self.wait_visible("screen-loading", Duration::from_secs(1))?;
        self.wait_text("scene-mode", "2D", Duration::from_secs(5))?;
        self.wait_absent("screen-loading", Duration::from_secs(2))?;
        Ok(())
    }

    fn flow_create_3d_screen(&self) -> Result<(), String> {
        self.click("add-screen")?;
        self.wait_visible("add-screen-2d", Duration::from_secs(5))?;
        self.wait_visible("add-screen-3d", Duration::from_secs(5))?;
        self.click("add-screen-3d")?;
        self.wait_text("scene-mode", "3D", Duration::from_secs(5))?;
        self.wait_visible("screen-tab-screen-new-1", Duration::from_secs(5))?;
        ensure!(
            self.attr("screen-tab-screen-new-1", "aria-current")? == "page",
            "新建 3D 画面未成为当前标签页"
        );
        thread::sleep(Duration::from_millis(600));
        self.click("screen-tab-screen-bas-2d")?;
        self.wait_text("scene-mode", "2D", Duration::from_secs(5))?;
        Ok(())
    }

    fn flow_operation_guide(&self) -> Result<(), String> {
        self.click("guide-button")?;
        self.wait_visible("guide-dialog", Duration::from_secs(5))?;
        self.wait_text(
            "guide-content",
            "新建 2D 或 3D 画面",
            Duration::from_secs(5),
        )?;
        self.click("guide-tab-2d")?;
        self.wait_text("guide-content", "2D：PS5 DualSense", Duration::from_secs(5))?;
        self.wait_text("guide-content", "触控板双指滚动", Duration::from_secs(5))?;
        self.wait_text(
            "guide-content",
            "DualSense 触摸板按下",
            Duration::from_secs(5),
        )?;
        self.wait_text(
            "guide-content",
            "参数模式：方向键 ← / →",
            Duration::from_secs(5),
        )?;
        self.click("guide-tab-3d")?;
        self.wait_text("guide-content", "3D：PS5 DualSense", Duration::from_secs(5))?;
        self.wait_text(
            "guide-content",
            "3D：键盘（先点击画布）",
            Duration::from_secs(5),
        )?;
        self.wait_text("guide-content", "拖拽设备", Duration::from_secs(5))?;
        self.click("guide-tab-gamepad")?;
        self.wait_text("guide-content", "组件库：手柄添加", Duration::from_secs(5))?;
        self.wait_text(
            "guide-content",
            "数据字典：浏览与选中 Tag",
            Duration::from_secs(5),
        )?;
        ensure!(
            self.eval_bool(
                "(() => { const el = document.querySelector('[data-testid=\"guide-content\"]'); return el ? el.scrollHeight > el.clientHeight : false; })()"
            )?,
            "手柄操作指引未形成独立滚动区域"
        );
        ensure!(
            self.eval_bool(
                "(() => { const el = document.querySelector('[data-testid=\"guide-content\"]'); el.scrollTop = el.scrollHeight; return el.scrollTop + el.clientHeight >= el.scrollHeight - 1; })()"
            )?,
            "手柄操作指引无法滚动到底部"
        );
        self.click("guide-close")?;
        self.wait_absent("guide-dialog", Duration::from_secs(5))?;

        self.focus_selector("#bevy-canvas")?;
        ensure!(
            self.eval_string("document.activeElement?.id")? == "bevy-canvas",
            "画布未获得焦点"
        );
        self.press("F1")?;
        self.wait_visible("guide-dialog", Duration::from_secs(5))?;
        self.press("Escape")?;
        self.wait_absent("guide-dialog", Duration::from_secs(5))?;
        Ok(())
    }

    fn flow_select_2d(&self) -> Result<(), String> {
        thread::sleep(Duration::from_millis(1_500));
        let (x, y) = self.design_to_page(780.0, 340.0)?;
        self.click_at(x, y)?;
        thread::sleep(Duration::from_secs(3));
        self.wait_text("selection-id", "node.ahu", Duration::from_secs(15))?;
        self.wait_value("node-name", "AHU-01", Duration::from_secs(5))?;

        let (snap_x, snap_y) = self.design_to_page(805.0, 455.0)?;
        self.drag((x, y), (snap_x, snap_y), 12, None)?;
        thread::sleep(Duration::from_millis(200));
        self.wait_value("node-x", "670", Duration::from_secs(5))?;
        self.wait_value("node-y", "360", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.wait_value("node-x", "650", Duration::from_secs(5))?;
        self.wait_value("node-y", "250", Duration::from_secs(5))?;
        Ok(())
    }

    fn flow_keyboard_nudge_2d(&self) -> Result<(), String> {
        self.focus_selector("#bevy-canvas")?;
        self.press("ArrowRight")?;
        thread::sleep(Duration::from_millis(150));
        self.wait_value("node-x", "651", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.wait_value("node-x", "650", Duration::from_secs(5))?;

        self.focus_selector("#bevy-canvas")?;
        self.press_shift("ArrowRight")?;
        thread::sleep(Duration::from_millis(150));
        self.wait_value("node-x", "660", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.wait_value("node-x", "650", Duration::from_secs(5))?;
        Ok(())
    }

    fn flow_gamepad_2d(&self) -> Result<(), String> {
        self.wait_text("gamepad-status", "DualSense", Duration::from_secs(10))?;
        let (x, y) = self.design_to_page(780.0, 340.0)?;
        self.click_at(x, y)?;
        self.wait_text("selection-id", "node.ahu", Duration::from_secs(5))?;
        thread::sleep(Duration::from_millis(250));

        self.set_gamepad_axis(0, 0.8)?;
        self.flush_frames(4)?;
        self.set_gamepad_axis(0, 0.0)?;
        self.flush_frames(2)?;
        let moved = self.input_value("node-x")?.parse::<f64>().unwrap_or(650.0);
        ensure!(moved != 650.0, "DualSense 左摇杆没有移动 2D 节点");
        self.click("undo-button")?;
        self.wait_value("node-x", "650", Duration::from_secs(5))?;

        let (snap_x, snap_y) = self.design_to_page(805.0, 455.0)?;
        self.drag((x, y), (snap_x, snap_y), 12, None)?;
        thread::sleep(Duration::from_millis(250));
        let haptic = self.eval("window.__moxPadMock.lastHaptic")?;
        ensure!(
            haptic["type"].as_str() == Some("dual-rumble"),
            "对齐时没有触发 DualSense 振动: {haptic}"
        );
        self.click("undo-button")?;
        self.wait_value("node-x", "650", Duration::from_secs(5))?;

        self.press_gamepad_button(8)?;
        self.wait_visible("guide-dialog", Duration::from_secs(5))?;
        self.press_gamepad_button(1)?;
        self.wait_absent("guide-dialog", Duration::from_secs(5))?;
        self.wait_text("selection-id", "node.ahu", Duration::from_secs(5))?;

        self.press_gamepad_button(17)?;
        self.wait_visible("controller-tuning", Duration::from_secs(5))?;
        self.wait_text("controller-tuning", "X", Duration::from_secs(5))?;
        self.press_gamepad_button(15)?;
        self.wait_value("node-x", "651", Duration::from_secs(5))?;
        self.press_gamepad_button(13)?;
        self.wait_text("controller-tuning", "Y", Duration::from_secs(5))?;
        self.press_gamepad_button(14)?;
        self.wait_value("node-y", "249", Duration::from_secs(5))?;
        self.press_gamepad_button(1)?;
        self.wait_absent("controller-tuning", Duration::from_secs(5))?;
        self.wait_text("selection-id", "node.ahu", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.click("undo-button")?;
        self.wait_value("node-x", "650", Duration::from_secs(5))?;
        self.wait_value("node-y", "250", Duration::from_secs(5))?;
        Ok(())
    }

    fn flow_edit_and_undo(&self) -> Result<(), String> {
        self.wait_value("node-x", "650", Duration::from_secs(5))?;
        self.fill("node-x", "700")?;
        self.press("Enter")?;
        self.wait_value("node-x", "700", Duration::from_secs(5))?;
        self.fill("node-x", "820")?;
        self.press("Enter")?;
        self.wait_value("node-x", "820", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.wait_value("node-x", "700", Duration::from_secs(5))?;
        self.click("undo-button")?;
        self.wait_value("node-x", "650", Duration::from_secs(5))?;
        Ok(())
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("字符串必须可序列化")
}

fn testid_expr(testid: &str) -> String {
    format!(
        "document.querySelector('[data-testid=\"' + {} + '\"]')",
        json_string(testid)
    )
}

fn sanitize_test_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn chrome_path() -> Result<PathBuf, String> {
    let candidates = [
        std::env::var_os("CHROME_PATH").map(PathBuf::from),
        Some(PathBuf::from(DEFAULT_CHROME)),
        Some(PathBuf::from(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        )),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "未找到 Chrome for Testing。请设置 CHROME_PATH 或把浏览器放到默认测试路径。".to_string()
        })
}

pub fn run(args: Vec<String>) -> Result<(), String> {
    let url_from_env = args.iter().any(|arg| arg == "--url-from-env");
    let release = args.iter().any(|arg| arg == "--release");
    let port = option_value(&args, "--port")
        .and_then(|value| value.parse::<u16>().ok())
        .or_else(|| {
            std::env::var("MOX_HMI_PORT")
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
        })
        .unwrap_or(choose_port()?);

    let mut server = None;
    let url = if url_from_env {
        let mut url = std::env::var("MOX_HMI_URL")
            .map_err(|_| "使用 --url-from-env 时必须设置 MOX_HMI_URL".to_string())?;
        if !url.ends_with('/') {
            url.push('/');
        }
        url
    } else {
        println!(
            "Starting dx serve ({}) on http://127.0.0.1:{port}/",
            if release { "release" } else { "debug" }
        );
        let mut dev = DevServer::start(&crate::workspace_root(), port, release)?;
        dev.wait_ready()?;
        let url = dev.url.clone();
        server = Some(dev);
        url
    };

    if server.is_none() {
        wait_for_remote(&url)?;
    }

    let mut harness = Harness::launch()?;
    harness.only_flow = option_value(&args, "--flow").and_then(|value| value.parse().ok());
    harness.from_flow = option_value(&args, "--from-flow").and_then(|value| value.parse().ok());
    let result = harness.run_flows(&url);
    drop(server);
    result
}

fn wait_for_remote(url: &str) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        let output = Command::new("curl")
            .args(["-fsS", "--max-time", "2", url])
            .output();
        if output.is_ok_and(|output| output.status.success()) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(format!("远端页面未就绪: {url}"))
}

fn option_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().enumerate().find_map(|(index, arg)| {
        if arg == name {
            return args.get(index + 1).map(String::as_str);
        }
        arg.strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
    })
}
