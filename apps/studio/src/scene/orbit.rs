use super::CanvasPinch;
use super::input::{self, GamepadState, PadButton};
use super::plugin::{CachedProject, HmiCameras};
use super::three_d::{HoveredThreeD, THREE_D_ROOM_SIZE};
use super::three_d_edit::ThreeDEditState;
use bevy::input::mouse::{AccumulatedMouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use hmi_core::{DeviceModel, Screen};

const THREE_D_FOCUS_EDGE_INSET: f32 = 4.0;
const THREE_D_FOCUS_MIN_HEIGHT: f32 = 0.5;
const THREE_D_FOCUS_MAX_HEIGHT: f32 = 6.0;

#[derive(Resource, Debug, Clone)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub focus: Vec3,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: -0.65,
            pitch: 0.48,
            distance: 15.0,
            focus: Vec3::new(0.0, 1.4, 0.0),
        }
    }
}

#[derive(Default)]
pub struct CameraSync {
    reset_revision: u32,
    command_revision: u64,
}

impl OrbitCamera {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn apply(&self, transform: &mut Transform) {
        let horizontal = self.distance * self.pitch.cos();
        transform.translation = self.focus
            + Vec3::new(
                horizontal * self.yaw.sin(),
                self.distance * self.pitch.sin(),
                horizontal * self.yaw.cos(),
            );
        transform.look_at(self.focus, Vec3::Y);
    }

    fn clamp_focus(&mut self) {
        let horizontal_limit = THREE_D_ROOM_SIZE * 0.5 - THREE_D_FOCUS_EDGE_INSET;
        self.focus.x = self.focus.x.clamp(-horizontal_limit, horizontal_limit);
        self.focus.y = self
            .focus
            .y
            .clamp(THREE_D_FOCUS_MIN_HEIGHT, THREE_D_FOCUS_MAX_HEIGHT);
        self.focus.z = self.focus.z.clamp(-horizontal_limit, horizontal_limit);
    }
}

fn apply_right_stick_orbit(orbit: &mut OrbitCamera, stick: Vec2, dt: f32) {
    orbit.yaw -= stick.x * 1.8 * dt;
    orbit.pitch = (orbit.pitch - stick.y * 1.25 * dt).clamp(-0.15, 1.25);
}

#[allow(clippy::too_many_arguments)]
pub fn orbit_system(
    cached: Res<CachedProject>,
    cameras: Res<HmiCameras>,
    time: Res<Time>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    pad: Res<GamepadState>,
    hovered: Res<HoveredThreeD>,
    mut edit_state: ResMut<ThreeDEditState>,
    mut wheel_events: MessageReader<MouseWheel>,
    mut pinch_events: MessageReader<CanvasPinch>,
    mut orbit: ResMut<OrbitCamera>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
    mut camera_sync: Local<CameraSync>,
) {
    let current_command = crate::bridge::camera_command_revision();
    let command = if current_command != camera_sync.command_revision {
        camera_sync.command_revision = current_command;
        crate::bridge::camera_command()
    } else {
        None
    };
    if !cached.is_three_d() {
        edit_state.view_orbit = false;
        crate::bridge::set_axis_gizmo_3d(None);
        for event in wheel_events.read() {
            let _ = event;
        }
        for event in pinch_events.read() {
            let _ = event;
        }
        return;
    }

    let pointer_on_canvas = window_query
        .single()
        .ok()
        .and_then(|window| window.cursor_position())
        .is_some_and(input::pointer_over_canvas);
    if pointer_on_canvas && mouse_buttons.just_pressed(MouseButton::Left) {
        edit_state.view_orbit = !input::scene_input_enabled()
            || view_orbit_intent(
                input::alt_pressed(&keyboard),
                hovered.0.as_deref().is_some(),
            );
    }
    if !mouse_buttons.pressed(MouseButton::Left) {
        edit_state.view_orbit = false;
    }

    let current_reset = crate::bridge::camera_reset_revision();
    if current_reset != camera_sync.reset_revision {
        camera_sync.reset_revision = current_reset;
        orbit.reset();
    }

    match command {
        Some(crate::bridge::CameraCommand::View3d { focus }) => {
            orbit.focus = Vec3::from_array(focus);
        }
        Some(crate::bridge::CameraCommand::Focus3d) => frame_selected(&cached, &mut orbit),
        _ => {}
    }

    if input::keyboard_input_enabled() && !input::command_modifier_pressed(&keyboard) {
        apply_orbit_keyboard(&cached, &keyboard, time.delta_secs().min(0.05), &mut orbit);
    }
    if input::gamepad_input_enabled() && pad.connected && !crate::bridge::controller_tuning_active()
    {
        apply_orbit_gamepad(&cached, &pad, time.delta_secs().min(0.05), &mut orbit);
    }

    let delta = mouse_motion.delta;
    apply_orbit_mouse_drag(
        &mut orbit,
        &mouse_buttons,
        pointer_on_canvas,
        delta,
        edit_state.view_orbit,
    );

    if let Ok(mut transform) = camera_query.get_mut(cameras.three_d) {
        apply_orbit_middle_pan(
            &mut orbit,
            &mouse_buttons,
            pointer_on_canvas,
            delta,
            &transform,
        );
        apply_orbit_wheel(
            &mut wheel_events,
            pointer_on_canvas,
            &keyboard,
            &mut orbit,
            &transform,
        );
        apply_orbit_pinch(&mut pinch_events, pointer_on_canvas, &mut orbit);
        orbit.clamp_focus();
        orbit.apply(&mut transform);
        publish_axis_gizmo(&transform);
    }
    crate::bridge::set_view_focus_3d(orbit.focus.to_array());
}

