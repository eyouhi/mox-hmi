use super::input::{self, GamepadState, PadButton};
use super::plugin::{CachedProject, HmiCameras};
use super::three_d::{HoveredThreeD, ThreeDVisualIndex};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use hmi_core::{DeviceModel, DeviceNode, EditorCommand, Screen};

const NUDGE_STEP: f32 = 0.1;
const NUDGE_STEP_LARGE: f32 = 1.0;
const STICK_SPEED: f32 = 2.4;
const STICK_SPEED_VERTICAL: f32 = 1.2;
const MARQUEE_DRAG_THRESHOLD: f32 = 6.0;

#[derive(Resource, Default)]
pub struct ThreeDEditState {
    drag: Option<PreviewMove>,
    trackpad: Option<TrackpadDrag>,
    marquee: Option<MarqueeDrag>,
}

#[derive(Debug, Clone)]
struct PreviewMove {
    screen_id: String,
    originals: Vec<DeviceNode>,
    previews: Vec<DeviceNode>,
}

impl PreviewMove {
    fn new(screen_id: String, nodes: &[&DeviceNode]) -> Option<Self> {
        if nodes.is_empty() {
            return None;
        }
        let originals = nodes.iter().map(|node| (*node).clone()).collect::<Vec<_>>();
        Some(Self {
            screen_id,
            previews: originals.clone(),
            originals,
        })
    }

    fn ids(&self) -> Vec<String> {
        self.originals.iter().map(|node| node.id.clone()).collect()
    }

    fn translate(&mut self, delta: Vec3) {
        for (original, preview) in self.originals.iter().zip(self.previews.iter_mut()) {
            preview.position = (Vec3::from_array(original.position) + delta).to_array();
        }
    }

    fn translate_preview(&mut self, delta: Vec3) {
        for preview in &mut self.previews {
            preview.position = (Vec3::from_array(preview.position) + delta).to_array();
        }
    }

    fn changed(&self) -> bool {
        self.originals
            .iter()
            .zip(&self.previews)
            .any(|(original, preview)| original.position != preview.position)
    }
}

#[derive(Debug, Clone)]
struct TrackpadDrag {
    preview: PreviewMove,
    start_ground: Vec3,
    plane_y: f32,
}

#[derive(Debug, Clone, Copy)]
struct MarqueeDrag {
    start: Vec2,
    current: Vec2,
    additive: bool,
}

#[derive(Debug, Clone, Copy)]
struct ScreenRect {
    min: Vec2,
    max: Vec2,
}

impl ScreenRect {
    fn intersects(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
    }
}

