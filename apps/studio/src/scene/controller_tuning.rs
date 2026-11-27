use super::input::{self, GamepadState, PadButton};
use super::plugin::CachedProject;
use bevy::prelude::*;
use hmi_core::{
    DeviceModel, DeviceNode, EditorCommand, Screen, UiNode, Widget2D, display_axis_to_scene,
    scene_to_display_axes,
};

const COARSE_MULTIPLIER: f32 = 10.0;

#[derive(Resource, Default)]
pub struct ControllerTuningState {
    active: bool,
    index: usize,
    node_id: Option<String>,
    screen_id: Option<String>,
    suppress_shortcuts: bool,
}

impl ControllerTuningState {
    pub fn shortcuts_suppressed(&self) -> bool {
        self.suppress_shortcuts
    }
}

#[derive(Debug, Clone, Copy)]
enum TuningField {
    RectX,
    RectY,
    RectWidth,
    RectHeight,
    RectRotation,
    ZIndex,
    StrokeWidth,
    FontSize,
    BarMin,
    BarMax,
    PipeThickness,
    Position(usize),
    Rotation3d(usize),
    Scale(usize),
    ModelSize(usize),
    DuctLength,
    DuctDiameter,
    ValveSize,
}

#[derive(Debug, Clone)]
struct TuningParam {
    field: TuningField,
    label: &'static str,
    step: f32,
    value: f32,
}

pub fn controller_tuning(
    cached: Res<CachedProject>,
    pad: Res<GamepadState>,
    mut state: ResMut<ControllerTuningState>,
) {
    state.suppress_shortcuts = false;
    if !pad.connected || !input::scene_input_enabled() || crate::bridge::gamepad_menu_open() {
        deactivate(&mut state);
        return;
    }

    let selected_nodes = crate::bridge::selected_nodes();
    let single_selection = selected_nodes.len() == 1;
    let selected = single_selection.then(|| selected_nodes[0].clone());
    let selected_locked = selected.as_deref().and_then(|node_id| {
        let project = cached.project.as_ref()?;
        let screen_id = cached.screen_id.as_ref()?;
        let screen = project.screen(screen_id)?;
        match screen {
            Screen::TwoD { nodes, .. } => nodes
                .iter()
                .find(|node| node.id == node_id)
                .map(|node| node.locked),
            Screen::ThreeD { nodes, .. } => nodes
                .iter()
                .find(|node| node.id == node_id)
                .map(|node| node.locked),
        }
    });

    if pad.just_pressed(PadButton::Touchpad) {
        if state.active {
            deactivate(&mut state);
            return;
        }
        if !single_selection {
            crate::bridge::set_notice(
                crate::bridge::NoticeLevel::Warning,
                "参数模式一次只能调整一个节点，请先单选",
            );
            return;
        }
        match selected_locked {
            Some(false) => {
                state.active = true;
                state.index = 0;
                state.node_id = selected.clone();
                state.screen_id = cached.screen_id.clone();
            }
            Some(true) => crate::bridge::set_notice(
                crate::bridge::NoticeLevel::Warning,
                "节点已锁定，请先用键鼠解锁",
            ),
            None => crate::bridge::set_notice(
                crate::bridge::NoticeLevel::Warning,
                "请先选择一个节点，再按 DualSense 触摸板",
            ),
        }
    }

    if !state.active {
        return;
    }
    if !single_selection {
        deactivate(&mut state);
        return;
    }
    let Some(selected) = selected else {
        deactivate(&mut state);
        return;
    };
    if state.node_id.as_deref() != Some(selected.as_str())
        || state.screen_id.as_deref() != cached.screen_id.as_deref()
    {
        state.index = 0;
        state.node_id = Some(selected.clone());
        state.screen_id = cached.screen_id.clone();
    }
    if selected_locked.is_none_or(|locked| locked) {
        deactivate(&mut state);
        return;
    }
    if pad.just_pressed(PadButton::Circle) {
        deactivate(&mut state);
        state.suppress_shortcuts = true;
        return;
    }

    let Some(project) = cached.project.as_ref() else {
        deactivate(&mut state);
        return;
    };
    let Some(screen_id) = cached.screen_id.as_ref() else {
        deactivate(&mut state);
        return;
    };
    let Some(screen) = project.screen(screen_id) else {
        deactivate(&mut state);
        return;
    };

    match screen {
        Screen::TwoD { nodes, .. } => {
            let Some(node) = nodes.iter().find(|node| node.id == selected) else {
                deactivate(&mut state);
                return;
            };
            let mut params = tuning_params_2d(node);
            if params.is_empty() {
                deactivate(&mut state);
                return;
            }
            state.index = state.index.min(params.len() - 1);
            let mut index_changed = false;
            if pad.just_pressed(PadButton::DpadUp) {
                state.index = (state.index + params.len() - 1) % params.len();
                index_changed = true;
            }
            if pad.just_pressed(PadButton::DpadDown) {
                state.index = (state.index + 1) % params.len();
                index_changed = true;
            }
            let current = &params[state.index];
            let delta = tuning_delta(pad.as_ref(), current.step);
            if delta.abs() > f32::EPSILON {
                let mut updated = node.clone();
                apply_2d(&mut updated, current.field, delta);
                if let Err(error) = crate::bridge::execute(EditorCommand::Update2dNode {
                    screen_id: screen_id.clone(),
                    node: updated.clone(),
                }) {
                    crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error.to_string());
                }
                params = tuning_params_2d(&updated);
            } else if index_changed {
                params = tuning_params_2d(node);
            }
            publish(&params, state.index);
        }
        Screen::ThreeD { nodes, .. } => {
            let Some(node) = nodes.iter().find(|node| node.id == selected) else {
                deactivate(&mut state);
                return;
            };
            let mut params = tuning_params_3d(node);
            if params.is_empty() {
                deactivate(&mut state);
                return;
            }
            state.index = state.index.min(params.len() - 1);
            let mut index_changed = false;
            if pad.just_pressed(PadButton::DpadUp) {
                state.index = (state.index + params.len() - 1) % params.len();
                index_changed = true;
            }
            if pad.just_pressed(PadButton::DpadDown) {
                state.index = (state.index + 1) % params.len();
                index_changed = true;
            }
            let current = &params[state.index];
            let delta = tuning_delta(pad.as_ref(), current.step);
            if delta.abs() > f32::EPSILON {
                let mut updated = node.clone();
                apply_3d(&mut updated, current.field, delta);
                if let Err(error) = crate::bridge::execute(EditorCommand::Update3dNode {
                    screen_id: screen_id.clone(),
                    node: updated.clone(),
                }) {
                    crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error.to_string());
                }
                params = tuning_params_3d(&updated);
            } else if index_changed {
                params = tuning_params_3d(node);
            }
            publish(&params, state.index);
        }
    }
}