fn apply_orbit_keyboard(
    cached: &CachedProject,
    keyboard: &ButtonInput<KeyCode>,
    dt: f32,
    orbit: &mut OrbitCamera,
) {
    if input::alt_pressed(keyboard) {
        if keyboard.pressed(KeyCode::ArrowLeft) {
            orbit.yaw += 1.4 * dt;
        }
        if keyboard.pressed(KeyCode::ArrowRight) {
            orbit.yaw -= 1.4 * dt;
        }
        if keyboard.pressed(KeyCode::ArrowUp) {
            orbit.pitch = (orbit.pitch + 1.0 * dt).clamp(-0.15, 1.25);
        }
        if keyboard.pressed(KeyCode::ArrowDown) {
            orbit.pitch = (orbit.pitch - 1.0 * dt).clamp(-0.15, 1.25);
        }
    } else {
        let mut pan = Vec3::ZERO;
        let forward = Vec3::new(-orbit.yaw.sin(), 0.0, -orbit.yaw.cos());
        let right = Vec3::new(orbit.yaw.cos(), 0.0, -orbit.yaw.sin());
        if keyboard.pressed(KeyCode::KeyW) {
            pan += forward;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            pan -= forward;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            pan += right;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            pan -= right;
        }
        if let Some(direction) = pan.try_normalize() {
            orbit.focus += direction * orbit.distance * 1.2 * dt;
        }
    }
    let mut zoom: f32 = 0.0;
    if keyboard.pressed(KeyCode::Equal) || keyboard.pressed(KeyCode::NumpadAdd) {
        zoom += 1.0;
    }
    if keyboard.pressed(KeyCode::Minus) || keyboard.pressed(KeyCode::NumpadSubtract) {
        zoom -= 1.0;
    }
    if zoom.abs() > f32::EPSILON {
        orbit.distance = (orbit.distance * (-zoom * 1.6 * dt).exp()).clamp(6.0, 24.0);
    }
    if keyboard.just_pressed(KeyCode::Home) {
        orbit.reset();
    }
    if keyboard.just_pressed(KeyCode::KeyF) {
        frame_selected(cached, orbit);
    }
}

fn apply_orbit_gamepad(
    cached: &CachedProject,
    pad: &GamepadState,
    dt: f32,
    orbit: &mut OrbitCamera,
) {
    let stick = pad.right_stick();
    if stick.length_squared() > 0.0 {
        apply_right_stick_orbit(orbit, stick, dt);
    }
    let zoom = pad.right_trigger() - pad.left_trigger();
    if zoom.abs() > 0.01 {
        orbit.distance = (orbit.distance * (-zoom * 1.8 * dt).exp()).clamp(6.0, 24.0);
    }
    if pad.just_pressed(PadButton::Cross) {
        frame_selected(cached, orbit);
    }
}