#[allow(clippy::too_many_arguments)]
pub fn three_d_edit(
    cached: Res<CachedProject>,
    pad: Res<GamepadState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    visual_index: Res<ThreeDVisualIndex>,
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    mut transforms: Query<&mut Transform>,
    mut state: ResMut<ThreeDEditState>,
) {
    if !cached.is_three_d() {
        commit_preview(state.drag.as_ref());
        state.drag = None;
        return;
    }
    let Some(nodes) = active_nodes(&cached) else {
        return;
    };
    let selected = crate::bridge::selected_nodes();
    if let Some(drag) = state.drag.as_ref()
        && drag.ids() != unlocked_selection_ids(nodes, &selected)
    {
        commit_preview(Some(drag));
        state.drag = None;
    }
    if state.trackpad.is_some() {
        return;
    }
    let unlocked = unlocked_selected_nodes(nodes, &selected);
    if unlocked.is_empty() {
        commit_preview(state.drag.as_ref());
        state.drag = None;
        return;
    }

    if input::keyboard_input_enabled() && !input::command_modifier_pressed(&keyboard) {
        let shift = input::shift_pressed(&keyboard);
        let step = if shift { NUDGE_STEP_LARGE } else { NUDGE_STEP };
        let mut delta = Vec3::ZERO;
        if keyboard.just_pressed(KeyCode::KeyA) {
            delta.x -= step;
        }
        if keyboard.just_pressed(KeyCode::KeyD) {
            delta.x += step;
        }
        if keyboard.just_pressed(KeyCode::KeyW) {
            delta.z -= step;
        }
        if keyboard.just_pressed(KeyCode::KeyS) {
            delta.z += step;
        }
        if keyboard.just_pressed(KeyCode::KeyQ) {
            delta.y -= step;
        }
        if keyboard.just_pressed(KeyCode::KeyE) {
            delta.y += step;
        }
        if delta != Vec3::ZERO {
            commit_preview(state.drag.as_ref());
            state.drag = None;
            let Some(screen_id) = cached.screen_id.clone() else {
                return;
            };
            let commands = unlocked
                .iter()
                .map(|node| {
                    let mut updated = (*node).clone();
                    updated.position = (Vec3::from_array(updated.position) + delta).to_array();
                    EditorCommand::Update3dNode {
                        screen_id: screen_id.clone(),
                        node: updated,
                    }
                })
                .collect();
            if let Err(error) = crate::bridge::execute_batch(commands) {
                crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error.to_string());
            }
            return;
        }
    }

    if crate::bridge::controller_tuning_active() {
        commit_preview(state.drag.as_ref());
        state.drag = None;
        return;
    }

    if !input::gamepad_input_enabled() || !pad.connected {
        commit_preview(state.drag.as_ref());
        state.drag = None;
        return;
    }

    let stick = pad.left_stick();
    let vertical_mode = pad.pressed(PadButton::L3);
    if stick.length_squared() > 0.0 {
        let (right, forward) = camera_query
            .single()
            .map(camera_ground_basis)
            .unwrap_or((Vec3::X, -Vec3::Z));
        if state.drag.is_none() {
            let Some(screen_id) = cached.screen_id.clone() else {
                return;
            };
            state.drag = PreviewMove::new(screen_id, &unlocked);
        }
        if let Some(drag) = state.drag.as_mut() {
            let dt = time.delta_secs().min(0.05);
            let speed = if vertical_mode {
                STICK_SPEED_VERTICAL
            } else {
                STICK_SPEED
            };
            let delta = gamepad_translation(right, forward, stick, vertical_mode, speed, dt);
            drag.translate_preview(delta);
            apply_preview_set(&visual_index, &mut transforms, &drag.previews);
        }
        return;
    }

    if state.drag.is_some() {
        commit_preview(state.drag.as_ref());
        state.drag = None;
        return;
    }

    let mut dpad = Vec3::ZERO;
    if pad.just_pressed(PadButton::DpadLeft) {
        dpad.x -= NUDGE_STEP;
    }
    if pad.just_pressed(PadButton::DpadRight) {
        dpad.x += NUDGE_STEP;
    }
    if vertical_mode {
        if pad.just_pressed(PadButton::DpadUp) {
            dpad.y += NUDGE_STEP;
        }
        if pad.just_pressed(PadButton::DpadDown) {
            dpad.y -= NUDGE_STEP;
        }
    } else {
        if pad.just_pressed(PadButton::DpadUp) {
            dpad.z -= NUDGE_STEP;
        }
        if pad.just_pressed(PadButton::DpadDown) {
            dpad.z += NUDGE_STEP;
        }
    }
    if dpad != Vec3::ZERO {
        let Some(screen_id) = cached.screen_id.clone() else {
            return;
        };
        let commands = unlocked
            .iter()
            .map(|node| {
                let mut updated = (*node).clone();
                updated.position = (Vec3::from_array(updated.position) + dpad).to_array();
                EditorCommand::Update3dNode {
                    screen_id: screen_id.clone(),
                    node: updated,
                }
            })
            .collect();
        if let Err(error) = crate::bridge::execute_batch(commands) {
            crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error.to_string());
        }
    }
}

fn camera_ground_basis(camera: &GlobalTransform) -> (Vec3, Vec3) {
    let right = camera.right().as_vec3();
    let forward = camera.forward().as_vec3();
    let right = Vec3::new(right.x, 0.0, right.z);
    let forward = Vec3::new(forward.x, 0.0, forward.z);
    (
        right.try_normalize().unwrap_or(Vec3::X),
        forward.try_normalize().unwrap_or(-Vec3::Z),
    )
}

fn gamepad_translation(
    right: Vec3,
    forward: Vec3,
    stick: Vec2,
    vertical: bool,
    speed: f32,
    dt: f32,
) -> Vec3 {
    let mut delta = right * stick.x * speed * dt;
    if vertical {
        delta.y += stick.y * speed * dt;
    } else {
        delta += forward * stick.y * speed * dt;
    }
    delta
}