fn tuning_delta(pad: &GamepadState, step: f32) -> f32 {
    let mut delta = 0.0;
    if pad.just_pressed(PadButton::DpadLeft) {
        delta -= step;
    }
    if pad.just_pressed(PadButton::DpadRight) {
        delta += step;
    }
    if pad.just_pressed(PadButton::L1) {
        delta -= step * COARSE_MULTIPLIER;
    }
    if pad.just_pressed(PadButton::R1) {
        delta += step * COARSE_MULTIPLIER;
    }
    delta
}

fn publish(params: &[TuningParam], index: usize) {
    let Some(current) = params.get(index) else {
        crate::bridge::set_controller_tuning_hud(None);
        return;
    };
    crate::bridge::set_controller_tuning_hud(Some(crate::bridge::ControllerTuningHud {
        label: current.label.to_string(),
        value: current.value,
        step: current.step,
        index,
        count: params.len(),
    }));
}

fn deactivate(state: &mut ControllerTuningState) {
    state.active = false;
    state.index = 0;
    state.node_id = None;
    state.screen_id = None;
    crate::bridge::set_controller_tuning_hud(None);
}

fn tuning_params_2d(node: &UiNode) -> Vec<TuningParam> {
    let mut params = vec![
        TuningParam {
            field: TuningField::RectX,
            label: "X",
            step: 1.0,
            value: node.rect.x,
        },
        TuningParam {
            field: TuningField::RectY,
            label: "Y",
            step: 1.0,
            value: node.rect.y,
        },
        TuningParam {
            field: TuningField::RectWidth,
            label: "宽度",
            step: 1.0,
            value: node.rect.width,
        },
        TuningParam {
            field: TuningField::RectHeight,
            label: "高度",
            step: 1.0,
            value: node.rect.height,
        },
        TuningParam {
            field: TuningField::RectRotation,
            label: "旋转 °",
            step: 1.0,
            value: node.rotation_deg,
        },
        TuningParam {
            field: TuningField::ZIndex,
            label: "层级 Z",
            step: 1.0,
            value: node.z as f32,
        },
    ];
    match &node.widget {
        Widget2D::Panel { stroke_width, .. } => params.push(TuningParam {
            field: TuningField::StrokeWidth,
            label: "描边宽度",
            step: 0.5,
            value: *stroke_width,
        }),
        Widget2D::Label { font_size, .. } => params.push(TuningParam {
            field: TuningField::FontSize,
            label: "字号",
            step: 1.0,
            value: *font_size,
        }),
        Widget2D::BarGauge { min, max, .. } => {
            params.push(TuningParam {
                field: TuningField::BarMin,
                label: "仪表最小值",
                step: 1.0,
                value: *min,
            });
            params.push(TuningParam {
                field: TuningField::BarMax,
                label: "仪表最大值",
                step: 1.0,
                value: *max,
            });
        }
        Widget2D::Pipe { thickness, .. } => params.push(TuningParam {
            field: TuningField::PipeThickness,
            label: "线宽",
            step: 0.5,
            value: *thickness,
        }),
        Widget2D::ValueCard { .. }
        | Widget2D::StatusLamp { .. }
        | Widget2D::Button { .. }
        | Widget2D::EquipmentSymbol { .. } => {}
    }
    params
}

