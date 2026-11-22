use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HmiError {
    #[error("Tag `{0}` 不存在")]
    UnknownTag(String),
    #[error("Tag `{0}` 不可写")]
    TagNotWritable(String),
    #[error("画面 `{0}` 不存在")]
    UnknownScreen(String),
    #[error("节点 `{0}` 不存在")]
    UnknownNode(String),
    #[error("ID `{0}` 重复")]
    DuplicateId(String),
    #[error("命令无效：{0}")]
    InvalidCommand(String),
    #[error("Tag `{tag_id}` 正被节点引用：{nodes:?}")]
    TagInUse { tag_id: String, nodes: Vec<String> },
    #[error("JSON 无效：{0}")]
    Json(String),
}