#[allow(clippy::too_many_arguments)]
pub fn three_d_trackpad_edit(
    cached: Res<CachedProject>,
    cameras: Res<HmiCameras>,
    hovered: Res<HoveredThreeD>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    visual_index: Res<ThreeDVisualIndex>,
    mut transforms: Query<&mut Transform>,
    mut state: ResMut<ThreeDEditState>,
) {
    if !cached.is_three_d() || !input::scene_input_enabled() {
        finish_trackpad(&mut state);
        state.marquee = None;
        return;
    }
    let Ok(window) = window_query.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Some(nodes) = active_nodes(&cached) else {
        return;
    };

    if mouse_buttons.just_released(MouseButton::Left) {
        finish_trackpad(&mut state);
        if let Some(marquee) = state.marquee.take()
            && marquee.start.distance(marquee.current) >= MARQUEE_DRAG_THRESHOLD
            && let Ok((camera, camera_transform)) = camera_query.get(cameras.three_d)
        {
            let screen = marquee_screen_rect(marquee.start, marquee.current);
            let hits = nodes
                .iter()
                .filter(|node| node.visible)
                .filter(|node| {
                    device_screen_bounds(camera, camera_transform, node)
                        .is_some_and(|bounds| bounds.intersects(screen))
                })
                .map(|node| node.id.clone())
                .collect::<Vec<_>>();
            let ids = if marquee.additive {
                let mut ids = crate::bridge::selected_nodes();
                for id in hits {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                ids
            } else {
                hits
            };
            crate::bridge::set_selected_nodes(ids);
        }
        return;
    }
    if !mouse_buttons.pressed(MouseButton::Left) {
        finish_trackpad(&mut state);
        state.marquee = None;
        return;
    }

    if let Some(marquee) = state.marquee.as_mut() {
        marquee.current = cursor;
        return;
    }

    if let Some(drag) = state.trackpad.as_mut() {
        let Ok((camera, camera_transform)) = camera_query.get(cameras.three_d) else {
            finish_trackpad(&mut state);
            return;
        };
        let Some(ground) = ground_point(camera, camera_transform, cursor, drag.plane_y) else {
            return;
        };
        let delta = ground - drag.start_ground;
        let mut preview = drag.preview.clone();
        preview.translate(Vec3::new(delta.x, 0.0, delta.z));
        drag.preview = preview.clone();
        apply_preview_set(&visual_index, &mut transforms, &preview.previews);
        return;
    }

    let pointer_on_canvas = input::pointer_over_canvas(cursor);
    if !mouse_buttons.just_pressed(MouseButton::Left) || !pointer_on_canvas {
        return;
    }
    if keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight) {
        return;
    }

    let additive = input::shift_pressed(&keyboard)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight)
        || keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight);

    if let Some(node_id) = hovered.0.as_deref() {
        if additive {
            return;
        }
        if !crate::bridge::is_node_selected(node_id) {
            crate::bridge::select_node(Some(node_id.to_string()));
        }
        let selected = crate::bridge::selected_nodes();
        let unlocked = unlocked_selected_nodes(nodes, &selected);
        if unlocked.is_empty() {
            crate::bridge::set_notice(
                crate::bridge::NoticeLevel::Warning,
                "所选设备均已锁定，无法移动",
            );
            return;
        }
        let Some(screen_id) = cached.screen_id.clone() else {
            return;
        };
        let Ok((camera, camera_transform)) = camera_query.get(cameras.three_d) else {
            return;
        };
        let plane_y = selected
            .iter()
            .find_map(|id| nodes.iter().find(|node| &node.id == id))
            .map(|node| node.position[1])
            .unwrap_or(0.0);
        let Some(ground) = ground_point(camera, camera_transform, cursor, plane_y) else {
            return;
        };
        commit_preview(state.drag.as_ref());
        state.drag = None;
        state.trackpad = PreviewMove::new(screen_id, &unlocked).map(|preview| TrackpadDrag {
            preview,
            start_ground: ground,
            plane_y,
        });
        return;
    }

    state.marquee = Some(MarqueeDrag {
        start: cursor,
        current: cursor,
        additive,
    });
}