fn apply_2d(node: &mut UiNode, field: TuningField, delta: f32) {
    match field {
        TuningField::RectX => {
            node.rect.x = (node.rect.x + delta).clamp(0.0, 1600.0 - node.rect.width.min(1600.0));
        }
        TuningField::RectY => {
            node.rect.y = (node.rect.y + delta).clamp(0.0, 900.0 - node.rect.height.min(900.0));
        }
        TuningField::RectWidth => {
            node.rect.width = (node.rect.width + delta).clamp(8.0, 1600.0);
        }
        TuningField::RectHeight => {
            node.rect.height = (node.rect.height + delta).clamp(8.0, 900.0);
        }
        TuningField::RectRotation => {
            node.rotation_deg = (node.rotation_deg + delta).rem_euclid(360.0);
        }
        TuningField::ZIndex => {
            node.z = node.z.saturating_add(delta.round() as i32);
        }
        TuningField::StrokeWidth => {
            if let Widget2D::Panel { stroke_width, .. } = &mut node.widget {
                *stroke_width = (*stroke_width + delta).max(0.0);
            }
        }
        TuningField::FontSize => {
            if let Widget2D::Label { font_size, .. } = &mut node.widget {
                *font_size = (*font_size + delta).max(1.0);
            }
        }
        TuningField::BarMin => {
            if let Widget2D::BarGauge { min, max, .. } = &mut node.widget {
                *min = (*min + delta).min(*max - 0.01);
            }
        }
        TuningField::BarMax => {
            if let Widget2D::BarGauge { min, max, .. } = &mut node.widget {
                *max = (*max + delta).max(*min + 0.01);
            }
        }
        TuningField::PipeThickness => {
            if let Widget2D::Pipe { thickness, .. } = &mut node.widget {
                *thickness = (*thickness + delta).max(0.5);
            }
        }
        TuningField::Position(_)
        | TuningField::Rotation3d(_)
        | TuningField::Scale(_)
        | TuningField::ModelSize(_)
        | TuningField::DuctLength
        | TuningField::DuctDiameter
        | TuningField::ValveSize => {}
    }
}

