use crate::control::{self, AiConnection, AiStatus};
use dioxus::prelude::*;

#[component]
pub fn AiControlPanel() -> Element {
    let connection = control::AI_CONNECTION();
    let activities = control::AI_ACTIVITY();
    let webmcp_available = control::WEBMCP_AVAILABLE();
    let mcp_settings_available = control::MCP_SETTINGS_AVAILABLE();
    let mcp_enabled = control::MCP_ENABLED();
    let mcp_endpoint = control::MCP_ENDPOINT();
    let mut collapsed = use_signal(|| false);
    let mut mcp_feedback = use_signal(String::new);
    let mut copy_label = use_signal(|| "复制命令".to_string());
    let mut copy_error = use_signal(String::new);

    if !mcp_settings_available && connection == AiConnection::Disabled && activities.is_empty() {
        return rsx! {};
    }
    let has_activity = !activities.is_empty();
    if !mcp_settings_available && !has_activity && !matches!(connection, AiConnection::Connected(_))
    {
        return rsx! {};
    }

    let (status_text, status_class, url) = if mcp_settings_available {
        if !mcp_enabled {
            ("MCP 已关闭", "is-disabled", mcp_endpoint.clone())
        } else {
            match &connection {
                AiConnection::Connecting(_) => ("正在连接", "is-waiting", mcp_endpoint.clone()),
                AiConnection::Connected(_) => ("已连接", "is-ok", mcp_endpoint.clone()),
                AiConnection::Disconnected(reason, _) => {
                    (reason.as_str(), "is-error", mcp_endpoint.clone())
                }
                AiConnection::Disabled => ("等待 Codex", "is-waiting", mcp_endpoint.clone()),
            }
        }
    } else if webmcp_available && connection == AiConnection::Disabled {
        (
            "WebMCP 就绪",
            "is-ok",
            "webmcp://document.modelContext".to_string(),
        )
    } else {
        match &connection {
            AiConnection::Connecting(url) => ("正在连接", "is-waiting", url.clone()),
            AiConnection::Connected(url) => ("已连接", "is-ok", url.clone()),
            AiConnection::Disconnected(reason, url) => (reason.as_str(), "is-error", url.clone()),
            AiConnection::Disabled => ("已关闭", "is-disabled", String::new()),
        }
    };
    let endpoint_label = if mcp_settings_available && !mcp_enabled {
        "MCP 端点已关闭".to_string()
    } else {
        url.clone()
    };
    let command = if mcp_settings_available && mcp_enabled {
        control::codex_mcp_command()
    } else {
        String::new()
    };
    let command_for_copy = command.clone();
    let running = activities
        .iter()
        .any(|activity| activity.status == AiStatus::Running);
    let panel_class = if collapsed() {
        "ai-tunnel is-collapsed"
    } else {
        "ai-tunnel"
    };

    rsx! {
        aside { class: "{panel_class}", "data-testid": "ai-tunnel", aria_label: "AI 操作隧道",
            header { class: "ai-tunnel-head",
                div { class: "ai-tunnel-title",
                    span { class: "ai-tunnel-mark", aria_hidden: "true", "AI" }
                    div {
                        div { class: "eyebrow", "Control Tunnel" }
                        strong { "AI 操作隧道" }
                    }
                }
                span {
                    class: "ai-tunnel-status {status_class}",
                    "data-testid": "ai-connection",
                    role: "status",
                    aria_live: "polite",
                    aria_atomic: "true",
                    if running { span { class: "ai-pulse", aria_hidden: "true" } }
                    "{status_text}"
                }
                button {
                    class: "ai-tunnel-icon",
                    aria_label: if collapsed() { "展开 AI 操作隧道" } else { "收起 AI 操作隧道" },
                    aria_expanded: if collapsed() { "false" } else { "true" },
                    aria_controls: "ai-tunnel-body",
                    onclick: move |_| collapsed.set(!collapsed()),
                    if collapsed() { "+" } else { "−" }
                }
            }
            div { id: "ai-tunnel-body", class: "ai-tunnel-body", hidden: collapsed(),
                if mcp_settings_available {
                    div { class: "ai-mcp-setting",
                        div { class: "ai-mcp-setting-copy",
                            strong { "MCP 控制" }
                            span {
                                if mcp_enabled {
                                    "已启用。Codex 可通过本机 MCP 端点操作当前工程。"
                                } else {
                                    "默认关闭。开启后 Codex 才能连接并调用 mox_hmi_* 工具。"
                                }
                            }
                        }
                        button {
                            class: if mcp_enabled { "ai-switch is-on" } else { "ai-switch" },
                            r#type: "button",
                            role: "switch",
                            "aria-checked": if mcp_enabled { "true" } else { "false" },
                            aria_label: if mcp_enabled { "关闭 MCP" } else { "开启 MCP" },
                            "data-testid": "mcp-switch",
                            onclick: move |_| {
                                mcp_feedback.set(String::new());
                                copy_label.set("复制命令".to_string());
                                copy_error.set(String::new());
                                if let Err(error) = control::set_mcp_enabled(!mcp_enabled) {
                                    mcp_feedback.set(format!("保存 MCP 开关失败：{error}"));
                                }
                            },
                            span { class: "ai-switch-track", aria_hidden: "true" }
                            span { class: "ai-switch-label", if mcp_enabled { "已开启" } else { "已关闭" } }
                        }
                    }
                }
                div { class: "ai-tunnel-meta",
                    code { title: "{endpoint_label}", "{endpoint_label}" }
                    span { "每一步均通过现有命令与撤销栈执行" }
                }
                if !command.is_empty() {
                    div { class: "ai-codex-command",
                        div { class: "ai-codex-command-head",
                            strong { "Codex 连接命令" }
                            div { class: "ai-codex-command-tools",
                                span { "在终端执行一次" }
                                button {
                                    class: "ai-copy-command",
                                    r#type: "button",
                                    "data-testid": "copy-codex-command",
                                    onclick: move |_| {
                                        let command = command_for_copy.clone();
                                        copy_label.set("复制中".to_string());
                                        copy_error.set(String::new());
                                        spawn(async move {
                                            match control::copy_mcp_command(&command).await {
                                                Ok(()) => copy_label.set("已复制".to_string()),
                                                Err(error) => {
                                                    copy_label.set("复制失败".to_string());
                                                    copy_error.set(format!("复制 Codex 命令失败：{error}"));
                                                }
                                            }
                                        });
                                    },
                                    "{copy_label}"
                                }
                            }
                        }
                        code {
                            class: "ai-codex-command-code",
                            title: "{command}",
                            "data-testid": "codex-mcp-command",
                            "{command}"
                        }
                        span { class: "ai-codex-command-note",
                            "执行后新开一个 Codex 会话。命令包含本机控制令牌，请勿外传。"
                        }
                    }
                }
                if !mcp_feedback().is_empty() {
                    p { class: "ai-setting-error", role: "alert", "{mcp_feedback()}" }
                }
                if !copy_error().is_empty() {
                    p { class: "ai-setting-error", role: "alert", "{copy_error()}" }
                }
                ul {
                    class: "ai-activity-list",
                    "data-testid": "ai-activity-list",
                    role: "log",
                    aria_live: "polite",
                    aria_atomic: "false",
                    if activities.is_empty() {
                        li { class: "ai-empty",
                            strong {
                                if mcp_settings_available && !mcp_enabled {
                                    "MCP 尚未启用"
                                } else {
                                    "等待 AI 操作"
                                }
                            }
                            span {
                                if mcp_settings_available && !mcp_enabled {
                                    "打开上方开关后，这里会显示 Codex 连接命令。"
                                } else {
                                    "连接 MCP，或在支持 WebMCP 的浏览器中直接调用工具，操作会实时显示在这里。"
                                }
                            }
                        }
                    }
                    for activity in activities {
                        li {
                            key: "{activity.id}",
                            class: "ai-activity is-{activity.status.as_str()}",
                            "data-testid": "ai-activity",
                            span { class: "ai-activity-time", "{activity.time}" }
                            div { class: "ai-activity-copy",
                                strong { "{activity.label}" }
                                span { "{activity.detail}" }
                            }
                            span { class: "ai-activity-state",
                                match activity.status {
                                    AiStatus::Running => "执行中",
                                    AiStatus::Ok => "完成",
                                    AiStatus::Error => "失败",
                                }
                            }
                        }
                    }
                }
                footer { class: "ai-tunnel-foot",
                    span { "编辑器是唯一执行端，AI 不直接修改文件。" }
                    div { class: "ai-tunnel-actions",
                        button {
                            class: "mini-button",
                            onclick: move |_| control::clear_activities(),
                            "清空"
                        }
                        button {
                            class: "mini-button",
                            onclick: move |_| control::reconnect(),
                            "重连"
                        }
                    }
                }
            }
        }
    }
}
