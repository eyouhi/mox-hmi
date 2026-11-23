use crate::{
    e2e::Harness,
    server::{DevServer, choose_port},
};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

const CONTROL_BIN: &str = "target/debug/mox-hmi-control";
const CONTROL_LOG: &str = "/tmp/mox-hmi-control-e2e.log";

struct RpcClient {
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<String>,
    next_id: u64,
}

impl RpcClient {
    fn start(root: &Path, port: u16, token: &str) -> Result<Self, String> {
        let status = Command::new("cargo")
            .current_dir(root)
            .args(["build", "-p", "mox-hmi-control"])
            .status()
            .map_err(|error| format!("无法构建 mox-hmi-control: {error}"))?;
        if !status.success() {
            return Err(format!("mox-hmi-control 构建失败: {status}"));
        }

        let log =
            File::create(CONTROL_LOG).map_err(|error| format!("无法创建控制隧道日志: {error}"))?;
        let stderr = log
            .try_clone()
            .map_err(|error| format!("无法复制控制隧道日志句柄: {error}"))?;
        let mut child = Command::new(root.join(CONTROL_BIN))
            .current_dir(root)
            .args(["--port", &port.to_string(), "--token", token])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|error| format!("无法启动 mox-hmi-control: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "mox-hmi-control 缺少 stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "mox-hmi-control 缺少 stdout".to_string())?;
        let (sender, responses) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Ok(Self {
                    child,
                    stdin,
                    responses,
                    next_id: 1,
                });
            }
            if child
                .try_wait()
                .map_err(|error| format!("检查控制隧道状态失败: {error}"))?
                .is_some()
            {
                return Err("mox-hmi-control 提前退出".to_string());
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err("等待控制隧道监听超时".to_string())
    }

    fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let mut payload = json!({"jsonrpc": "2.0", "id": id, "method": method});
        if let Some(params) = params {
            payload["params"] = params;
        }
        writeln!(self.stdin, "{payload}").map_err(|error| format!("写入 MCP 请求失败: {error}"))?;
        self.stdin
            .flush()
            .map_err(|error| format!("刷新 MCP 请求失败: {error}"))?;
        let line = self
            .responses
            .recv_timeout(Duration::from_secs(30))
            .map_err(|error| format!("等待 MCP 响应超时: {error}"))?;
        let response: Value = serde_json::from_str(&line)
            .map_err(|error| format!("MCP 响应不是 JSON: {error}\n{line}"))?;
        if let Some(error) = response.get("error") {
            return Err(format!("MCP 错误: {error}"));
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| format!("MCP 响应缺少 result: {response}"))
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        self.request(
            "tools/call",
            Some(json!({"name": name, "arguments": arguments})),
        )
    }
}

impl Drop for RpcClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn run(args: Vec<String>) -> Result<(), String> {
    let url_from_env = args.iter().any(|arg| arg == "--url-from-env");
    let release = args.iter().any(|arg| arg == "--release");
    let root = crate::workspace_root();
    let control_port = choose_port()?;
    let token = "mox-hmi-e2e-token";
    let mut server = None;
    let app_url = if url_from_env {
        let mut url = std::env::var("MOX_HMI_URL")
            .map_err(|_| "使用 --url-from-env 时必须设置 MOX_HMI_URL".to_string())?;
        if !url.ends_with('/') {
            url.push('/');
        }
        url
    } else {
        let mut port = choose_port()?;
        while port == control_port {
            port = choose_port()?;
        }
        let mut dev = DevServer::start(&root, port, release)?;
        dev.wait_ready()?;
        let url = dev.url.clone();
        server = Some(dev);
        url
    };

    let mut control = RpcClient::start(&root, control_port, token)?;
    let control_url = format!("ws://127.0.0.1:{control_port}/control");
    let separator = if app_url.contains('?') { '&' } else { '?' };
    let page_url = format!("{app_url}{separator}control={control_url}&control_token={token}");

