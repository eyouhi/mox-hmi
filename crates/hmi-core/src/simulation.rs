use crate::{
    ProjectDocument, SimulatorKind,
    error::HmiError,
    tags::{TagDefinition, TagSnapshot, TagValue, TagValueType},
};
use std::collections::BTreeMap;

pub const SIMULATION_TICK_MS: u64 = 250;
pub const SINE_PERIOD_MS: f32 = 12_000.0;

#[derive(Debug, Clone)]
struct SimEntry {
    definition: TagDefinition,
    value: TagValue,
    rng: u64,
    manual_override: bool,
}

#[derive(Debug, Clone)]
pub struct TagSimulation {
    entries: BTreeMap<String, SimEntry>,
    clock_ms: u64,
    accumulator_ms: u64,
    snapshot: TagSnapshot,
}

impl TagSimulation {
    pub fn new(project: &ProjectDocument) -> Self {
        let mut entries = BTreeMap::new();
        let mut values = BTreeMap::new();
        for definition in &project.tags {
            let value = definition.initial.clone();
            values.insert(definition.id.clone(), value.clone());
            entries.insert(
                definition.id.clone(),
                SimEntry {
                    rng: fnv1a(definition.id.as_bytes()),
                    definition: definition.clone(),
                    value,
                    manual_override: false,
                },
            );
        }
        Self {
            entries,
            clock_ms: 0,
            accumulator_ms: 0,
            snapshot: TagSnapshot::new(values, 1),
        }
    }

    pub fn snapshot(&self) -> &TagSnapshot {
        &self.snapshot
    }

    pub fn tick(&mut self, dt_ms: u64) -> bool {
        self.accumulator_ms = self.accumulator_ms.saturating_add(dt_ms);
        let mut changed = false;
        while self.accumulator_ms >= SIMULATION_TICK_MS {
            self.accumulator_ms -= SIMULATION_TICK_MS;
            self.clock_ms = self.clock_ms.saturating_add(SIMULATION_TICK_MS);
            for entry in self.entries.values_mut() {
                if entry.manual_override {
                    continue;
                }
                let next = next_value(entry, self.clock_ms);
                if next != entry.value {
                    entry.value = next;
                    changed |= self
                        .snapshot
                        .set(entry.definition.id.clone(), entry.value.clone());
                }
            }
        }
        changed
    }

    pub fn write(&mut self, tag_id: &str, value: TagValue) -> Result<(), HmiError> {
        let entry = self
            .entries
            .get_mut(tag_id)
            .ok_or_else(|| HmiError::UnknownTag(tag_id.into()))?;
        if !entry.definition.writable {
            return Err(HmiError::TagNotWritable(tag_id.into()));
        }
        if !value.matches_type(entry.definition.value_type) {
            return Err(HmiError::InvalidCommand(format!(
                "Tag `{tag_id}` 的值类型不匹配"
            )));
        }
        if let TagValue::Number(number) = value
            && !number.is_finite()
        {
            return Err(HmiError::InvalidCommand(format!(
                "Tag `{tag_id}` 的值必须是有限数值"
            )));
        }
        entry.value = value.clone();
        entry.manual_override = true;
        self.snapshot.set(tag_id, value);
        Ok(())
    }

    pub fn resume(&mut self, tag_id: &str) -> Result<(), HmiError> {
        let entry = self
            .entries
            .get_mut(tag_id)
            .ok_or_else(|| HmiError::UnknownTag(tag_id.into()))?;
        entry.manual_override = false;
        Ok(())
    }

    pub fn replace_definitions(&mut self, tags: &[TagDefinition]) {
        let old = std::mem::take(&mut self.entries);
        for definition in tags {
            let preserved = old.get(&definition.id);
            let value = match preserved {
                Some(entry) if entry.definition.value_type == definition.value_type => {
                    entry.value.clone()
                }
                _ => definition.initial.clone(),
            };
            self.entries.insert(
                definition.id.clone(),
                SimEntry {
                    definition: definition.clone(),
                    value,
                    rng: preserved
                        .map_or_else(|| fnv1a(definition.id.as_bytes()), |entry| entry.rng),
                    manual_override: preserved.is_some_and(|entry| entry.manual_override),
                },
            );
        }
        self.snapshot = TagSnapshot::new(
            self.entries
                .iter()
                .map(|(id, entry)| (id.clone(), entry.value.clone()))
                .collect(),
            1,
        );
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn next_value(entry: &mut SimEntry, clock_ms: u64) -> TagValue {
    let definition = &entry.definition;
    match definition.simulator {
        SimulatorKind::Constant | SimulatorKind::Manual => entry.value.clone(),
        SimulatorKind::Pulse => TagValue::Bool((clock_ms % 4_000) >= 3_000),
        SimulatorKind::Sine => {
            let min = definition.min.unwrap_or(0.0);
            let max = definition.max.unwrap_or(100.0);
            let center = (min + max) * 0.5;
            let amplitude = (max - min) * 0.5;
            let phase = (entry.rng % 360) as f32 * std::f32::consts::PI / 180.0;
            let angle = clock_ms as f32 / SINE_PERIOD_MS * std::f32::consts::TAU + phase;
            let value = (center + amplitude * angle.sin()).clamp(min, max);
            TagValue::Number(if value.is_finite() { value } else { center })
        }
        SimulatorKind::Step => {
            if clock_ms >= 5_000 {
                match definition.value_type {
                    TagValueType::Number => TagValue::Number(definition.max.unwrap_or(100.0)),
                    TagValueType::Bool => TagValue::Bool(true),
                    TagValueType::Enum => entry.value.clone(),
                }
            } else {
                match definition.value_type {
                    TagValueType::Number => TagValue::Number(definition.min.unwrap_or(0.0)),
                    TagValueType::Bool => TagValue::Bool(false),
                    TagValueType::Enum => entry.value.clone(),
                }
            }
        }
        SimulatorKind::RandomWalk => match &entry.value {
            TagValue::Number(current) => {
                let min = definition.min.unwrap_or(0.0);
                let max = definition.max.unwrap_or(100.0);
                entry.rng = entry
                    .rng
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                let unit = ((entry.rng >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0;
                let next = (current + unit * (max - min) * 0.015).clamp(min, max);
                TagValue::Number(if next.is_finite() { next } else { *current })
            }
            _ => entry.value.clone(),
        },
    }
}
