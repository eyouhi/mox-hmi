use crate::{
    ProjectDocument, SCHEMA_VERSION,
    validation::{MAX_IMPORT_BYTES, ValidationIssue, validate_project},
};
use thiserror::Error;

pub const STORAGE_KEY: &str = "mox-hmi.project.v1";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PersistenceError {
    #[error("工程文件超过 5 MB 限制")]
    TooLarge,
    #[error("JSON 解析失败：{0}")]
    Json(String),
    #[error("工程校验失败：{}", .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
    Validation(Vec<ValidationIssue>),
}

pub fn to_json(project: &ProjectDocument) -> Result<String, PersistenceError> {
    validate_project(project).map_err(PersistenceError::Validation)?;
    serde_json::to_string_pretty(project).map_err(|error| PersistenceError::Json(error.to_string()))
}

pub fn from_json(raw: &str) -> Result<ProjectDocument, PersistenceError> {
    if raw.len() > MAX_IMPORT_BYTES {
        return Err(PersistenceError::TooLarge);
    }
    let project: ProjectDocument =
        serde_json::from_str(raw).map_err(|error| PersistenceError::Json(error.to_string()))?;
    if project.schema_version != SCHEMA_VERSION {
        return Err(PersistenceError::Validation(vec![ValidationIssue {
            path: "schema_version".into(),
            message: format!("只支持 schema {SCHEMA_VERSION}"),
        }]));
    }
    validate_project(&project).map_err(PersistenceError::Validation)?;
    Ok(project)
}

pub fn export_filename(project: &ProjectDocument) -> String {
    let raw = project.id.strip_prefix("project.").unwrap_or(&project.id);
    let mut clean = String::new();
    let mut last_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            clean.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !clean.is_empty() {
            clean.push('-');
            last_dash = true;
        }
    }
    while clean.ends_with('-') {
        clean.pop();
    }
    if clean.is_empty() {
        clean.push_str("project");
    }
    format!("{clean}.moxhmi.json")
}
