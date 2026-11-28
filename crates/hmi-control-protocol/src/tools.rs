use serde_json::{Value, json};

pub const STATUS_TOOL: &str = "mox_hmi_status";
pub const LIST_COMMANDS_TOOL: &str = "mox_hmi_list_commands";
pub const INSPECT_PROJECT_TOOL: &str = "mox_hmi_inspect_project";
pub const LIST_TAGS_TOOL: &str = "mox_hmi_list_tags";
pub const GET_NODE_TOOL: &str = "mox_hmi_get_node";
pub const SEARCH_NODES_TOOL: &str = "mox_hmi_search_nodes";
pub const VALIDATE_PROJECT_TOOL: &str = "mox_hmi_validate_project";
pub const EXPORT_PROJECT_TOOL: &str = "mox_hmi_export_project";
pub const CAPTURE_PREVIEW_TOOL: &str = "mox_hmi_capture_preview";
pub const EXECUTE_COMMAND_TOOL: &str = "mox_hmi_execute_command";

pub const EXECUTE_ACTIONS: &[&str] = &[
    "select_screen",
    "select_node",
    "select_nodes",
    "select_all_nodes",
    "set_mode",
    "execute_editor_command",
    "execute_editor_commands",
    "copy_selection",
    "cut_selection",
    "paste_clipboard",
    "duplicate_selection",
    "delete_selection",
    "write_tag",
    "read_tag",
    "read_tags",
    "invoke_button",
    "tick_tags",
    "resume_tag",
    "undo",
    "redo",
    "save",
    "import_project",
    "set_view_2d",
    "set_view_3d",
    "align_selected_3d",
    "focus_selection",
    "reset_camera_3d",
];

pub const EDITOR_COMMAND_TYPES: &[&str] = &[
    "add_screen",
    "delete_screen",
    "rename_screen",
    "set_startup_screen",
    "add2d_node",
    "update2d_node",
    "add3d_node",
    "update3d_node",
    "delete_node",
    "duplicate_node",
    "set_bindings",
    "add_tag",
    "update_tag",
    "delete_tag",
];

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
            "annotations": {
                "readOnlyHint": self.read_only_hint,
                "destructiveHint": self.consequential_hint,
                "idempotentHint": self.read_only_hint,
                "openWorldHint": false,
                "consequentialHint": self.consequential_hint,
                "untrustedContentHint": self.untrusted_content_hint,
            },
        })
    }
}