pub fn draw_three_d_marquee(
    state: Res<ThreeDEditState>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut gizmos: Gizmos,
) {
    let Some(marquee) = state.marquee else {
        return;
    };
    if marquee.start.distance(marquee.current) < MARQUEE_DRAG_THRESHOLD {
        return;
    }
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let screen = marquee_screen_rect(marquee.start, marquee.current);
    let points = [
        screen.min,
        Vec2::new(screen.max.x, screen.min.y),
        screen.max,
        Vec2::new(screen.min.x, screen.max.y),
    ]
    .map(|point| project_screen_point(camera, camera_transform, point));
    let Some(points) = points.into_iter().collect::<Option<Vec<_>>>() else {
        return;
    };
    let color = Color::srgba(0.35, 0.88, 0.98, 0.95);
    for index in 0..4 {
        gizmos.line(points[index], points[(index + 1) % 4], color);
    }
}

pub fn draw_three_d_hover(
    cached: Res<CachedProject>,
    hovered: Res<HoveredThreeD>,
    mut gizmos: Gizmos,
) {
    if !cached.is_three_d() || crate::bridge::WORKSPACE_MODE() == crate::bridge::WorkspaceMode::Run
    {
        return;
    }
    let Some(node_id) = hovered.0.as_deref() else {
        return;
    };
    if crate::bridge::is_node_selected(node_id) {
        return;
    }
    let Some(nodes) = active_nodes(&cached) else {
        return;
    };
    let Some(node) = nodes.iter().find(|node| node.id == node_id && node.visible) else {
        return;
    };
    let corners = device_world_corners(node);
    for (scale, alpha) in [(1.06, 0.08), (1.035, 0.15), (1.0, 0.38)] {
        draw_box_edges(
            &mut gizmos,
            &corners,
            scale,
            Color::srgba(0.35, 0.88, 0.98, alpha),
        );
    }
}

fn draw_box_edges(gizmos: &mut Gizmos, corners: &[Vec3; 8], scale: f32, color: Color) {
    let center = corners.iter().copied().sum::<Vec3>() / 8.0;
    let scaled = corners.map(|corner| center + (corner - center) * scale);
    for (from, to) in [
        (0, 1),
        (1, 3),
        (3, 2),
        (2, 0),
        (4, 5),
        (5, 7),
        (7, 6),
        (6, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ] {
        gizmos.line(scaled[from], scaled[to], color);
    }
}

fn device_world_corners(node: &DeviceNode) -> [Vec3; 8] {
    let (center, half) = device_local_bounds(&node.model);
    let rotation = Quat::from_euler(
        EulerRot::XYZ,
        node.rotation_deg[0].to_radians(),
        node.rotation_deg[1].to_radians(),
        node.rotation_deg[2].to_radians(),
    );
    let scale = Vec3::from_array(node.scale).abs();
    let position = Vec3::from_array(node.position);
    [
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
    ]
    .map(|corner| position + rotation * (center + corner * half * scale))
}

fn finish_trackpad(state: &mut ThreeDEditState) {
    if let Some(drag) = state.trackpad.take() {
        commit_preview(Some(&drag.preview));
    }
}

fn active_nodes(cached: &CachedProject) -> Option<&[DeviceNode]> {
    let project = cached.project.as_ref()?;
    let screen_id = cached.screen_id.as_ref()?;
    match project.screen(screen_id)? {
        Screen::ThreeD { nodes, .. } => Some(nodes),
        Screen::TwoD { .. } => None,
    }
}

fn unlocked_selected_nodes<'a>(
    nodes: &'a [DeviceNode],
    selected: &[String],
) -> Vec<&'a DeviceNode> {
    selected
        .iter()
        .filter_map(|id| nodes.iter().find(|node| &node.id == id))
        .filter(|node| !node.locked && node.visible)
        .collect()
}

fn unlocked_selection_ids(nodes: &[DeviceNode], selected: &[String]) -> Vec<String> {
    unlocked_selected_nodes(nodes, selected)
        .iter()
        .map(|node| node.id.clone())
        .collect()
}

fn apply_preview_set(
    visual_index: &ThreeDVisualIndex,
    transforms: &mut Query<&mut Transform>,
    nodes: &[DeviceNode],
) {
    for node in nodes {
        apply_preview(visual_index, transforms, node);
    }
}

fn apply_preview(
    visual_index: &ThreeDVisualIndex,
    transforms: &mut Query<&mut Transform>,
    node: &DeviceNode,
) {
    let Some(visual) = visual_index.nodes.get(&node.id) else {
        return;
    };
    if let Ok(mut transform) = transforms.get_mut(visual.root) {
        transform.translation = Vec3::from_array(node.position);
    }
    if let Ok(mut transform) = transforms.get_mut(visual.selection_ring) {
        transform.translation = Vec3::new(node.position[0], 0.025, node.position[2]);
    }
}

