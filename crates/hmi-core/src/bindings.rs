use crate::{
    Binding, BindingMapping, BindingTarget, HexColor, HmiError, TagDefinition, TagSnapshot,
    TagValue, TagValueType,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum VisualValue {
    Text(String),
    Color(HexColor),
    Visible(bool),
    Progress(f32),
    RotationSpeed(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualPatch {
    pub node_id: String,
    pub value: VisualValue,
}

pub fn format_tag_value(value: &TagValue, precision: u8, unit: &str) -> String {
    let precision = usize::from(precision);
    let body = match value {
        TagValue::Number(number) => format!("{number:.precision$}"),
        TagValue::Bool(true) => "运行".into(),
        TagValue::Bool(false) => "停止".into(),
        TagValue::Enum(value) => value.clone(),
    };
    if unit.is_empty() {
        body
    } else {
        format!("{body} {unit}")
    }
}

pub fn default_mapping(target: BindingTarget, tag: &TagDefinition) -> BindingMapping {
    match (target, tag.value_type) {
        (BindingTarget::Text, _) => BindingMapping::Format {
            precision: None,
            unit: None,
        },
        (BindingTarget::Color, TagValueType::Bool) => BindingMapping::BoolColor {
            true_color: HexColor("#37e6d4".into()),
            false_color: HexColor("#31424b".into()),
        },
        (BindingTarget::Color, TagValueType::Number) => BindingMapping::ThresholdColor {
            thresholds: vec![crate::ThresholdStop {
                value: tag.min.unwrap_or(0.0),
                color: HexColor("#37e6d4".into()),
            }],
        },
        (BindingTarget::Color, TagValueType::Enum) => BindingMapping::Direct,
        (BindingTarget::Visible, _) => BindingMapping::Direct,
        (BindingTarget::Progress, TagValueType::Bool) => BindingMapping::Direct,
        (BindingTarget::Progress, _) => BindingMapping::Range {
            input_min: tag.min.unwrap_or(0.0),
            input_max: tag.max.unwrap_or(100.0),
            output_min: 0.0,
            output_max: 1.0,
            clamp: true,
        },
        (BindingTarget::RotationSpeed, TagValueType::Bool) => BindingMapping::Direct,
        (BindingTarget::RotationSpeed, _) => BindingMapping::Range {
            input_min: tag.min.unwrap_or(0.0),
            input_max: tag.max.unwrap_or(50.0),
            output_min: 0.0,
            output_max: 15.0,
            clamp: true,
        },
    }
}

fn invalid(node_id: &str, binding: &Binding, reason: &str) -> HmiError {
    HmiError::InvalidCommand(format!(
        "节点 `{node_id}` 的 Tag `{}` 绑定无效：{reason}",
        binding.tag_id
    ))
}

fn numeric(value: &TagValue, node_id: &str, binding: &Binding) -> Result<f32, HmiError> {
    match value {
        TagValue::Number(value) if value.is_finite() => Ok(*value),
        TagValue::Number(_) => Err(invalid(node_id, binding, "数值不是有限值")),
        _ => Err(invalid(node_id, binding, "需要 Number Tag")),
    }
}

fn boolean(value: &TagValue, node_id: &str, binding: &Binding) -> Result<bool, HmiError> {
    match value {
        TagValue::Bool(value) => Ok(*value),
        _ => Err(invalid(node_id, binding, "需要 Bool Tag")),
    }
}

#[allow(clippy::too_many_arguments)]
fn map_range(
    input: f32,
    input_min: f32,
    input_max: f32,
    output_min: f32,
    output_max: f32,
    clamp: bool,
    node_id: &str,
    binding: &Binding,
) -> Result<f32, HmiError> {
    if !input_min.is_finite()
        || !input_max.is_finite()
        || !output_min.is_finite()
        || !output_max.is_finite()
        || input_min == input_max
    {
        return Err(invalid(node_id, binding, "Range 边界非法"));
    }
    let input = if clamp {
        input.clamp(input_min.min(input_max), input_min.max(input_max))
    } else {
        input
    };
    let ratio = (input - input_min) / (input_max - input_min);
    let output = output_min + ratio * (output_max - output_min);
    if output.is_finite() {
        Ok(output)
    } else {
        Err(invalid(node_id, binding, "Range 结果不是有限值"))
    }
}

pub fn resolve_binding(
    node_id: &str,
    binding: &Binding,
    tag: &TagDefinition,
    value: &TagValue,
) -> Result<VisualPatch, HmiError> {
    if !value.matches_type(tag.value_type) {
        return Err(invalid(node_id, binding, "运行时值类型与 Tag 定义不一致"));
    }

    let value = match (binding.target, &binding.mapping) {
        (BindingTarget::Text, BindingMapping::Format { precision, unit }) => {
            let precision = precision.unwrap_or(tag.precision);
            let unit = unit.as_deref().unwrap_or(&tag.unit);
            VisualValue::Text(format_tag_value(value, precision, unit))
        }
        (
            BindingTarget::Color,
            BindingMapping::BoolColor {
                true_color,
                false_color,
            },
        ) => {
            let on = boolean(value, node_id, binding)?;
            VisualValue::Color(if on {
                true_color.clone()
            } else {
                false_color.clone()
            })
        }
        (BindingTarget::Color, BindingMapping::ThresholdColor { thresholds }) => {
            let input = numeric(value, node_id, binding)?;
            let Some(first) = thresholds.first() else {
                return Err(invalid(node_id, binding, "ThresholdColor 没有阈值"));
            };
            let selected = thresholds
                .iter()
                .rev()
                .find(|stop| stop.value <= input)
                .unwrap_or(first);
            VisualValue::Color(selected.color.clone())
        }
        (BindingTarget::Visible, BindingMapping::Direct) => match value {
            TagValue::Bool(value) => VisualValue::Visible(*value),
            TagValue::Number(value) => VisualValue::Visible(*value > 0.5),
            TagValue::Enum(_) => return Err(invalid(node_id, binding, "Visible 不支持 Enum")),
        },
        (BindingTarget::Progress, BindingMapping::Direct) => match value {
            TagValue::Bool(value) => VisualValue::Progress(if *value { 1.0 } else { 0.0 }),
            TagValue::Number(value) => VisualValue::Progress(value.clamp(0.0, 1.0)),
            TagValue::Enum(_) => return Err(invalid(node_id, binding, "Progress 不支持 Enum")),
        },
        (
            BindingTarget::Progress,
            BindingMapping::Range {
                input_min,
                input_max,
                output_min,
                output_max,
                clamp,
            },
        ) => {
            let input = numeric(value, node_id, binding)?;
            VisualValue::Progress(
                map_range(
                    input,
                    *input_min,
                    *input_max,
                    *output_min,
                    *output_max,
                    *clamp,
                    node_id,
                    binding,
                )?
                .clamp(0.0, 1.0),
            )
        }
        (BindingTarget::RotationSpeed, BindingMapping::Direct) => match value {
            TagValue::Bool(value) => VisualValue::RotationSpeed(if *value { 1.0 } else { 0.0 }),
            TagValue::Number(value) => VisualValue::RotationSpeed(value.max(0.0)),
            TagValue::Enum(_) => {
                return Err(invalid(node_id, binding, "RotationSpeed 不支持 Enum"));
            }
        },
        (
            BindingTarget::RotationSpeed,
            BindingMapping::Range {
                input_min,
                input_max,
                output_min,
                output_max,
                clamp,
            },
        ) => {
            let input = numeric(value, node_id, binding)?;
            VisualValue::RotationSpeed(
                map_range(
                    input,
                    *input_min,
                    *input_max,
                    *output_min,
                    *output_max,
                    *clamp,
                    node_id,
                    binding,
                )?
                .max(0.0),
            )
        }
        _ => {
            return Err(invalid(node_id, binding, "目标与映射类型不兼容"));
        }
    };

    Ok(VisualPatch {
        node_id: node_id.into(),
        value,
    })
}

pub fn resolve_bindings(
    node_id: &str,
    bindings: &[Binding],
    tags: &BTreeMap<String, TagDefinition>,
    snapshot: &TagSnapshot,
) -> Vec<VisualPatch> {
    bindings
        .iter()
        .filter_map(|binding| {
            let tag = tags.get(&binding.tag_id)?;
            let value = snapshot.get(&binding.tag_id)?;
            resolve_binding(node_id, binding, tag, value).ok()
        })
        .collect()
}