fn apply_orbit_mouse_drag(
    orbit: &mut OrbitCamera,
    mouse_buttons: &ButtonInput<MouseButton>,
    pointer_on_canvas: bool,
    delta: Vec2,
    view_orbit: bool,
) {
    if pointer_on_canvas
        && (mouse_buttons.pressed(MouseButton::Right)
            || (mouse_buttons.pressed(MouseButton::Left) && view_orbit))
        && delta.length_squared() > 0.0
    {
        orbit.yaw -= delta.x * 0.006;
        orbit.pitch = (orbit.pitch + delta.y * 0.005).clamp(-0.15, 1.25);
    }
}

fn view_orbit_intent(option_pressed: bool, hovered_device: bool) -> bool {
    !option_pressed && !hovered_device
}

fn apply_orbit_middle_pan(
    orbit: &mut OrbitCamera,
    mouse_buttons: &ButtonInput<MouseButton>,
    pointer_on_canvas: bool,
    delta: Vec2,
    transform: &Transform,
) {
    if pointer_on_canvas
        && mouse_buttons.pressed(MouseButton::Middle)
        && delta.length_squared() > 0.0
    {
        let right = transform.right().as_vec3();
        let up = transform.up().as_vec3();
        let distance = orbit.distance;
        orbit.focus -= right * delta.x * distance * 0.0015;
        orbit.focus += up * delta.y * distance * 0.0015;
    }
}

fn apply_orbit_wheel(
    wheel_events: &mut MessageReader<'_, '_, MouseWheel>,
    pointer_on_canvas: bool,
    keyboard: &ButtonInput<KeyCode>,
    orbit: &mut OrbitCamera,
    transform: &Transform,
) {
    for event in wheel_events.read() {
        if !pointer_on_canvas {
            continue;
        }
        if input::wheel_intent(event, keyboard) == input::WheelIntent::Zoom {
            orbit.distance =
                (orbit.distance / input::wheel_zoom_factor(event.y, event.unit)).clamp(6.0, 24.0);
        } else if event.x.abs() > f32::EPSILON || event.y.abs() > f32::EPSILON {
            let right = transform.right().as_vec3();
            let up = transform.up().as_vec3();
            let scale = orbit.distance * 0.0015;
            orbit.focus += wheel_pan_focus_delta(Vec2::new(event.x, event.y), right, up, scale);
        }
    }
}

fn apply_orbit_pinch(
    pinch_events: &mut MessageReader<'_, '_, CanvasPinch>,
    pointer_on_canvas: bool,
    orbit: &mut OrbitCamera,
) {
    if !pointer_on_canvas {
        for event in pinch_events.read() {
            let _ = event;
        }
        return;
    }
    for event in pinch_events.read() {
        if event.delta.abs() <= f32::EPSILON {
            continue;
        }
        apply_orbit_pinch_delta(orbit, event.delta);
    }
}

fn apply_orbit_pinch_delta(orbit: &mut OrbitCamera, delta: f32) {
    orbit.distance = (orbit.distance / input::pinch_zoom_factor(delta)).clamp(6.0, 24.0);
}

fn publish_axis_gizmo(transform: &Transform) {
    let right = transform.right().as_vec3();
    let up = transform.up().as_vec3();
    let project = |axis: Vec3| [axis.dot(right), -axis.dot(up)];
    crate::bridge::set_axis_gizmo_3d(Some(crate::bridge::AxisGizmo3D {
        x: project(Vec3::X),
        y: project(Vec3::Z),
        z: project(Vec3::Y),
    }));
}

