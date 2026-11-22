use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagValueType {
    Number,
    Bool,
    Enum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulatorKind {
    Constant,
    Sine,
    Pulse,
    Step,
    RandomWalk,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TagValue {
    Number(f32),
    Bool(bool),
    Enum(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagDefinition {
    pub id: String,
    pub name: String,
    pub value_type: TagValueType,
    pub unit: String,
    #[serde(default)]
    pub min: Option<f32>,
    #[serde(default)]
    pub max: Option<f32>,
    pub precision: u8,
    pub initial: TagValue,
    pub writable: bool,
    pub simulator: SimulatorKind,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TagSnapshot {
    values: BTreeMap<String, TagValue>,
    revision: u64,
}

impl TagSnapshot {
    pub fn new(values: BTreeMap<String, TagValue>, revision: u64) -> Self {
        Self { values, revision }
    }

    pub fn get(&self, id: &str) -> Option<&TagValue> {
        self.values.get(id)
    }

    pub fn values(&self) -> &BTreeMap<String, TagValue> {
        &self.values
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn set(&mut self, id: impl Into<String>, value: TagValue) -> bool {
        let id = id.into();
        if self.values.get(&id) == Some(&value) {
            return false;
        }
        self.values.insert(id, value);
        self.revision += 1;
        true
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let removed = self.values.remove(id).is_some();
        if removed {
            self.revision += 1;
        }
        removed
    }
}

impl TagValue {
    pub fn matches_type(&self, ty: TagValueType) -> bool {
        matches!(
            (self, ty),
            (TagValue::Number(_), TagValueType::Number)
                | (TagValue::Bool(_), TagValueType::Bool)
                | (TagValue::Enum(_), TagValueType::Enum)
        )
    }

    pub fn as_number(&self) -> Option<f32> {
        match self {
            TagValue::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            TagValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn display_text(&self) -> String {
        match self {
            TagValue::Number(value) => value.to_string(),
            TagValue::Bool(value) => {
                if *value {
                    "运行".to_string()
                } else {
                    "停止".to_string()
                }
            }
            TagValue::Enum(value) => value.clone(),
        }
    }
}
