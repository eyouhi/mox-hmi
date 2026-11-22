use crate::control::{self, AiConnection, AiStatus};
use dioxus::prelude::*;

#[component]
pub fn AiControlPanel() -> Element {
    let connection = control::AI_CONNECTION();
    let activities = control::AI_ACTIVITY();
    let mut collapsed = use_signal(|| false);

    if connection == AiConnection::Disabled {
        return rsx! {};
    }
    let has_activity = !activities.is_empty();
    if !has_activity && !matches!(connection, AiConnection::Connected(_)) {
        return rsx! {};
    }

    let (status_text, status_class, url) = match &connection {
        AiConnection::Connecting(url) => ("正在连接", "is-waiting", url.clone()),
        AiConnection::Connected(url) => ("已连接", "is-ok", url.clone()),
        AiConnection::Disconnected(reason, url) => (reason.as_str(), "is-error", url.clone()),
        AiConnection::Disabled => ("已关闭", "is-disabled", String::new()),
    };
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
                div { class: "ai-tunnel-meta",
                    code { title: "{url}", "{url}" }
                    span { "每一步均通过现有命令与撤销栈执行" }
                }
                ul {
                    class: "ai-activity-list",
                    "data-testid": "ai-activity-list",
                    role: "log",
                    aria_live: "polite",
                    aria_atomic: "false",
                    if activities.is_empty() {
                        li { class: "ai-empty",
                            strong { "等待 AI 操作" }
                            span { "连接 MCP 控制工具后，工具调用会实时显示在这里。" }
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
                    span { "浏览器是唯一执行端，AI 不直接修改文件。" }
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