fn frame_selected(cached: &CachedProject, orbit: &mut OrbitCamera) {
    let Some(project) = cached.project.as_ref() else {
        return;
    };
    let Some(screen_id) = cached.screen_id.as_ref() else {
        return;
    };
    let Some(Screen::ThreeD { nodes, .. }) = project.screen(screen_id) else {
        return;
    };
    let selected = crate::bridge::selected_nodes();
    let selected_nodes = selected
        .iter()
        .filter_map(|node_id| nodes.iter().find(|node| &node.id == node_id))
        .collect::<Vec<_>>();
    if selected_nodes.is_empty() {
        return;
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut span = 0.0_f32;
    for node in &selected_nodes {
        let position = Vec3::from_array(node.position);
        min = min.min(position);
        max = max.max(position);
        span = span.max(model_span(&node.model));
    }
    let center = (min + max) * 0.5;
    let spread = (max - min).length();
    orbit.focus = center;
    orbit.distance = ((span.max(spread * 1.4)) * 2.2).clamp(6.0, 24.0);
}

fn wheel_pan_focus_delta(delta: Vec2, right: Vec3, up: Vec3, scale: f32) -> Vec3 {
    (up * delta.y - right * delta.x) * scale
}

fn model_span(model: &DeviceModel) -> f32 {
    match model {
        DeviceModel::Ahu { size, .. }
        | DeviceModel::Fan { size, .. }
        | DeviceModel::Pump { size, .. }
        | DeviceModel::Chiller { size, .. }
        | DeviceModel::CoolingTower { size, .. }
        | DeviceModel::HeatExchanger { size, .. }
        | DeviceModel::ControlCabinet { size, .. } => size[0].max(size[1]).max(size[2]),
        DeviceModel::Duct { length, .. } | DeviceModel::Pipe { length, .. } => *length,
        DeviceModel::Valve { size, .. } => *size * 2.0,
        DeviceModel::Glb { .. } => 3.0,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OrbitCamera, THREE_D_FOCUS_MAX_HEIGHT, THREE_D_FOCUS_MIN_HEIGHT, THREE_D_ROOM_SIZE,
        apply_orbit_pinch_delta, apply_right_stick_orbit, view_orbit_intent, wheel_pan_focus_delta,
    };
    use bevy::prelude::{Vec2, Vec3};

    #[test]
    fn right_stick_matches_look_direction() {
        let mut orbit = OrbitCamera {
            yaw: 0.0,
            pitch: 0.4,
            distance: 13.0,
            focus: Vec3::ZERO,
        };
        apply_right_stick_orbit(&mut orbit, Vec2::new(1.0, 0.0), 1.0);
        assert!(orbit.yaw < 0.0);

        let mut orbit = OrbitCamera {
            yaw: 0.0,
            pitch: 0.4,
            distance: 13.0,
            focus: Vec3::ZERO,
        };
        apply_right_stick_orbit(&mut orbit, Vec2::new(0.0, 1.0), 1.0);
        assert!(orbit.pitch < 0.4);
    }

    #[test]
    fn wheel_pan_keeps_content_under_fingers() {
        assert_eq!(
            wheel_pan_focus_delta(Vec2::new(10.0, 5.0), Vec3::X, Vec3::Y, 2.0),
            Vec3::new(-20.0, 10.0, 0.0)
        );
    }

    #[test]
    fn pinch_out_moves_orbit_camera_closer() {
        let mut orbit = OrbitCamera::default();
        let before = orbit.distance;
        apply_orbit_pinch_delta(&mut orbit, 0.1);
        assert!(orbit.distance < before);

        let closer = orbit.distance;
        apply_orbit_pinch_delta(&mut orbit, -0.1);
        assert!(orbit.distance > closer);
    }

    #[test]
    fn view_orbit_only_starts_on_empty_canvas_without_option() {
        assert!(view_orbit_intent(false, false));
        assert!(!view_orbit_intent(true, false));
        assert!(!view_orbit_intent(false, true));
        assert!(!view_orbit_intent(true, true));
    }

    #[test]
    fn orbit_focus_stays_inside_the_room() {
        let mut orbit = OrbitCamera::default();
        orbit.focus = Vec3::new(100.0, 100.0, -100.0);

        orbit.clamp_focus();

        let horizontal_limit = THREE_D_ROOM_SIZE * 0.5 - 4.0;
        assert_eq!(
            orbit.focus,
            Vec3::new(
                horizontal_limit,
                THREE_D_FOCUS_MAX_HEIGHT,
                -horizontal_limit
            )
        );

        orbit.focus = Vec3::new(-100.0, -100.0, 100.0);
        orbit.clamp_focus();

        assert_eq!(
            orbit.focus,
            Vec3::new(
                -horizontal_limit,
                THREE_D_FOCUS_MIN_HEIGHT,
                horizontal_limit
            )
        );
    }
}
