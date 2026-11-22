use crate::hub::ControlHub;
use hmi_control_protocol::PROTOCOL_VERSION;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

pub async fn serve(hub: ControlHub) -> Result<(), String> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = BufWriter::new(tokio::io::stdout());

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|error| format!("读取 MCP stdin 失败: {error}"))?
    {
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                write_response(
                    &mut stdout,
                    json!({
                        "jsonrpc": "2.0",
                        "id": Value::Null,
                        "error": { "code": -32700, "message": format!("JSON 解析失败: {error}") }
                    }),
                )
                .await?;
                continue;
            }
        };

        if let Some(payload) = handle_jsonrpc(&hub, request).await {
            write_response(&mut stdout, payload).await?;
        }
    }

    Ok(())
}

/// Handle one JSON-RPC message. Notifications (messages without an `id`) return
/// `None`, which maps to an empty HTTP 202 response for the streamable HTTP
/// transport.
pub async fn handle_jsonrpc(hub: &ControlHub, request: Value) -> Option<Value> {
    let id = request.get("id").cloned()?;
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let response = match method {
        "initialize" => Ok(initialize_result(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(hub, &params).await,
        "resources/list" => Ok(json!({ "resources": [] })),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        "logging/setLevel" => Ok(json!({})),
        _ => Err((-32601, format!("未知 MCP 方法 `{method}`"))),
    };

    Some(match response {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    })
}

async fn write_response(
    stdout: &mut BufWriter<tokio::io::Stdout>,
    value: Value,
) -> Result<(), String> {
    let text =
        serde_json::to_string(&value).map_err(|error| format!("序列化 MCP 响应失败: {error}"))?;
    stdout
        .write_all(text.as_bytes())
        .await
        .map_err(|error| format!("写入 MCP stdout 失败: {error}"))?;
    stdout
        .write_all(b"\n")
        .await
        .map_err(|error| format!("写入 MCP stdout 失败: {error}"))?;
    stdout
        .flush()
        .await
        .map_err(|error| format!("刷新 MCP stdout 失败: {error}"))
}

fn initialize_result(params: &Value) -> Value {
    let protocol = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or("2024-11-05");
    json!({
        "protocolVersion": protocol,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "mox-hmi-control",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": format!(
            "控制已连接的 MOX HMI Studio 浏览器。每次工具调用都会在页面的 AI 操作隧道中可见。协议版本 {PROTOCOL_VERSION}。"
        ),
    })
}

async fn call_tool(hub: &ControlHub, params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| (-32602, "tools/call 缺少 name".to_string()))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match name {
        "mox_hmi_status" => {
            let status = hub.status().await;
            Ok(tool_result(status, false))
        }
        "mox_hmi_list_commands" => Ok(tool_result(command_catalog(), false)),
        "mox_hmi_inspect_project" => {
            let reply = hub
                .request("inspect_project", "读取 HMI 工程与画面", arguments)
                .await
                .map_err(|error| (-32603, error))?;
            Ok(reply_result(reply))
        }
        "mox_hmi_execute_command" => {
            let action = arguments
                .get("action")
                .and_then(Value::as_str)
                .ok_or_else(|| (-32602, "缺少 action".to_string()))?;
            let label = arguments
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("执行 {action}"));
            let command_params = arguments
                .get("params")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let reply = hub
                .request(action, &label, command_params)
                .await
                .map_err(|error| (-32603, error))?;
            Ok(reply_result(reply))
        }
        other => Err((-32602, format!("未知工具 `{other}`"))),
    }
}

fn reply_result(reply: crate::hub::BrowserReply) -> Value {
    let mut payload = json!({
        "ok": reply.ok,
        "result": reply.result,
        "state": reply.state,
        "duration_ms": reply.duration_ms,
    });
    if let Some(error) = reply.error {
        payload["error"] = Value::String(error);
    }
    tool_result(payload, !reply.ok)
}

fn tool_result(payload: Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": payload,
        "isError": is_error,
    })
}

fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "mox_hmi_status",
            "description": "查看浏览器是否已连接控制隧道，以及当前画面、选择、编辑/运行模式和修订号。",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }),
        json!({
            "name": "mox_hmi_list_commands",
            "description": "列出可以执行的 HMI 控制动作和 EditorCommand 类型。",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }),
        json!({
            "name": "mox_hmi_inspect_project",
            "description": "读取完整 HMI 工程 JSON，或指定 screen_id 只看一个画面。先用它获取节点、Tag 和绑定结构。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "screen_id": { "type": "string", "description": "可选，只返回该画面" }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "mox_hmi_execute_command",
            "description": "在已连接的 MOX HMI 浏览器中执行动作。动作走现有 EditorCommand/撤销/校验，每一步会显示在用户界面的 AI 操作隧道中。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": [
                            "select_screen", "select_node", "set_mode",
                            "execute_editor_command", "write_tag", "undo", "redo",
                            "save", "reset_camera_3d"
                        ]
                    },
                    "description": { "type": "string", "description": "展示给用户的这一步操作说明" },
                    "params": { "type": "object", "description": "动作参数" }
                },
                "required": ["action"],
                "additionalProperties": false
            }
        }),
    ]
}

fn command_catalog() -> Value {
    json!({
        "actions": [
            { "name": "select_screen", "params": { "screen_id": "string" } },
            { "name": "select_node", "params": { "node_id": "string|null" } },
            { "name": "set_mode", "params": { "mode": "edit|run" } },
            { "name": "inspect_project", "params": { "screen_id": "optional string" } },
            { "name": "execute_editor_command", "params": { "command": "EditorCommand JSON object" } },
            { "name": "write_tag", "params": { "tag_id": "string", "value": "number|boolean|string" } },
            { "name": "undo", "params": {} },
            { "name": "redo", "params": {} },
            { "name": "save", "params": {} },
            { "name": "reset_camera_3d", "params": {} }
        ],
        "editor_command_types": [
            "add_screen", "delete_screen", "rename_screen", "set_startup_screen",
            "add2d_node", "update2d_node", "add3d_node", "update3d_node",
            "delete_node", "duplicate_node", "set_bindings",
            "add_tag", "update_tag", "delete_tag"
        ],
        "editor_command_shape": {
            "type": "update2d_node",
            "screen_id": "screen.bas_2d",
            "node": { "id": "node.example", "name": "示例", "rect": { "x": 10, "y": 10, "width": 100, "height": 50 }, "rotation_deg": 0, "z": 1, "visible": true, "locked": false, "widget": { "type": "label", "text": "AI", "font_size": 24, "color": "#37e6d4", "align": "left" }, "bindings": [] }
        }
    })
}
