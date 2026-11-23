use serde_json::{Value, json};

pub const STATUS_TOOL: &str = "mox_hmi_status";
pub const LIST_COMMANDS_TOOL: &str = "mox_hmi_list_commands";
pub const INSPECT_PROJECT_TOOL: &str = "mox_hmi_inspect_project";
pub const EXECUTE_COMMAND_TOOL: &str = "mox_hmi_execute_command";

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub read_only_hint: bool,
    pub consequential_hint: bool,
    pub untrusted_content_hint: bool,
}

impl ToolSpec {
    pub fn mcp_json(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        })
    }
}

pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: STATUS_TOOL,
            description: "查看 HMI 是否已连接、当前画面、选择、编辑/运行模式和修订号。",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            read_only_hint: true,
            consequential_hint: false,
            untrusted_content_hint: false,
        },
        ToolSpec {
            name: LIST_COMMANDS_TOOL,
            description: "列出可以执行的 HMI 控制动作和 EditorCommand 类型。",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            read_only_hint: true,
            consequential_hint: false,
            untrusted_content_hint: false,
        },
        ToolSpec {
            name: INSPECT_PROJECT_TOOL,
            description: "读取完整 HMI 工程 JSON，或指定 screen_id 只看一个画面。先用它获取节点、Tag 和绑定结构。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "screen_id": { "type": "string", "description": "可选，只返回该画面" }
                },
                "additionalProperties": false
            }),
            read_only_hint: true,
            consequential_hint: false,
            untrusted_content_hint: true,
        },
        ToolSpec {
            name: EXECUTE_COMMAND_TOOL,
            description: "在当前 HMI 页面中执行动作。动作走现有 EditorCommand/撤销/校验/自动保存，每一步会显示在用户界面的 AI 操作隧道中。",
            input_schema: json!({
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
            }),
            read_only_hint: false,
            consequential_hint: true,
            untrusted_content_hint: false,
        },
    ]
}

pub fn command_catalog() -> Value {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_the_stable_control_tools() {
        let specs = tool_specs();
        let names: Vec<_> = specs.iter().map(|spec| spec.name).collect();
        assert_eq!(
            names,
            vec![
                STATUS_TOOL,
                LIST_COMMANDS_TOOL,
                INSPECT_PROJECT_TOOL,
                EXECUTE_COMMAND_TOOL
            ]
        );
        let execute = specs
            .iter()
            .find(|spec| spec.name == EXECUTE_COMMAND_TOOL)
            .expect("execute tool");
        assert!(execute.consequential_hint);
        assert!(!execute.read_only_hint);
        assert_eq!(
            execute.input_schema["properties"]["action"]["enum"]
                .as_array()
                .map(Vec::len),
            Some(9)
        );
    }

    #[test]
    fn command_catalog_lists_editor_commands() {
        let catalog = command_catalog();
        assert!(
            catalog["editor_command_types"]
                .as_array()
                .is_some_and(|types| types.iter().any(|kind| kind == "add3d_node"))
        );
    }
}