fn commit_preview(drag: Option<&PreviewMove>) {
    let Some(drag) = drag else {
        return;
    };
    if !drag.changed() {
        return;
    }
    let commands = drag
        .originals
        .iter()
        .zip(&drag.previews)
        .filter(|(original, preview)| original.position != preview.position)
        .map(|(_, preview)| {
            let mut node = preview.clone();
            for axis in &mut node.position {
                *axis = (*axis * 1_000.0).round() / 1_000.0;
            }
            EditorCommand::Update3dNode {
                screen_id: drag.screen_id.clone(),
                node,
            }
        })
        .collect();
    if let Err(error) = crate::bridge::execute_batch(commands) {
        crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error.to_string());
    }
}

fn ground_point(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    cursor: Vec2,
    plane_y: f32,
) -> Option<Vec3> {
    let ray = camera.viewport_to_world(camera_transform, cursor).ok()?;
    let direction = ray.direction.as_vec3();
    if direction.y.abs() < 1e-5 {
        return None;
    }
    let distance = (plane_y - ray.origin.y) / direction.y;
    if distance < 0.0 {
        return None;
    }
    Some(ray.origin + direction * distance)
}

fn marquee_screen_rect(start: Vec2, current: Vec2) -> ScreenRect {
    ScreenRect {
        min: start.min(current),
        max: start.max(current),
    }
}

fn project_screen_point(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    point: Vec2,
) -> Option<Vec3> {
    let ray = camera.viewport_to_world(camera_transform, point).ok()?;
    Some(ray.origin + ray.direction.as_vec3() * 2.0)
}

fn device_screen_bounds(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    node: &DeviceNode,
) -> Option<ScreenRect> {
    let (center, half) = device_local_bounds(&node.model);
    let rotation = Quat::from_euler(
        EulerRot::XYZ,
        node.rotation_deg[0].to_radians(),
        node.rotation_deg[1].to_radians(),
        node.rotation_deg[2].to_radians(),
    );
    let scale = Vec3::from_array(node.scale).abs();
    let position = Vec3::from_array(node.position);
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut projected = 0;
    for corner in [
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
    ] {
        let world = position + rotation * (center + corner * half * scale);
        if let Ok(point) = camera.world_to_viewport(camera_transform, world) {
            min = min.min(point);
            max = max.max(point);
            projected += 1;
        }
    }
    (projected > 0).then_some(ScreenRect { min, max })
}

fn device_local_bounds(model: &DeviceModel) -> (Vec3, Vec3) {
    match model {
        DeviceModel::Ahu { size, .. }
        | DeviceModel::Fan { size, .. }
        | DeviceModel::Pump { size, .. } => (Vec3::ZERO, Vec3::from_array(*size) * 0.5),
        DeviceModel::Duct {
            length, diameter, ..
        }
        | DeviceModel::Pipe {
            length, diameter, ..
        } => (
            Vec3::ZERO,
            Vec3::new(*length * 0.5, *diameter * 0.5, *diameter * 0.5),
        ),
        DeviceModel::Valve { size, .. } => (
            Vec3::new(0.0, *size * 0.65, 0.0),
            Vec3::new(*size * 0.5, *size * 0.75, *size * 0.5),
        ),
        DeviceModel::Glb { .. } => (Vec3::new(0.0, 1.0, 0.0), Vec3::splat(1.5)),
    }
}

#[cfg(test)]
mod tests {
    use super::gamepad_translation;
    use bevy::prelude::{Vec2, Vec3};

    #[test]
    fn gamepad_stick_follows_camera_basis() {
        let right = Vec3::X;
        let forward = -Vec3::Z;
        let horizontal = gamepad_translation(right, forward, Vec2::new(0.5, 0.0), false, 2.0, 1.0);
        assert_eq!(horizontal, Vec3::X);
        let depth = gamepad_translation(right, forward, Vec2::new(0.0, 0.5), false, 2.0, 1.0);
        assert_eq!(depth, -Vec3::Z);
    }

    #[test]
    fn vertical_mode_keeps_stick_x_on_ground_and_uses_up_positive_stick_y() {
        let delta = gamepad_translation(Vec3::X, -Vec3::Z, Vec2::new(0.5, 0.5), true, 2.0, 1.0);
        assert_eq!(delta, Vec3::new(1.0, 1.0, 0.0));
    }
}