pub fn execute_action_names() -> Vec<&'static str> {
    EXECUTE_ACTIONS.to_vec()
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
            description: "列出可以执行的 HMI 控制动作、查询工具和 EditorCommand 类型。",
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
            name: LIST_TAGS_TOOL,
            description: "列出 Tag 定义和当前仿真值，可按 id/name/unit 搜索。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "可选，匹配 Tag id、名称或单位" },
                    "include_values": { "type": "boolean", "description": "是否附带当前值，默认 true" }
                },
                "additionalProperties": false
            }),
            read_only_hint: true,
            consequential_hint: false,
            untrusted_content_hint: true,
        },
        ToolSpec {
            name: GET_NODE_TOOL,
            description: "按 node_id 读取单个 2D/3D 节点；不传 screen_id 时使用当前画面。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": { "type": "string" },
                    "screen_id": { "type": "string", "description": "可选，指定画面" }
                },
                "required": ["node_id"],
                "additionalProperties": false
            }),
            read_only_hint: true,
            consequential_hint: false,
            untrusted_content_hint: true,
        },
        ToolSpec {
            name: SEARCH_NODES_TOOL,
            description: "按 id、名称或 widget/label 内容搜索节点，返回所在画面和节点 JSON。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "可选，空字符串表示列出节点" },
                    "screen_id": { "type": "string", "description": "可选，只搜索指定画面" },
                    "kind": { "type": "string", "enum": ["2d", "3d"], "description": "可选，只搜索 2D 或 3D 节点" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "description": "默认 50" }
                },
                "additionalProperties": false
            }),
            read_only_hint: true,
            consequential_hint: false,
            untrusted_content_hint: true,
        },
        ToolSpec {
            name: VALIDATE_PROJECT_TOOL,
            description: "运行工程校验并返回全部问题；适合在批量编辑后检查结果。",
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
            name: EXPORT_PROJECT_TOOL,
            description: "导出当前工程的规范 JSON 和下载文件名。",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            read_only_hint: true,
            consequential_hint: false,
            untrusted_content_hint: true,
        },
        ToolSpec {
            name: CAPTURE_PREVIEW_TOOL,
            description: "截取当前 HMI 画布并返回 PNG 图片，用于编辑后的视觉校验。原生编辑器返回 Bevy 纹理；Web 端返回 canvas 快照。",
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
            name: EXECUTE_COMMAND_TOOL,
            description: "在当前 HMI 页面中执行动作；先调用 mox_hmi_list_commands 查看动作参数。动作走现有 EditorCommand/撤销/校验/自动保存，每一步会显示在用户界面的 AI 操作隧道中。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": EXECUTE_ACTIONS
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
        "tools": [
            { "name": STATUS_TOOL, "kind": "read" },
            { "name": LIST_COMMANDS_TOOL, "kind": "read" },
            { "name": INSPECT_PROJECT_TOOL, "kind": "read" },
            { "name": LIST_TAGS_TOOL, "kind": "read" },
            { "name": GET_NODE_TOOL, "kind": "read" },
            { "name": SEARCH_NODES_TOOL, "kind": "read" },
            { "name": VALIDATE_PROJECT_TOOL, "kind": "read" },
            { "name": EXPORT_PROJECT_TOOL, "kind": "read" },
            { "name": CAPTURE_PREVIEW_TOOL, "kind": "read" },
            { "name": EXECUTE_COMMAND_TOOL, "kind": "write", "actions": EXECUTE_ACTIONS }
        ],
        "actions": [
            { "name": "select_screen", "params": { "screen_id": "string" } },
            { "name": "select_node", "params": { "node_id": "string|null" } },
            { "name": "select_nodes", "params": { "node_ids": ["string"], "mode": "replace|add|toggle", "screen_id": "optional string" } },
            { "name": "select_all_nodes", "params": {} },
            { "name": "set_mode", "params": { "mode": "edit|run" } },
            { "name": "execute_editor_command", "params": { "command": "EditorCommand JSON object" } },
            { "name": "execute_editor_commands", "params": { "commands": ["EditorCommand JSON object"] } },
            { "name": "copy_selection", "params": {} },
            { "name": "cut_selection", "params": {} },
            { "name": "paste_clipboard", "params": {} },
            { "name": "duplicate_selection", "params": {} },
            { "name": "delete_selection", "params": {} },
            { "name": "write_tag", "params": { "tag_id": "string", "value": "number|boolean|string" } },
            { "name": "read_tag", "params": { "tag_id": "string" } },
            { "name": "read_tags", "params": {} },
            { "name": "invoke_button", "params": { "node_id": "string", "screen_id": "optional string" } },
            { "name": "tick_tags", "params": { "dt_ms": "number (default 250)" } },
            { "name": "resume_tag", "params": { "tag_id": "string" } },
            { "name": "undo", "params": {} },
            { "name": "redo", "params": {} },
            { "name": "save", "params": {} },
            { "name": "import_project", "params": { "json": "project JSON string", "project": "optional project object" } },
            { "name": "set_view_2d", "params": { "zoom": "optional number 0.5..4.0", "center": "optional [x, y]" } },
            { "name": "set_view_3d", "params": { "focus": "optional [x, y, z]" } },
            { "name": "align_selected_3d", "params": { "axis": "0|1|2" } },
            { "name": "focus_selection", "params": {} },
            { "name": "reset_camera_3d", "params": {} }
        ],
        "queries": [
            { "name": "list_tags", "params": { "query": "optional string", "include_values": "optional boolean" } },
            { "name": "get_node", "params": { "node_id": "string", "screen_id": "optional string" } },
            { "name": "search_nodes", "params": { "query": "optional string", "screen_id": "optional string", "kind": "optional 2d|3d", "limit": "optional number" } },
            { "name": "validate_project", "params": {} },
            { "name": "export_project", "params": {} },
            { "name": "capture_preview", "params": {} }
        ],
        "editor_command_types": EDITOR_COMMAND_TYPES,
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
                LIST_TAGS_TOOL,
                GET_NODE_TOOL,
                SEARCH_NODES_TOOL,
                VALIDATE_PROJECT_TOOL,
                EXPORT_PROJECT_TOOL,
                CAPTURE_PREVIEW_TOOL,
                EXECUTE_COMMAND_TOOL,
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
                .map(|actions| actions.iter().filter_map(Value::as_str).collect::<Vec<_>>()),
            Some(EXECUTE_ACTIONS.to_vec())
        );
    }

    #[test]
    fn mcp_metadata_exposes_annotations() {
        let specs = tool_specs();
        let status = specs
            .iter()
            .find(|spec| spec.name == STATUS_TOOL)
            .expect("status tool")
            .mcp_json();
        assert_eq!(status["annotations"]["readOnlyHint"], true);
        assert_eq!(status["annotations"]["destructiveHint"], false);
        assert_eq!(status["annotations"]["idempotentHint"], true);

        let execute = specs
            .iter()
            .find(|spec| spec.name == EXECUTE_COMMAND_TOOL)
            .expect("execute tool")
            .mcp_json();
        assert_eq!(execute["annotations"]["readOnlyHint"], false);
        assert_eq!(execute["annotations"]["destructiveHint"], true);
    }

    #[test]
    fn command_catalog_matches_the_execute_schema() {
        let catalog = command_catalog();
        let catalog_actions: Vec<_> = catalog["actions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|action| action["name"].as_str())
            .collect();
        assert_eq!(catalog_actions, EXECUTE_ACTIONS);
        assert!(
            catalog["editor_command_types"]
                .as_array()
                .is_some_and(|types| types.iter().any(|kind| kind == "add3d_node"))
        );
        assert!(catalog["queries"].as_array().is_some_and(|queries| {
            queries
                .iter()
                .any(|query| query["name"] == "capture_preview")
        }));
    }
}
