use crate::hub::ControlHub;
use hmi_control_protocol::{
    PROTOCOL_VERSION,
    tools::{
        self, CAPTURE_PREVIEW_TOOL, EXECUTE_COMMAND_TOOL, EXPORT_PROJECT_TOOL, GET_NODE_TOOL,
        INSPECT_PROJECT_TOOL, LIST_COMMANDS_TOOL, LIST_TAGS_TOOL, SEARCH_NODES_TOOL, STATUS_TOOL,
        VALIDATE_PROJECT_TOOL,
    },
};
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
        "tools/list" => Ok(json!({
            "tools": tools::tool_specs()
                .iter()
                .map(tools::ToolSpec::mcp_json)
                .collect::<Vec<_>>()
        })),
        "tools/call" => call_tool(hub, &params).await,
        "resources/list" => Ok(resource_list()),
        "resources/read" => resource_read(hub, &params).await,
        "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
        "prompts/list" => Ok(prompt_list()),
        "prompts/get" => prompt_get(&params),
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
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "listChanged": false },
            "prompts": { "listChanged": false }
        },
        "serverInfo": {
            "name": "mox-hmi-control",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": format!(
            "控制已连接的 MOX HMI Studio 浏览器。先用 mox_hmi_status、mox_hmi_inspect_project、mox_hmi_list_tags 和 mox_hmi_search_nodes 读取现状，用 mox_hmi_capture_preview 做视觉校验，再通过 mox_hmi_execute_command 修改；每次调用都会显示在页面的 AI 操作隧道中。协议版本 {PROTOCOL_VERSION}。"
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
        STATUS_TOOL => {
            let status = hub.status().await;
            Ok(tool_result(status, false))
        }
        LIST_COMMANDS_TOOL => Ok(tool_result(tools::command_catalog(), false)),
        INSPECT_PROJECT_TOOL => {
            request_tool(hub, "inspect_project", "读取 HMI 工程与画面", arguments).await
        }
        LIST_TAGS_TOOL => request_tool(hub, "list_tags", "读取 HMI Tag 列表", arguments).await,
        GET_NODE_TOOL => request_tool(hub, "get_node", "读取 HMI 节点", arguments).await,
        SEARCH_NODES_TOOL => request_tool(hub, "search_nodes", "搜索 HMI 节点", arguments).await,
        VALIDATE_PROJECT_TOOL => {
            request_tool(hub, "validate_project", "校验 HMI 工程", arguments).await
        }
        EXPORT_PROJECT_TOOL => {
            request_tool(hub, "export_project", "导出 HMI 工程", arguments).await
        }
        CAPTURE_PREVIEW_TOOL => capture_preview(hub, arguments).await,
        EXECUTE_COMMAND_TOOL => {
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
            request_tool(hub, action, &label, command_params).await
        }
        other => Err((-32602, format!("未知工具 `{other}`"))),
    }
}

async fn request_tool(
    hub: &ControlHub,
    action: &str,
    label: &str,
    arguments: Value,
) -> Result<Value, (i64, String)> {
    let reply = hub
        .request(action, label, arguments)
        .await
        .map_err(|error| (-32603, error))?;
    Ok(reply_result(reply))
}

async fn capture_preview(hub: &ControlHub, arguments: Value) -> Result<Value, (i64, String)> {
    let reply = hub
        .request("capture_preview", "截取当前 HMI 画面", arguments)
        .await
        .map_err(|error| (-32603, error))?;
    if !reply.ok {
        return Ok(reply_result(reply));
    }
    let Some(encoded) = reply.result.get("base64").and_then(Value::as_str) else {
        return Ok(reply_result(reply));
    };
    let mut result = reply.result.clone();
    if let Some(object) = result.as_object_mut() {
        object.remove("base64");
    }
    let payload = json!({
        "ok": true,
        "result": result,
        "state": reply.state,
        "duration_ms": reply.duration_ms,
    });
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
    Ok(json!({
        "content": [
            { "type": "text", "text": text },
            { "type": "image", "data": encoded, "mimeType": "image/png" }
        ],
        "structuredContent": payload,
        "isError": false,
    }))
}

fn resource_list() -> Value {
    json!({
        "resources": [
            {
                "uri": "mox-hmi://status",
                "name": "MOX HMI 状态",
                "description": "浏览器连接、当前画面、选择、模式和修订号。",
                "mimeType": "application/json"
            },
            {
                "uri": "mox-hmi://project",
                "name": "MOX HMI 工程",
                "description": "当前工程 JSON，包含画面、节点、Tag 和绑定。",
                "mimeType": "application/json"
            },
            {
                "uri": "mox-hmi://commands",
                "name": "MOX HMI 动作目录",
                "description": "可执行动作、查询工具和 EditorCommand 类型。",
                "mimeType": "application/json"
            }
        ]
    })
}