    let harness = Harness::launch()?;
    harness.navigate(&page_url)?;
    let result = (|| {
        harness.wait_ready()?;
        harness.wait_text("ai-connection", "已连接", Duration::from_secs(30))?;
        if !harness.visible_selector(".ai-connection-frame.is-visible")? {
            return Err("AI 已连接，但浅蓝紫渐变边框未显示".to_string());
        }

        let initialized = control.request(
            "initialize",
            Some(json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "mox-hmi-e2e", "version": "0"}
            })),
        )?;
        if initialized["serverInfo"]["name"].as_str() != Some("mox-hmi-control") {
            return Err(format!("控制服务初始化失败: {initialized}"));
        }
        let tools = control.request("tools/list", None)?;
        let names: Vec<&str> = tools["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        if !names.contains(&"mox_hmi_execute_command") {
            return Err(format!("控制隧道缺少 mox_hmi_execute_command: {names:?}"));
        }

        let ai_body_visible = harness.visible_selector("#ai-tunnel-body")?;
        if !ai_body_visible {
            return Err("AI 操作隧道初始不可见".to_string());
        }
        harness.click_selector("button[aria-controls=\"ai-tunnel-body\"]")?;
        harness.wait_until("AI 操作隧道折叠", Duration::from_secs(5), || {
            Ok(!harness.visible_selector("#ai-tunnel-body")?)
        })?;
        harness.click_by_label("展开 AI 操作隧道")?;
        harness.wait_until("AI 操作隧道展开", Duration::from_secs(5), || {
            Ok(harness.visible_selector("#ai-tunnel-body")?)
        })?;

        let status = control.tool("mox_hmi_status", json!({}))?;
        if status["structuredContent"]["connected"].as_bool() != Some(true) {
            return Err(format!("控制隧道未连接: {status}"));
        }
        let inspected = control.tool(
            "mox_hmi_inspect_project",
            json!({"screen_id": "screen.bas_2d"}),
        )?;
        if inspected["isError"].as_bool() == Some(true) {
            return Err(format!("检查项目失败: {inspected}"));
        }

        let selected = control.tool(
            "mox_hmi_execute_command",
            json!({
                "action": "select_node",
                "description": "AI 选中送风机 SF-01",
                "params": {"node_id": "node.supply_fan"}
            }),
        )?;
        if selected["isError"].as_bool() == Some(true) {
            return Err(format!("AI 选择节点失败: {selected}"));
        }
        harness.wait_text("selection-id", "node.supply_fan", Duration::from_secs(10))?;
        harness.wait_until("AI 操作记录可见", Duration::from_secs(10), || {
            Ok(harness.eval_bool(
                "[...document.querySelectorAll('[data-testid=\"ai-activity\"]')].some((el) => el.textContent.includes('AI 选中送风机 SF-01'))",
            )?)
        })?;

        let renamed = control.tool(
            "mox_hmi_execute_command",
            json!({
                "action": "execute_editor_command",
                "description": "AI 重命名环控工艺图",
                "params": {
                    "command": {
                        "type": "rename_screen",
                        "screen_id": "screen.bas_2d",
                        "name": "环控工艺图 · AI"
                    }
                }
            }),
        )?;
        if renamed["isError"].as_bool() == Some(true) {
            return Err(format!("AI 重命名失败: {renamed}"));
        }
        harness.wait_text(
            "screen-tab-screen-bas-2d",
            "环控工艺图 · AI",
            Duration::from_secs(10),
        )?;
        harness.wait_text("selection-id", "node.supply_fan", Duration::from_secs(10))?;

        let undone = control.tool(
            "mox_hmi_execute_command",
            json!({
                "action": "undo",
                "description": "AI 撤销刚才的重命名",
                "params": {}
            }),
        )?;
        if undone["isError"].as_bool() == Some(true) {
            return Err(format!("AI 撤销失败: {undone}"));
        }
        harness.wait_text(
            "screen-tab-screen-bas-2d",
            "环控工艺图",
            Duration::from_secs(10),
        )?;
        harness.wait_until("AI 撤销记录可见", Duration::from_secs(10), || {
            Ok(harness.eval_bool(
                "[...document.querySelectorAll('[data-testid=\"ai-activity\"]')].some((el) => el.textContent.includes('AI 撤销刚才的重命名'))",
            )?)
        })?;
        harness.screenshot("/tmp/mox-hmi-ai-control.png")?;
        let errors = harness.console_errors()?;
        if !errors.is_empty() {
            return Err(format!(
                "控制隧道验收出现浏览器错误:\n{}",
                errors.join("\n")
            ));
        }
        println!("PASS: MCP control tunnel");
        println!("Screenshot: /tmp/mox-hmi-ai-control.png");
        Ok(())
    })();

    drop(server);
    result
}