fn tuning_params_3d(node: &DeviceNode) -> Vec<TuningParam> {
    let axes = ["X", "Y", "Z"];
    let rotations = ["RX", "RY", "RZ"];
    let scales = ["SX", "SY", "SZ"];
    let positions = scene_to_display_axes(node.position);
    let rotation_values = scene_to_display_axes(node.rotation_deg);
    let scale_values = scene_to_display_axes(node.scale);
    let mut params = Vec::new();
    for (axis, label) in axes.into_iter().enumerate() {
        params.push(TuningParam {
            field: TuningField::Position(display_axis_to_scene(axis)),
            label,
            step: 0.1,
            value: positions[axis],
        });
    }
    for (axis, label) in rotations.into_iter().enumerate() {
        params.push(TuningParam {
            field: TuningField::Rotation3d(display_axis_to_scene(axis)),
            label,
            step: 1.0,
            value: rotation_values[axis],
        });
    }
    for (axis, label) in scales.into_iter().enumerate() {
        params.push(TuningParam {
            field: TuningField::Scale(display_axis_to_scene(axis)),
            label,
            step: 0.05,
            value: scale_values[axis],
        });
    }
    match &node.model {
        DeviceModel::Ahu { size, .. }
        | DeviceModel::Fan { size, .. }
        | DeviceModel::Pump { size, .. }
        | DeviceModel::Chiller { size, .. }
        | DeviceModel::CoolingTower { size, .. }
        | DeviceModel::HeatExchanger { size, .. }
        | DeviceModel::ControlCabinet { size, .. } => {
            for (axis, label) in ["宽", "高", "深"].into_iter().enumerate() {
                params.push(TuningParam {
                    field: TuningField::ModelSize(axis),
                    label,
                    step: 0.1,
                    value: size[axis],
                });
            }
        }
        DeviceModel::Duct {
            length, diameter, ..
        }
        | DeviceModel::Pipe {
            length, diameter, ..
        } => {
            params.push(TuningParam {
                field: TuningField::DuctLength,
                label: "模型长度",
                step: 0.1,
                value: *length,
            });
            params.push(TuningParam {
                field: TuningField::DuctDiameter,
                label: "模型直径",
                step: 0.1,
                value: *diameter,
            });
        }
        DeviceModel::Valve { size, .. } => params.push(TuningParam {
            field: TuningField::ValveSize,
            label: "阀体尺寸",
            step: 0.1,
            value: *size,
        }),
        DeviceModel::Glb { .. } => {}
    }
    params
}

fn apply_3d(node: &mut DeviceNode, field: TuningField, delta: f32) {
    match field {
        TuningField::Position(axis) => {
            node.position[axis] = round3(node.position[axis] + delta);
        }
        TuningField::Rotation3d(axis) => {
            node.rotation_deg[axis] = (node.rotation_deg[axis] + delta).rem_euclid(360.0);
        }
        TuningField::Scale(axis) => {
            node.scale[axis] = round3((node.scale[axis] + delta).max(0.01));
        }
        TuningField::ModelSize(axis) => match &mut node.model {
            DeviceModel::Ahu { size, .. }
            | DeviceModel::Fan { size, .. }
            | DeviceModel::Pump { size, .. }
            | DeviceModel::Chiller { size, .. }
            | DeviceModel::CoolingTower { size, .. }
            | DeviceModel::HeatExchanger { size, .. }
            | DeviceModel::ControlCabinet { size, .. } => {
                size[axis] = round3((size[axis] + delta).max(0.01));
            }
            _ => {}
        },
        TuningField::DuctLength => match &mut node.model {
            DeviceModel::Duct { length, .. } | DeviceModel::Pipe { length, .. } => {
                *length = round3((*length + delta).max(0.01));
            }
            _ => {}
        },
        TuningField::DuctDiameter => match &mut node.model {
            DeviceModel::Duct { diameter, .. } | DeviceModel::Pipe { diameter, .. } => {
                *diameter = round3((*diameter + delta).max(0.01));
            }
            _ => {}
        },
        TuningField::ValveSize => {
            if let DeviceModel::Valve { size, .. } = &mut node.model {
                *size = round3((*size + delta).max(0.01));
            }
        }
        TuningField::RectX
        | TuningField::RectY
        | TuningField::RectWidth
        | TuningField::RectHeight
        | TuningField::RectRotation
        | TuningField::ZIndex
        | TuningField::StrokeWidth
        | TuningField::FontSize
        | TuningField::BarMin
        | TuningField::BarMax
        | TuningField::PipeThickness => {}
    }
}

fn round3(value: f32) -> f32 {
    (value * 1_000.0).round() / 1_000.0
}