async fn resource_read(hub: &ControlHub, params: &Value) -> Result<Value, (i64, String)> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| (-32602, "resources/read 缺少 uri".to_string()))?;
    match uri {
        "mox-hmi://status" => Ok(resource_contents(uri, hub.status().await)),
        "mox-hmi://commands" => Ok(resource_contents(uri, tools::command_catalog())),
        "mox-hmi://project" => {
            let reply = hub
                .request("inspect_project", "读取 MCP 工程资源", json!({}))
                .await
                .map_err(|error| (-32603, error))?;
            if !reply.ok {
                return Err((
                    -32603,
                    reply.error.unwrap_or_else(|| "读取工程失败".to_string()),
                ));
            }
            let project = reply.result.get("project").cloned().unwrap_or(reply.result);
            Ok(resource_contents(uri, project))
        }
        _ => Err((-32602, format!("未知资源 `{uri}`"))),
    }
}

fn resource_contents(uri: &str, value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    json!({
        "contents": [
            { "uri": uri, "mimeType": "application/json", "text": text }
        ]
    })
}

fn prompt_list() -> Value {
    json!({
        "prompts": [
            {
                "name": "mox_hmi_review",
                "description": "审查当前 HMI 画面的结构、Tag 绑定和视觉布局。",
                "arguments": [
                    {
                        "name": "screen_id",
                        "description": "可选，指定画面 id；省略时审查当前画面。",
                        "required": false
                    }
                ]
            }
        ]
    })
}

fn prompt_get(params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| (-32602, "prompts/get 缺少 name".to_string()))?;
    match name {
        "mox_hmi_review" => {
            let screen_id = params
                .get("arguments")
                .and_then(|arguments| arguments.get("screen_id"))
                .and_then(Value::as_str)
                .unwrap_or("当前画面");
            Ok(json!({
                "description": "审查 HMI 画面",
                "messages": [
                    {
                        "role": "user",
                        "content": {
                            "type": "text",
                            "text": format!(
                                "请审查 {screen_id}。先调用 mox_hmi_inspect_project 获取结构和 Tag 绑定，再调用 mox_hmi_capture_preview 查看视觉效果；最后用 mox_hmi_validate_project 校验，并按优先级列出布局、可读性、绑定和运行时交互问题。"
                            )
                        }
                    }
                ]
            }))
        }
        _ => Err((-32602, format!("未知提示词 `{name}`"))),
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn rpc(hub: &ControlHub, method: &str, params: Value) -> Value {
        handle_jsonrpc(
            hub,
            json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }),
        )
        .await
        .expect("response")
    }

    #[tokio::test]
    async fn exposes_rich_tool_metadata() {
        let hub = ControlHub::new(None);
        let response = rpc(&hub, "tools/list", json!({})).await;
        let tools = response["result"]["tools"].as_array().expect("tools array");
        let names = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&CAPTURE_PREVIEW_TOOL));
        assert!(names.contains(&LIST_TAGS_TOOL));
        assert!(names.contains(&GET_NODE_TOOL));
        assert!(names.contains(&SEARCH_NODES_TOOL));
        assert!(names.contains(&VALIDATE_PROJECT_TOOL));
        assert!(names.contains(&EXPORT_PROJECT_TOOL));

        let execute = tools
            .iter()
            .find(|tool| tool["name"] == EXECUTE_COMMAND_TOOL)
            .expect("execute tool");
        assert_eq!(execute["annotations"]["readOnlyHint"], false);
        assert_eq!(execute["annotations"]["destructiveHint"], true);
        let actions = execute["inputSchema"]["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum");
        assert!(
            actions
                .iter()
                .any(|action| action == "execute_editor_commands")
        );
        assert!(actions.iter().any(|action| action == "capture_preview") == false);
    }

    #[tokio::test]
    async fn exposes_resources_and_prompts() {
        let hub = ControlHub::new(None);
        let resources = rpc(&hub, "resources/list", json!({})).await;
        let uris = resources["result"]["resources"]
            .as_array()
            .expect("resources array")
            .iter()
            .filter_map(|resource| resource["uri"].as_str())
            .collect::<Vec<_>>();
        assert!(uris.contains(&"mox-hmi://status"));
        assert!(uris.contains(&"mox-hmi://project"));
        assert!(uris.contains(&"mox-hmi://commands"));

        let status = rpc(&hub, "resources/read", json!({ "uri": "mox-hmi://status" })).await;
        assert_eq!(
            status["result"]["contents"][0]["mimeType"],
            "application/json"
        );

        let prompts = rpc(&hub, "prompts/list", json!({})).await;
        assert_eq!(prompts["result"]["prompts"][0]["name"], "mox_hmi_review");
        let prompt = rpc(
            &hub,
            "prompts/get",
            json!({ "name": "mox_hmi_review", "arguments": { "screen_id": "screen.bas_2d" } }),
        )
        .await;
        assert!(
            prompt["result"]["messages"][0]["content"]["text"]
                .as_str()
                .is_some_and(|text| text.contains("mox_hmi_capture_preview"))
        );
    }
}
