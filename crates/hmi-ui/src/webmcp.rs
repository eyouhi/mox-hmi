#[cfg(target_arch = "wasm32")]
use hmi_control_protocol::tools;
#[cfg(target_arch = "wasm32")]
use serde_json::{Value, json};

pub fn start() {
    #[cfg(target_arch = "wasm32")]
    imp::start();
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use js_sys::{Function, Object, Reflect};
    use std::cell::Cell;
    use wasm_bindgen::{JsCast, JsValue, closure::Closure};
    use wasm_bindgen_futures::{JsFuture, future_to_promise, spawn_local};

    thread_local! {
        static STARTED: Cell<bool> = const { Cell::new(false) };
    }

    pub fn start() {
        if STARTED.with(|started| started.replace(true)) {
            return;
        }
        let Some(model_context) = model_context() else {
            return;
        };

        crate::control::mark_webmcp_available();
        let mut pending = Vec::new();
        for spec in tools::tool_specs() {
            match register_tool(&model_context, &spec) {
                Ok(Some(promise)) => pending.push(promise),
                Ok(None) => {}
                Err(error) => {
                    web_sys::console::warn_1(&JsValue::from_str(&format!(
                        "MOX HMI WebMCP: 注册工具 `{}` 失败：{error}",
                        spec.name
                    )));
                }
            }
        }

        let context = model_context.clone();
        spawn_local(async move {
            for promise in pending {
                if let Err(error) = JsFuture::from(promise).await {
                    web_sys::console::warn_1(&JsValue::from_str(&format!(
                        "MOX HMI WebMCP: 工具注册被浏览器拒绝：{}",
                        js_error_message(error)
                    )));
                }
            }
            verify_registered(&context).await;
        });
    }

    fn model_context() -> Option<JsValue> {
        let window = web_sys::window()?;
        let document = window.document()?;
        let document_value: JsValue = document.into();
        if let Some(context) = model_context_on(&document_value) {
            return Some(context);
        }
        let navigator_value: JsValue = window.navigator().into();
        model_context_on(&navigator_value)
    }

    fn model_context_on(target: &JsValue) -> Option<JsValue> {
        let context = Reflect::get(target, &JsValue::from_str("modelContext")).ok()?;
        if context.is_undefined() || context.is_null() || !has_method(&context, "registerTool") {
            return None;
        }
        Some(context)
    }

    fn has_method(target: &JsValue, name: &str) -> bool {
        Reflect::get(target, &JsValue::from_str(name))
            .ok()
            .is_some_and(|value| value.is_function())
    }

    fn register_tool(
        model_context: &JsValue,
        spec: &tools::ToolSpec,
    ) -> Result<Option<js_sys::Promise>, String> {
        let tool = Object::new();
        set(&tool, "name", JsValue::from_str(spec.name))?;
        set(&tool, "description", JsValue::from_str(spec.description))?;
        let schema = serde_json::to_string(&spec.input_schema)
            .map_err(|error| format!("无法序列化工具 schema：{error}"))?;
        let schema = js_sys::JSON::parse(&schema).map_err(js_error_message)?;
        set(&tool, "inputSchema", schema)?;

        let annotations = Object::new();
        set(
            &annotations,
            "readOnlyHint",
            JsValue::from_bool(spec.read_only_hint),
        )?;
        set(
            &annotations,
            "consequentialHint",
            JsValue::from_bool(spec.consequential_hint),
        )?;
        set(
            &annotations,
            "untrustedContentHint",
            JsValue::from_bool(spec.untrusted_content_hint),
        )?;
        set(&tool, "annotations", annotations.into())?;

        let name = spec.name.to_string();
        let execute = Closure::<dyn Fn(JsValue, JsValue) -> JsValue>::new(
            move |input: JsValue, context: JsValue| {
                execute_tool_promise(name.clone(), input, context)
            },
        );
        set(&tool, "execute", execute.into_js_value())?;

        let tool_value: JsValue = tool.into();
        let register = Reflect::get(model_context, &JsValue::from_str("registerTool"))
            .map_err(js_error_message)?
            .dyn_into::<Function>()
            .map_err(|_| "document.modelContext.registerTool 不可调用".to_string())?;
        let result = register
            .call1(model_context, &tool_value)
            .map_err(js_error_message)?;
        Ok(result.dyn_into::<js_sys::Promise>().ok())
    }

    fn set(target: &Object, name: &str, value: JsValue) -> Result<(), String> {
        Reflect::set(target.as_ref(), &JsValue::from_str(name), &value)
            .map(|_| ())
            .map_err(js_error_message)
    }

    fn execute_tool_promise(name: String, input: JsValue, context: JsValue) -> JsValue {
        future_to_promise(async move {
            if signal_aborted(&context) {
                return Err(js_error("工具执行已取消"));
            }
            let input = input_to_value(&input).map_err(js_error)?;
            let result = execute_tool(&name, &input).map_err(js_error)?;
            let payload = serde_json::to_string(&result)
                .map_err(|error| js_error(format!("无法序列化工具结果：{error}")))?;
            js_sys::JSON::parse(&payload).map_err(|error| js_error(js_error_message(error)))
        })
        .into()
    }

    fn execute_tool(name: &str, input: &Value) -> Result<Value, String> {
        match name {
            tools::STATUS_TOOL => Ok(status_payload()),
            tools::LIST_COMMANDS_TOOL => Ok(tools::command_catalog()),
            tools::INSPECT_PROJECT_TOOL => {
                let (result, duration_ms) =
                    crate::control::track_action("inspect_project", "WebMCP 读取工程", input);
                let value = result?;
                Ok(action_envelope(value, duration_ms))
            }
            tools::LIST_TAGS_TOOL => tracked("list_tags", "WebMCP 读取 Tag", input),
            tools::GET_NODE_TOOL => tracked("get_node", "WebMCP 读取节点", input),
            tools::SEARCH_NODES_TOOL => tracked("search_nodes", "WebMCP 搜索节点", input),
            tools::VALIDATE_PROJECT_TOOL => tracked("validate_project", "WebMCP 校验工程", input),
            tools::EXPORT_PROJECT_TOOL => tracked("export_project", "WebMCP 导出工程", input),
            tools::CAPTURE_PREVIEW_TOOL => tracked("capture_preview", "WebMCP 截取画面", input),
            tools::EXECUTE_COMMAND_TOOL => {
                let action = input
                    .get("action")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "缺少字符串参数 action".to_string())?;
                let label = input
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("WebMCP · {action}"));
                let params = input.get("params").cloned().unwrap_or_else(|| json!({}));
                let (result, duration_ms) = crate::control::track_action(action, &label, &params);
                let value = result?;
                Ok(action_envelope(value, duration_ms))
            }
            other => Err(format!("未知 WebMCP 工具 `{other}`")),
        }
    }

    fn status_payload() -> Value {
        let tunnel = match crate::control::AI_CONNECTION() {
            crate::control::AiConnection::Disabled => "disabled",
            crate::control::AiConnection::Connecting(_) => "connecting",
            crate::control::AiConnection::Connected(_) => "connected",
            crate::control::AiConnection::Disconnected(_, _) => "disconnected",
        };
        json!({
            "connected": true,
            "transport": "webmcp",
            "session_id": null,
            "token_required": false,
            "mcp_tunnel": tunnel,
            "state": crate::control::current_state(),
        })
    }

    fn action_envelope(value: Value, duration_ms: u64) -> Value {
        json!({
            "ok": true,
            "result": value,
            "state": crate::control::current_state(),
            "duration_ms": duration_ms,
        })
    }

    fn tracked(action: &str, label: &str, input: &Value) -> Result<Value, String> {
        let (result, duration_ms) = crate::control::track_action(action, label, input);
        let value = result?;
        Ok(action_envelope(value, duration_ms))
    }

    fn input_to_value(input: &JsValue) -> Result<Value, String> {
        if input.is_undefined() || input.is_null() {
            return Ok(json!({}));
        }
        let serialized = if let Some(text) = input.as_string() {
            text
        } else {
            js_sys::JSON::stringify(input)
                .map_err(js_error_message)?
                .as_string()
                .ok_or_else(|| "工具输入无法序列化为 JSON".to_string())?
        };
        serde_json::from_str(&serialized).map_err(|error| format!("工具输入不是合法 JSON：{error}"))
    }

    fn signal_aborted(context: &JsValue) -> bool {
        if !context.is_object() {
            return false;
        }
        let Ok(signal) = Reflect::get(context, &JsValue::from_str("signal")) else {
            return false;
        };
        Reflect::get(&signal, &JsValue::from_str("aborted"))
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }

    async fn verify_registered(model_context: &JsValue) {
        let Some(get_tools) = Reflect::get(model_context, &JsValue::from_str("getTools"))
            .ok()
            .and_then(|value| value.dyn_into::<Function>().ok())
        else {
            return;
        };
        let Ok(promise) = get_tools.call0(model_context) else {
            web_sys::console::warn_1(&JsValue::from_str("MOX HMI WebMCP: getTools() 调用失败"));
            return;
        };
        let Ok(promise) = promise.dyn_into::<js_sys::Promise>() else {
            return;
        };
        let Ok(value) = JsFuture::from(promise).await else {
            web_sys::console::warn_1(&JsValue::from_str(
                "MOX HMI WebMCP: getTools() 未返回工具列表",
            ));
            return;
        };
        let Ok(tools) = value.dyn_into::<js_sys::Array>() else {
            return;
        };
        let names: Vec<String> = (0..tools.length())
            .filter_map(|index| {
                Reflect::get(&tools.get(index), &JsValue::from_str("name"))
                    .ok()
                    .and_then(|value| value.as_string())
            })
            .collect();
        let missing: Vec<_> = tools::tool_specs()
            .into_iter()
            .filter(|spec| !names.iter().any(|name| name == spec.name))
            .map(|spec| spec.name)
            .collect();
        if missing.is_empty() {
            web_sys::console::log_1(&JsValue::from_str(&format!(
                "MOX HMI WebMCP: 已注册 {} 个工具",
                names.len()
            )));
        } else {
            web_sys::console::warn_1(&JsValue::from_str(&format!(
                "MOX HMI WebMCP: 缺少工具 {missing:?}"
            )));
        }
    }

    fn js_error(message: impl AsRef<str>) -> JsValue {
        js_sys::Error::new(message.as_ref()).into()
    }

    fn js_error_message(error: JsValue) -> String {
        error
            .as_string()
            .or_else(|| {
                Reflect::get(&error, &JsValue::from_str("message"))
                    .ok()
                    .and_then(|value| value.as_string())
            })
            .unwrap_or_else(|| format!("{error:?}"))
    }
}
