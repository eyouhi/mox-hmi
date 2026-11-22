use super::input::{self, GamepadState, PadButton};
use super::plugin::{CachedProject, HmiCameras};
use super::two_d::{DESIGN_SIZE, HmiNodeRoot};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use hmi_core::{
    CenterSnap, EditorCommand, Rect, ResizeCorner, Screen, UiNode, Widget2D, design_to_world,
    point_in_rotated_rect, point_near_polyline, resize_rect, snap_rect_center, world_to_design,
};

const CENTER_SNAP_SCREEN_PX: f32 = 8.0;
const DRAG_ACTIVATION_SCREEN_PX: f32 = 3.0;
const SNAP_FEEDBACK_COOLDOWN_SECS: f64 = 0.07;
const GAMEPAD_MOVE_SPEED: f32 = 520.0;
const GAMEPAD_PAN_SPEED: f32 = 720.0;

type TwoDCameraQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Camera,
        &'static GlobalTransform,
        &'static mut Transform,
        &'static mut Projection,
    ),
    (With<Camera2d>, Without<HmiNodeRoot>),
>;

type TwoDRootQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static HmiNodeRoot, &'static mut Transform), Without<Camera2d>>;

#[derive(Resource)]
pub struct TwoDCameraState {
    pub zoom: f32,
}

impl Default for TwoDCameraState {
    fn default() -> Self {
        Self { zoom: 1.0 }
    }
}

#[derive(Resource, Default)]
pub struct TwoDEditState {
    drag: Option<DragState>,
    keyboard_snap: (Option<f32>, Option<f32>),
    keyboard_snap_until_s: f64,
}

#[derive(Debug, Clone)]
enum DragState {
    Move {
        start: [f32; 2],
        originals: Vec<(String, Rect)>,
        bounds_original: Rect,
        bounds_preview: Rect,
        activated: bool,
        snap: CenterSnap,
        last_snap: (Option<f32>, Option<f32>),
        last_feedback_s: f64,
        targets: CenterSnapTargets,
    },
    Resize {
        node_id: String,
        corner: ResizeCorner,
        start: [f32; 2],
        original: Rect,
        preview: Rect,
    },
    GamepadMove {
        screen_id: String,
        originals: Vec<(String, Rect)>,
        bounds_original: Rect,
        bounds_preview: Rect,
        snap: CenterSnap,
        last_snap: (Option<f32>, Option<f32>),
        last_feedback_s: f64,
        targets: CenterSnapTargets,
    },
    Marquee {
        start: [f32; 2],
        current: [f32; 2],
        additive: bool,
    },
    Pan {
        last_world: Vec2,
    },
}

#[derive(Debug, Clone, Default)]
struct CenterSnapTargets {
    x: Vec<f32>,
    y: Vec<f32>,
}

impl CenterSnapTargets {
    fn for_nodes(nodes: &[UiNode], moving_ids: &[String]) -> Self {
        let mut x = vec![DESIGN_SIZE[0] as f32 * 0.5];
        let mut y = vec![DESIGN_SIZE[1] as f32 * 0.5];
        for node in nodes
            .iter()
            .filter(|node| node.visible && !moving_ids.contains(&node.id))
        {
            x.push(node.rect.x + node.rect.width * 0.5);
            y.push(node.rect.y + node.rect.height * 0.5);
        }
        Self { x, y }
    }

    fn snap(&self, rect: Rect, zoom: f32) -> CenterSnap {
        let tolerance = CENTER_SNAP_SCREEN_PX / zoom.max(f32::EPSILON);
        snap_rect_center(
            rect,
            &self.x,
            &self.y,
            tolerance,
            Rect {
                x: 0.0,
                y: 0.0,
                width: DESIGN_SIZE[0] as f32,
                height: DESIGN_SIZE[1] as f32,
            },
        )
    }
}

fn union_rect<'a>(rects: impl Iterator<Item = &'a Rect>) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    for rect in rects {
        bounds = Some(match bounds {
            Some(current) => {
                let left = current.x.min(rect.x);
                let top = current.y.min(rect.y);
                let right = current.right().max(rect.right());
                let bottom = current.bottom().max(rect.bottom());
                Rect {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                }
            }
            None => *rect,
        });
    }
    bounds
}

fn rect_intersects(left: Rect, right: Rect) -> bool {
    left.x <= right.right()
        && left.right() >= right.x
        && left.y <= right.bottom()
        && left.bottom() >= right.y
}

fn rotated_bounds(node: &UiNode) -> Rect {
    let corners = rotated_corners(node.rect, node.rotation_deg);
    let left = corners
        .iter()
        .map(|corner| corner[0])
        .fold(f32::INFINITY, f32::min);
    let right = corners
        .iter()
        .map(|corner| corner[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let top = corners
        .iter()
        .map(|corner| corner[1])
        .fold(f32::INFINITY, f32::min);
    let bottom = corners
        .iter()
        .map(|corner| corner[1])
        .fold(f32::NEG_INFINITY, f32::max);
    Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

fn editor_modifier_pressed(keyboard: &ButtonInput<KeyCode>) -> bool {
    input::shift_pressed(keyboard)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight)
        || keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
}

fn selected_unlocked_nodes<'a>(nodes: &'a [UiNode], selected: &[String]) -> Vec<&'a UiNode> {
    selected
        .iter()
        .filter_map(|id| nodes.iter().find(|node| &node.id == id))
        .filter(|node| !node.locked && node.visible)
        .collect()
}

fn node_rects(nodes: &[&UiNode]) -> Vec<(String, Rect)> {
    nodes
        .iter()
        .map(|node| (node.id.clone(), node.rect))
        .collect()
}

fn grouped_previews(
    originals: &[(String, Rect)],
    original_bounds: Rect,
    preview_bounds: Rect,
) -> Vec<(String, Rect)> {
    let delta = [
        preview_bounds.x - original_bounds.x,
        preview_bounds.y - original_bounds.y,
    ];
    originals
        .iter()
        .map(|(id, rect)| {
            (
                id.clone(),
                Rect {
                    x: rect.x + delta[0],
                    y: rect.y + delta[1],
                    ..*rect
                },
            )
        })
        .collect()
}

fn clamped_group_move(bounds: Rect, delta: [f32; 2]) -> Rect {
    let min_dx = -bounds.x;
    let max_dx = (DESIGN_SIZE[0] as f32 - bounds.right()).max(min_dx);
    let min_dy = -bounds.y;
    let max_dy = (DESIGN_SIZE[1] as f32 - bounds.bottom()).max(min_dy);
    Rect {
        x: bounds.x + delta[0].clamp(min_dx, max_dx),
        y: bounds.y + delta[1].clamp(min_dy, max_dy),
        ..bounds
    }
}

fn marquee_rect(start: [f32; 2], current: [f32; 2]) -> Rect {
    Rect {
        x: start[0].min(current[0]),
        y: start[1].min(current[1]),
        width: (current[0] - start[0]).abs(),
        height: (current[1] - start[1]).abs(),
    }
}

fn nodes_intersecting_marquee(nodes: &[UiNode], marquee: Rect) -> Vec<String> {
    nodes
        .iter()
        .filter(|node| node.visible && node.id != "node.bg")
        .filter(|node| rect_intersects(rotated_bounds(node), marquee))
        .map(|node| node.id.clone())
        .collect()
}

fn active_nodes(cached: &CachedProject) -> Option<&[UiNode]> {
    let project = cached.project.as_ref()?;
    let screen_id = cached.screen_id.as_ref()?;
    match project.screen(screen_id)? {
        Screen::TwoD { nodes, .. } => Some(nodes),
        Screen::ThreeD { .. } => None,
    }
}

fn cursor_design(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    cursor: Vec2,
) -> Option<[f32; 2]> {
    let world = camera.viewport_to_world_2d(camera_transform, cursor).ok()?;
    Some(world_to_design([world.x, world.y], DESIGN_SIZE))
}

fn rotated_corners(rect: Rect, rotation_deg: f32) -> [[f32; 2]; 4] {
    let center = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    let angle = rotation_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    [
        [-rect.width * 0.5, -rect.height * 0.5],
        [rect.width * 0.5, -rect.height * 0.5],
        [rect.width * 0.5, rect.height * 0.5],
        [-rect.width * 0.5, rect.height * 0.5],
    ]
    .map(|corner| {
        [
            center[0] + corner[0] * cos - corner[1] * sin,
            center[1] + corner[0] * sin + corner[1] * cos,
        ]
    })
}

fn resize_corner_at(
    rect: Rect,
    rotation_deg: f32,
    point: [f32; 2],
    radius: f32,
) -> Option<ResizeCorner> {
    let corners = rotated_corners(rect, rotation_deg);
    let mut best: Option<(ResizeCorner, f32)> = None;
    for (corner, position) in [
        ResizeCorner::TopLeft,
        ResizeCorner::TopRight,
        ResizeCorner::BottomRight,
        ResizeCorner::BottomLeft,
    ]
    .into_iter()
    .zip(corners)
    {
        let dx = point[0] - position[0];
        let dy = point[1] - position[1];
        let distance = (dx * dx + dy * dy).sqrt();
        if distance <= radius && best.is_none_or(|(_, current)| distance < current) {
            best = Some((corner, distance));
        }
    }
    best.map(|(corner, _)| corner)
}

pub fn hit_test_2d_node(node: &UiNode, point: [f32; 2], zoom: f32) -> bool {
    if node.id == "node.bg" {
        return false;
    }
    match &node.widget {
        Widget2D::Pipe {
            points, thickness, ..
        } => {
            let local = [point[0] - node.rect.x, point[1] - node.rect.y];
            point_near_polyline(local, points, (thickness * 0.5).max(8.0 / zoom))
        }
        _ => point_in_rotated_rect(point, node.rect, node.rotation_deg),
    }
}

fn hit_node(nodes: &[UiNode], point: [f32; 2], zoom: f32) -> Option<&UiNode> {
    let mut candidates: Vec<&UiNode> = nodes
        .iter()
        .filter(|node| node.visible && hit_test_2d_node(node, point, zoom))
        .collect();
    candidates.sort_by_key(|node| std::cmp::Reverse(node.z));
    candidates.into_iter().next()
}

#[allow(clippy::too_many_arguments)]
pub fn two_d_input(
    cached: Res<CachedProject>,
    cameras: Res<HmiCameras>,
    time: Res<Time>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    pad: Res<GamepadState>,
    mut wheel_events: MessageReader<MouseWheel>,
    mut camera_query: TwoDCameraQuery<'_, '_>,
    mut root_query: TwoDRootQuery<'_, '_>,
    mut edit_state: ResMut<TwoDEditState>,
    mut camera_state: ResMut<TwoDCameraState>,
) {
    if !cached.is_two_d() {
        finish_gamepad_move(&cached, &mut edit_state);
        for event in wheel_events.read() {
            let _ = event;
        }
        edit_state.drag = None;
        edit_state.keyboard_snap = (None, None);
        return;
    }
    let Some(nodes) = active_nodes(&cached) else {
        return;
    };
    let Ok(window) = window_query.single() else {
        return;
    };
    let Ok((camera, camera_transform, mut transform, mut projection)) =
        camera_query.get_mut(cameras.two_d)
    else {
        return;
    };
    let cursor = window.cursor_position();
    let pointer_on_canvas = cursor.is_some_and(input::pointer_over_canvas);
    let viewport_center = camera.logical_viewport_size().map(|size| size * 0.5);
    let anchor = cursor.or(viewport_center);
    let Some(point) = anchor.and_then(|anchor| cursor_design(camera, camera_transform, anchor))
    else {
        return;
    };
    let run_mode = crate::bridge::WORKSPACE_MODE() == crate::bridge::WorkspaceMode::Run;

    let pan_requested = (pointer_on_canvas
        || matches!(edit_state.drag, Some(DragState::Pan { .. })))
        && (mouse_buttons.pressed(MouseButton::Middle)
            || (mouse_buttons.pressed(MouseButton::Left) && keyboard.pressed(KeyCode::Space)));
    if pan_requested {
        let current = Vec2::new(point[0], point[1]);
        let last = match edit_state.drag.as_ref() {
            Some(DragState::Pan { last_world }) => *last_world,
            _ => {
                edit_state.drag = Some(DragState::Pan {
                    last_world: current,
                });
                current
            }
        };
        if (current - last).length_squared() > f32::EPSILON {
            let previous_world = design_to_world([last.x, last.y], DESIGN_SIZE);
            let current_world = design_to_world([current.x, current.y], DESIGN_SIZE);
            transform.translation.x -= current_world[0] - previous_world[0];
            transform.translation.y -= current_world[1] - previous_world[1];
            edit_state.drag = Some(DragState::Pan {
                last_world: current,
            });
        }
        return;
    }
    if matches!(edit_state.drag, Some(DragState::Pan { .. })) {
        edit_state.drag = None;
    }

    if pointer_on_canvas && mouse_buttons.just_pressed(MouseButton::Left) {
        let selected = crate::bridge::selected_nodes();
        let additive = editor_modifier_pressed(&keyboard);
        if !run_mode
            && !additive
            && selected.len() == 1
            && let Some(selected_node) = nodes.iter().find(|node| node.id == selected[0])
            && !selected_node.locked
            && let Some(corner) = resize_corner_at(
                selected_node.rect,
                selected_node.rotation_deg,
                point,
                12.0 / camera_state.zoom,
            )
        {
            edit_state.drag = Some(DragState::Resize {
                node_id: selected_node.id.clone(),
                corner,
                start: point,
                original: selected_node.rect,
                preview: selected_node.rect,
            });
        } else if let Some(node) = hit_node(nodes, point, camera_state.zoom) {
            if run_mode {
                crate::bridge::select_node(Some(node.id.clone()));
                if let Widget2D::Button { action, .. } = &node.widget
                    && let Err(error) = crate::bridge::invoke_button_action(action)
                {
                    crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error.to_string());
                }
            } else if additive {
                crate::bridge::toggle_node_selection(&node.id);
            } else {
                if !crate::bridge::is_node_selected(&node.id) {
                    crate::bridge::select_node(Some(node.id.clone()));
                }
                let active_selection = crate::bridge::selected_nodes();
                let moving = selected_unlocked_nodes(nodes, &active_selection);
                if moving.is_empty() && !active_selection.is_empty() {
                    crate::bridge::set_notice(
                        crate::bridge::NoticeLevel::Warning,
                        "所选节点均已锁定，无法移动",
                    );
                }
                if !moving.is_empty()
                    && let Some(bounds_original) = union_rect(moving.iter().map(|node| &node.rect))
                {
                    let originals = node_rects(&moving);
                    let targets = CenterSnapTargets::for_nodes(nodes, &active_selection);
                    super::snap_feedback::prime();
                    edit_state.drag = Some(DragState::Move {
                        start: point,
                        originals,
                        bounds_original,
                        bounds_preview: bounds_original,
                        activated: false,
                        snap: CenterSnap {
                            rect: bounds_original,
                            x: None,
                            y: None,
                        },
                        last_snap: (None, None),
                        last_feedback_s: f64::NEG_INFINITY,
                        targets,
                    });
                }
            }
        } else if run_mode {
            crate::bridge::select_node(None);
        } else {
            if !additive {
                crate::bridge::select_node(None);
            }
            edit_state.drag = Some(DragState::Marquee {
                start: point,
                current: point,
                additive,
            });
        }
    }

    let continuing_drag = matches!(
        edit_state.drag,
        Some(DragState::Move { .. })
            | Some(DragState::Resize { .. })
            | Some(DragState::Marquee { .. })
    );
    if (pointer_on_canvas || continuing_drag) && mouse_buttons.pressed(MouseButton::Left) {
        match edit_state.drag.as_mut() {
            Some(DragState::Move {
                start,
                originals,
                bounds_original,
                bounds_preview,
                activated,
                snap,
                last_snap,
                last_feedback_s,
                targets,
            }) => {
                let delta = [point[0] - start[0], point[1] - start[1]];
                if !*activated {
                    if !drag_activation_reached(delta, camera_state.zoom) {
                        return;
                    }
                    *activated = true;
                }
                let raw = clamped_group_move(*bounds_original, delta);
                *snap = targets.snap(raw, camera_state.zoom);
                *bounds_preview = snap.rect;
                let engaged = snap.x.is_some_and(|target| last_snap.0 != Some(target))
                    || snap.y.is_some_and(|target| last_snap.1 != Some(target));
                if engaged {
                    let now = time.elapsed_secs_f64();
                    if now - *last_feedback_s >= SNAP_FEEDBACK_COOLDOWN_SECS {
                        super::snap_feedback::trigger();
                        *last_feedback_s = now;
                    }
                }
                *last_snap = (snap.x, snap.y);
                for (node_id, rect) in
                    grouped_previews(originals, *bounds_original, *bounds_preview)
                {
                    apply_rect_preview(&mut root_query, &node_id, rect);
                }
            }
            Some(DragState::Resize {
                start,
                original,
                corner,
                preview,
                ..
            }) => {
                *preview = resize_rect(
                    *original,
                    *corner,
                    [point[0] - start[0], point[1] - start[1]],
                    8.0,
                );
            }
            Some(DragState::Marquee { current, .. }) => {
                *current = point;
            }
            _ => {}
        }
    }

    if mouse_buttons.just_released(MouseButton::Left) {
        let drag = edit_state.drag.take();
        match drag {
            Some(DragState::Move {
                originals,
                bounds_preview,
                bounds_original,
                ..
            }) if bounds_preview != bounds_original => {
                commit_rects(
                    &cached,
                    &grouped_previews(&originals, bounds_original, bounds_preview),
                );
            }
            Some(DragState::Resize {
                node_id,
                preview,
                original,
                ..
            }) if preview != original => {
                commit_rect(&cached, &node_id, preview);
            }
            Some(drag @ DragState::GamepadMove { .. }) => {
                edit_state.drag = Some(drag);
            }
            Some(DragState::Marquee {
                start,
                current,
                additive,
            }) if start != current => {
                let marquee = marquee_rect(start, current);
                let hits = nodes_intersecting_marquee(nodes, marquee);
                let ids = if additive {
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
            _ => {}
        }
    }

    for event in wheel_events.read() {
        if !pointer_on_canvas {
            continue;
        }
        let zoom_requested =
            event.unit == MouseScrollUnit::Line || input::zoom_modifier_pressed(&keyboard);
        if zoom_requested {
            if event.y.abs() < f32::EPSILON {
                continue;
            }
            let sensitivity = if event.unit == MouseScrollUnit::Line {
                0.12
            } else {
                0.003
            };
            apply_zoom_2d(
                camera,
                camera_transform,
                &mut transform,
                &mut *projection,
                &mut camera_state,
                cursor,
                (-event.y * sensitivity).exp(),
            );
        } else if event.x.abs() > f32::EPSILON || event.y.abs() > f32::EPSILON {
            let offset = wheel_pan_translation(Vec2::new(event.x, event.y), camera_state.zoom);
            transform.translation.x += offset.x;
            transform.translation.y += offset.y;
        }
    }

    if !run_mode && input::keyboard_input_enabled() && !input::command_modifier_pressed(&keyboard) {
        if keyboard.just_pressed(KeyCode::KeyF) {
            frame_selected_2d(nodes, &mut transform, &mut projection, &mut camera_state);
        }
        let selected = crate::bridge::selected_nodes();
        let unlocked = selected_unlocked_nodes(nodes, &selected);
        if !unlocked.is_empty() {
            let step = if input::shift_pressed(&keyboard) {
                10.0
            } else {
                1.0
            };
            let mut delta = [0.0, 0.0];
            if keyboard.just_pressed(KeyCode::ArrowLeft) {
                delta[0] -= step;
            }
            if keyboard.just_pressed(KeyCode::ArrowRight) {
                delta[0] += step;
            }
            if keyboard.just_pressed(KeyCode::ArrowUp) {
                delta[1] -= step;
            }
            if keyboard.just_pressed(KeyCode::ArrowDown) {
                delta[1] += step;
            }
            let mut updates: Vec<(String, Rect)> = Vec::new();
            if delta != [0.0, 0.0] {
                let Some(bounds) = union_rect(unlocked.iter().map(|node| &node.rect)) else {
                    return;
                };
                let raw = clamped_group_move(bounds, delta);
                let targets = CenterSnapTargets::for_nodes(nodes, &selected);
                let snap = snap_moved_axes(targets.snap(raw, camera_state.zoom), raw, delta);
                let engaged = snap
                    .x
                    .is_some_and(|target| edit_state.keyboard_snap.0 != Some(target))
                    || snap
                        .y
                        .is_some_and(|target| edit_state.keyboard_snap.1 != Some(target));
                if engaged {
                    super::snap_feedback::trigger();
                }
                edit_state.keyboard_snap = (snap.x, snap.y);
                edit_state.keyboard_snap_until_s = time.elapsed_secs_f64() + 0.45;
                updates = grouped_previews(&node_rects(&unlocked), bounds, snap.rect);
            }
            let rotation_step = if input::shift_pressed(&keyboard) {
                15.0
            } else {
                1.0
            };
            let rotation: f32 = if selected.len() == 1 && keyboard.just_pressed(KeyCode::KeyQ) {
                -rotation_step
            } else if selected.len() == 1 && keyboard.just_pressed(KeyCode::KeyE) {
                rotation_step
            } else {
                0.0
            };
            let primary = crate::bridge::selected_node();
            if rotation.abs() > f32::EPSILON
                && let Some(primary_id) = primary.as_ref()
                && let Some(primary_node) = unlocked.iter().find(|node| &node.id == primary_id)
            {
                if let Some((_, rect)) = updates.iter().find(|(id, _)| id == primary_id) {
                    let _ = rect;
                } else {
                    updates.push((primary_node.id.clone(), primary_node.rect));
                }
            }
            if !updates.is_empty() {
                let Some(screen_id) = cached.screen_id.clone() else {
                    return;
                };
                let commands = updates
                    .into_iter()
                    .filter_map(|(id, rect)| {
                        let node = nodes.iter().find(|node| node.id == id)?;
                        let mut updated = node.clone();
                        updated.rect = rect;
                        if rotation.abs() > f32::EPSILON && primary.as_deref() == Some(id.as_str())
                        {
                            updated.rotation_deg =
                                (updated.rotation_deg + rotation).rem_euclid(360.0);
                        }
                        if updated == *node {
                            return None;
                        }
                        Some(EditorCommand::Update2dNode {
                            screen_id: screen_id.clone(),
                            node: updated,
                        })
                    })
                    .collect::<Vec<_>>();
                if let Err(error) = crate::bridge::execute_batch(commands) {
                    crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error.to_string());
                }
            }
        }
    }

    if crate::bridge::controller_tuning_active() {
        finish_gamepad_move(&cached, &mut edit_state);
    } else if !input::gamepad_input_enabled() || !pad.connected {
        finish_gamepad_move(&cached, &mut edit_state);
    } else {
        let dt = time.delta_secs().min(0.05);
        let right = pad.right_stick();
        if right.length_squared() > 0.0 {
            transform.translation.x += right.x * GAMEPAD_PAN_SPEED * dt / camera_state.zoom;
            transform.translation.y -= right.y * GAMEPAD_PAN_SPEED * dt / camera_state.zoom;
        }
        let zoom = pad.right_trigger() - pad.left_trigger();
        if zoom.abs() > 0.01 {
            apply_zoom_2d(
                camera,
                camera_transform,
                &mut transform,
                &mut *projection,
                &mut camera_state,
                anchor,
                (zoom * 1.8 * dt).exp(),
            );
        }
        for (node_id, rect) in gamepad_edit_2d(
            &cached,
            nodes,
            &pad,
            &mut edit_state,
            dt,
            time.elapsed_secs_f64(),
            camera_state.zoom,
        ) {
            apply_rect_preview(&mut root_query, &node_id, rect);
        }
        if pad.just_pressed(PadButton::Cross) {
            frame_selected_2d(nodes, &mut transform, &mut projection, &mut camera_state);
        }
    }

    if (mouse_buttons.just_released(MouseButton::Middle)
        || (mouse_buttons.just_released(MouseButton::Left) && keyboard.pressed(KeyCode::Space)))
        && matches!(edit_state.drag, Some(DragState::Pan { .. }))
    {
        edit_state.drag = None;
    }
}

fn drag_activation_reached(delta: [f32; 2], zoom: f32) -> bool {
    delta[0].hypot(delta[1]) * zoom >= DRAG_ACTIVATION_SCREEN_PX
}

fn wheel_pan_translation(delta: Vec2, zoom: f32) -> Vec2 {
    Vec2::new(-delta.x, delta.y) / zoom.max(f32::EPSILON)
}

fn snap_moved_axes(mut snap: CenterSnap, raw: Rect, delta: [f32; 2]) -> CenterSnap {
    if delta[0].abs() <= f32::EPSILON {
        snap.rect.x = raw.x;
        snap.x = None;
    }
    if delta[1].abs() <= f32::EPSILON {
        snap.rect.y = raw.y;
        snap.y = None;
    }
    snap
}

fn finish_gamepad_move(cached: &CachedProject, edit_state: &mut TwoDEditState) {
    if !matches!(edit_state.drag, Some(DragState::GamepadMove { .. })) {
        return;
    }
    let Some(DragState::GamepadMove {
        screen_id,
        originals,
        bounds_original,
        bounds_preview,
        ..
    }) = edit_state.drag.take()
    else {
        return;
    };
    if bounds_preview != bounds_original {
        commit_rects_for_screen(
            cached,
            &screen_id,
            &grouped_previews(&originals, bounds_original, bounds_preview),
        );
    }
}

fn apply_zoom_2d(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    transform: &mut Transform,
    projection: &mut Projection,
    camera_state: &mut TwoDCameraState,
    anchor: Option<Vec2>,
    factor: f32,
) {
    let old_zoom = camera_state.zoom;
    let new_zoom = (old_zoom * factor).clamp(0.5, 4.0);
    if (new_zoom - old_zoom).abs() <= f32::EPSILON {
        return;
    }
    if let Some(anchor) = anchor
        && let Ok(world) = camera.viewport_to_world_2d(camera_transform, anchor)
    {
        let old_scale = 1.0 / old_zoom;
        let new_scale = 1.0 / new_zoom;
        let center = transform.translation.truncate();
        let anchor = Vec2::new(world.x, world.y);
        transform.translation.x += (anchor.x - center.x) * (1.0 - new_scale / old_scale);
        transform.translation.y += (anchor.y - center.y) * (1.0 - new_scale / old_scale);
    }
    camera_state.zoom = new_zoom;
    if let Projection::Orthographic(orthographic) = projection {
        orthographic.scale = 1.0 / new_zoom;
    }
}

fn frame_selected_2d(
    nodes: &[UiNode],
    transform: &mut Transform,
    projection: &mut Projection,
    camera_state: &mut TwoDCameraState,
) {
    let selected = crate::bridge::selected_nodes();
    let Some(bounds) = union_rect(
        selected
            .iter()
            .filter_map(|id| nodes.iter().find(|node| &node.id == id))
            .map(|node| &node.rect),
    ) else {
        return;
    };
    let target_zoom = (800.0 / bounds.width.max(bounds.height).max(40.0)).clamp(0.5, 4.0);
    camera_state.zoom = target_zoom;
    if let Projection::Orthographic(orthographic) = projection {
        orthographic.scale = 1.0 / target_zoom;
    }
    let center = [
        bounds.x + bounds.width * 0.5,
        bounds.y + bounds.height * 0.5,
    ];
    let world = design_to_world(center, DESIGN_SIZE);
    transform.translation.x = world[0];
    transform.translation.y = world[1];
}

fn apply_rect_preview(root_query: &mut TwoDRootQuery<'_, '_>, node_id: &str, rect: Rect) {
    if let Some((_, _, mut node_transform)) = root_query
        .iter_mut()
        .find(|(_, root, _)| root.node_id == node_id)
    {
        let center = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
        let world = design_to_world(center, DESIGN_SIZE);
        node_transform.translation.x = world[0];
        node_transform.translation.y = world[1];
    }
}

fn gamepad_edit_2d(
    cached: &CachedProject,
    nodes: &[UiNode],
    pad: &GamepadState,
    edit_state: &mut TwoDEditState,
    dt: f32,
    now: f64,
    zoom: f32,
) -> Vec<(String, Rect)> {
    let selected = crate::bridge::selected_nodes();
    let unlocked = selected_unlocked_nodes(nodes, &selected);
    let unlocked_ids = unlocked
        .iter()
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    if let Some(DragState::GamepadMove { originals, .. }) = edit_state.drag.as_ref() {
        let drag_ids = originals
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        if drag_ids != unlocked_ids {
            if let Some(DragState::GamepadMove {
                screen_id,
                originals,
                bounds_original,
                bounds_preview,
                ..
            }) = edit_state.drag.take()
                && bounds_preview != bounds_original
            {
                commit_rects_for_screen(
                    cached,
                    &screen_id,
                    &grouped_previews(&originals, bounds_original, bounds_preview),
                );
            }
        }
    }
    if unlocked.is_empty() {
        return Vec::new();
    }

    let stick = pad.left_stick();
    if stick.length_squared() > 0.0 {
        if !matches!(edit_state.drag, Some(DragState::GamepadMove { .. })) {
            let Some(screen_id) = cached.screen_id.clone() else {
                return Vec::new();
            };
            let Some(bounds_original) = union_rect(unlocked.iter().map(|node| &node.rect)) else {
                return Vec::new();
            };
            let originals = node_rects(&unlocked);
            let targets = CenterSnapTargets::for_nodes(nodes, &selected);
            let snap = targets.snap(bounds_original, zoom);
            super::snap_feedback::prime();
            edit_state.drag = Some(DragState::GamepadMove {
                screen_id,
                originals,
                bounds_original,
                bounds_preview: bounds_original,
                snap,
                last_snap: (snap.x, snap.y),
                last_feedback_s: f64::NEG_INFINITY,
                targets,
            });
        }
        if let Some(DragState::GamepadMove {
            originals,
            bounds_original,
            bounds_preview,
            snap,
            last_snap,
            last_feedback_s,
            targets,
            ..
        }) = edit_state.drag.as_mut()
        {
            let speed = GAMEPAD_MOVE_SPEED * dt / zoom.max(f32::EPSILON);
            let raw = clamped_group_move(*bounds_preview, [stick.x * speed, -stick.y * speed]);
            *snap = targets.snap(raw, zoom);
            *bounds_preview = snap.rect;
            let engaged = snap.x.is_some_and(|target| last_snap.0 != Some(target))
                || snap.y.is_some_and(|target| last_snap.1 != Some(target));
            if engaged && now - *last_feedback_s >= SNAP_FEEDBACK_COOLDOWN_SECS {
                super::snap_feedback::trigger();
                *last_feedback_s = now;
            }
            *last_snap = (snap.x, snap.y);
            return grouped_previews(originals, *bounds_original, *bounds_preview);
        }
    }

    if matches!(edit_state.drag, Some(DragState::GamepadMove { .. })) {
        if let Some(DragState::GamepadMove {
            screen_id,
            originals,
            bounds_original,
            bounds_preview,
            ..
        }) = edit_state.drag.take()
            && bounds_preview != bounds_original
        {
            commit_rects_for_screen(
                cached,
                &screen_id,
                &grouped_previews(&originals, bounds_original, bounds_preview),
            );
        }
        return Vec::new();
    }

    let mut delta = [0.0, 0.0];
    if pad.just_pressed(PadButton::DpadLeft) {
        delta[0] -= 2.0;
    }
    if pad.just_pressed(PadButton::DpadRight) {
        delta[0] += 2.0;
    }
    if pad.just_pressed(PadButton::DpadUp) {
        delta[1] -= 2.0;
    }
    if pad.just_pressed(PadButton::DpadDown) {
        delta[1] += 2.0;
    }
    if delta != [0.0, 0.0] {
        return nudge_2d(cached, nodes, &selected, delta, zoom, edit_state, now);
    }
    Vec::new()
}

fn nudge_2d(
    cached: &CachedProject,
    nodes: &[UiNode],
    selected: &[String],
    delta: [f32; 2],
    zoom: f32,
    edit_state: &mut TwoDEditState,
    now: f64,
) -> Vec<(String, Rect)> {
    let unlocked = selected_unlocked_nodes(nodes, selected);
    let Some(bounds) = union_rect(unlocked.iter().map(|node| &node.rect)) else {
        return Vec::new();
    };
    let raw = clamped_group_move(bounds, delta);
    let targets = CenterSnapTargets::for_nodes(nodes, selected);
    let snap = targets.snap(raw, zoom);
    let engaged = snap
        .x
        .is_some_and(|target| edit_state.keyboard_snap.0 != Some(target))
        || snap
            .y
            .is_some_and(|target| edit_state.keyboard_snap.1 != Some(target));
    if engaged {
        let _ = now;
        super::snap_feedback::trigger();
    }
    edit_state.keyboard_snap = (snap.x, snap.y);
    edit_state.keyboard_snap_until_s = now + 0.45;
    let previews = grouped_previews(&node_rects(&unlocked), bounds, snap.rect);
    commit_rects(cached, &previews);
    previews
}

fn commit_rect(cached: &CachedProject, node_id: &str, rect: Rect) {
    commit_rects(cached, &[(node_id.to_string(), rect)]);
}

fn commit_rects(cached: &CachedProject, rects: &[(String, Rect)]) {
    let Some(screen_id) = cached.screen_id.as_ref() else {
        return;
    };
    commit_rects_for_screen(cached, screen_id, rects);
}

fn commit_rects_for_screen(cached: &CachedProject, screen_id: &str, rects: &[(String, Rect)]) {
    let Some(project) = cached.project.as_ref() else {
        return;
    };
    let Some(Screen::TwoD { nodes, .. }) = project.screen(screen_id) else {
        return;
    };
    let commands = rects
        .iter()
        .filter_map(|(node_id, rect)| {
            let node = nodes.iter().find(|node| &node.id == node_id)?;
            if node.rect == *rect {
                return None;
            }
            let mut updated = node.clone();
            updated.rect = *rect;
            Some(EditorCommand::Update2dNode {
                screen_id: screen_id.to_string(),
                node: updated,
            })
        })
        .collect::<Vec<_>>();
    if let Err(error) = crate::bridge::execute_batch(commands) {
        crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error.to_string());
    }
}

pub fn draw_two_d_overlay(
    cached: Res<CachedProject>,
    edit_state: Res<TwoDEditState>,
    camera_state: Res<TwoDCameraState>,
    time: Res<Time>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut gizmos: Gizmos,
) {
    if !cached.is_two_d() || crate::bridge::WORKSPACE_MODE() == crate::bridge::WorkspaceMode::Run {
        return;
    }
    let grid_minor = Color::srgba(0.12, 0.25, 0.29, 0.18);
    let grid_major = Color::srgba(0.18, 0.38, 0.42, 0.25);
    for x in (0..=1600).step_by(50) {
        let world = design_to_world([x as f32, 0.0], DESIGN_SIZE);
        let bottom = design_to_world([x as f32, 900.0], DESIGN_SIZE);
        let color = if x % 200 == 0 { grid_major } else { grid_minor };
        gizmos.line_2d(
            Vec2::new(world[0], world[1]),
            Vec2::new(bottom[0], bottom[1]),
            color,
        );
    }
    for y in (0..=900).step_by(50) {
        let world = design_to_world([0.0, y as f32], DESIGN_SIZE);
        let right = design_to_world([1600.0, y as f32], DESIGN_SIZE);
        let color = if y % 200 == 0 { grid_major } else { grid_minor };
        gizmos.line_2d(
            Vec2::new(world[0], world[1]),
            Vec2::new(right[0], right[1]),
            color,
        );
    }

    let Some(nodes) = active_nodes(&cached) else {
        return;
    };
    let selected = crate::bridge::selected_nodes();
    if !matches!(
        edit_state.drag,
        Some(DragState::Move { .. })
            | Some(DragState::Resize { .. })
            | Some(DragState::Marquee { .. })
            | Some(DragState::GamepadMove { .. })
    ) && let Ok(window) = window_query.single()
        && let Some(cursor) = window.cursor_position()
        && input::pointer_over_canvas(cursor)
        && let Ok((camera, camera_transform)) = camera_query.single()
        && let Some(point) = cursor_design(camera, camera_transform, cursor)
        && let Some(hovered) = hit_node(nodes, point, camera_state.zoom)
        && !selected.contains(&hovered.id)
    {
        draw_hover_glow(&mut gizmos, hovered, camera_state.zoom);
    }
    let selected_nodes = selected
        .iter()
        .filter_map(|id| nodes.iter().find(|node| &node.id == id))
        .collect::<Vec<_>>();
    let previews = match &edit_state.drag {
        Some(DragState::Move {
            originals,
            bounds_original,
            bounds_preview,
            ..
        }) => grouped_previews(originals, *bounds_original, *bounds_preview),
        Some(DragState::GamepadMove {
            originals,
            bounds_original,
            bounds_preview,
            ..
        }) => grouped_previews(originals, *bounds_original, *bounds_preview),
        Some(DragState::Resize {
            node_id, preview, ..
        }) => vec![(node_id.clone(), *preview)],
        _ => selected_nodes
            .iter()
            .map(|node| (node.id.clone(), node.rect))
            .collect(),
    };
    let preview_rects = previews.iter().map(|(_, rect)| rect).collect::<Vec<_>>();
    let preview_bounds = union_rect(preview_rects.into_iter());
    let selection_center = preview_bounds.map(|rect| {
        design_to_world(
            [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5],
            DESIGN_SIZE,
        )
    });
    for node in &selected_nodes {
        let rect = previews
            .iter()
            .find(|(id, _)| id == &node.id)
            .map(|(_, rect)| *rect)
            .unwrap_or(node.rect);
        let center = design_to_world(
            [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5],
            DESIGN_SIZE,
        );
        let rotation = Rot2::radians(-node.rotation_deg.to_radians());
        let primary = crate::bridge::selected_node().as_deref() == Some(node.id.as_str());
        gizmos.rect_2d(
            Isometry2d::new(Vec2::new(center[0], center[1]), rotation),
            Vec2::new(rect.width, rect.height),
            if primary {
                Color::srgb(0.22, 0.9, 0.83)
            } else {
                Color::srgba(0.22, 0.9, 0.83, 0.62)
            },
        );
    }
    if selected_nodes.len() == 1 {
        let node = selected_nodes[0];
        let rect = previews
            .iter()
            .find(|(id, _)| id == &node.id)
            .map(|(_, rect)| *rect)
            .unwrap_or(node.rect);
        for corner in rotated_corners(rect, node.rotation_deg) {
            let world = design_to_world(corner, DESIGN_SIZE);
            gizmos.circle_2d(
                Vec2::new(world[0], world[1]),
                5.0 / camera_state.zoom,
                Color::srgb(0.8, 1.0, 0.97),
            );
        }
    } else if let Some(bounds) = preview_bounds {
        let center = design_to_world(
            [
                bounds.x + bounds.width * 0.5,
                bounds.y + bounds.height * 0.5,
            ],
            DESIGN_SIZE,
        );
        gizmos.rect_2d(
            Isometry2d::new(Vec2::new(center[0], center[1]), Rot2::IDENTITY),
            Vec2::new(bounds.width, bounds.height),
            Color::srgba(0.22, 0.9, 0.83, 0.32),
        );
    }

    let snap = match &edit_state.drag {
        Some(DragState::Move { snap, .. }) | Some(DragState::GamepadMove { snap, .. }) => {
            Some(*snap)
        }
        _ if time.elapsed_secs_f64() <= edit_state.keyboard_snap_until_s => preview_bounds
            .filter(|_| {
                edit_state.keyboard_snap.0.is_some() || edit_state.keyboard_snap.1.is_some()
            })
            .map(|rect| CenterSnap {
                rect,
                x: edit_state.keyboard_snap.0,
                y: edit_state.keyboard_snap.1,
            }),
        _ => None,
    };

    if let Some(snap) = snap {
        let guide = Color::srgba(0.95, 0.72, 0.29, 0.92);
        if let Some(x) = snap.x {
            let top = design_to_world([x, 0.0], DESIGN_SIZE);
            let bottom = design_to_world([x, DESIGN_SIZE[1] as f32], DESIGN_SIZE);
            gizmos.line_2d(
                Vec2::new(top[0], top[1]),
                Vec2::new(bottom[0], bottom[1]),
                guide,
            );
        }
        if let Some(y) = snap.y {
            let left = design_to_world([0.0, y], DESIGN_SIZE);
            let right = design_to_world([DESIGN_SIZE[0] as f32, y], DESIGN_SIZE);
            gizmos.line_2d(
                Vec2::new(left[0], left[1]),
                Vec2::new(right[0], right[1]),
                guide,
            );
        }
        if (snap.x.is_some() || snap.y.is_some())
            && let Some(center) = selection_center
        {
            gizmos.circle_2d(
                Vec2::new(center[0], center[1]),
                4.0 / camera_state.zoom,
                guide,
            );
        }
    }

    if let Some(DragState::Marquee { start, current, .. }) = &edit_state.drag {
        let marquee = marquee_rect(*start, *current);
        let corners = [
            design_to_world([marquee.x, marquee.y], DESIGN_SIZE),
            design_to_world([marquee.right(), marquee.y], DESIGN_SIZE),
            design_to_world([marquee.right(), marquee.bottom()], DESIGN_SIZE),
            design_to_world([marquee.x, marquee.bottom()], DESIGN_SIZE),
        ];
        let color = Color::srgba(0.35, 0.88, 0.98, 0.95);
        for index in 0..4 {
            let from = corners[index];
            let to = corners[(index + 1) % 4];
            gizmos.line_2d(Vec2::new(from[0], from[1]), Vec2::new(to[0], to[1]), color);
        }
    }
}

fn draw_hover_glow(gizmos: &mut Gizmos, node: &UiNode, zoom: f32) {
    let center = [
        node.rect.x + node.rect.width * 0.5,
        node.rect.y + node.rect.height * 0.5,
    ];
    let world = design_to_world(center, DESIGN_SIZE);
    let rotation = Rot2::radians(-node.rotation_deg.to_radians());
    for (expand_px, alpha) in [(6.0, 0.06), (3.0, 0.13), (0.0, 0.34)] {
        let expand = expand_px / zoom.max(f32::EPSILON);
        gizmos.rect_2d(
            Isometry2d::new(Vec2::new(world[0], world[1]), rotation),
            Vec2::new(
                node.rect.width + expand * 2.0,
                node.rect.height + expand * 2.0,
            ),
            Color::srgba(0.35, 0.88, 0.98, alpha),
        );
    }
}

fn view_center_design(camera: &Camera, transform: &GlobalTransform) -> Option<[f32; 2]> {
    let viewport_center = camera.logical_viewport_size()? * 0.5;
    let world = camera
        .viewport_to_world_2d(transform, viewport_center)
        .ok()?;
    let design = world_to_design([world.x, world.y], DESIGN_SIZE);
    Some([design[0].clamp(0.0, 1600.0), design[1].clamp(0.0, 900.0)])
}

pub fn publish_view_state(
    two_d_cameras: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    camera_state: Res<TwoDCameraState>,
) {
    if let Ok((camera, camera_transform)) = two_d_cameras.single()
        && let Some(center) = view_center_design(camera, camera_transform)
    {
        crate::bridge::set_view_center_2d(center);
    }
    crate::bridge::set_zoom_2d(camera_state.zoom);
}

#[cfg(test)]
mod tests {
    use super::{drag_activation_reached, snap_moved_axes, wheel_pan_translation};
    use bevy::prelude::Vec2;
    use hmi_core::{CenterSnap, Rect};

    #[test]
    fn drag_activation_ignores_press_jitter() {
        assert!(!drag_activation_reached([1.0, 1.0], 1.0));
    }

    #[test]
    fn drag_activation_uses_screen_distance() {
        assert!(drag_activation_reached([3.0, 0.0], 1.0));
        assert!(!drag_activation_reached([3.0, 0.0], 0.5));
        assert!(drag_activation_reached([6.0, 0.0], 0.5));
    }

    #[test]
    fn wheel_pan_keeps_content_under_fingers() {
        assert_eq!(
            wheel_pan_translation(Vec2::new(10.0, 5.0), 2.0),
            Vec2::new(-5.0, 2.5)
        );
    }

    #[test]
    fn keyboard_snap_keeps_unmoved_axis() {
        let raw = Rect {
            x: 651.0,
            y: 250.0,
            width: 260.0,
            height: 180.0,
        };
        let snap = CenterSnap {
            rect: Rect {
                x: 670.0,
                y: 252.0,
                width: 260.0,
                height: 180.0,
            },
            x: Some(800.0),
            y: Some(342.0),
        };
        let snapped = snap_moved_axes(snap, raw, [1.0, 0.0]);
        assert_eq!(snapped.rect.y, 250.0);
        assert_eq!(snapped.y, None);
        assert_eq!(snapped.rect.x, 670.0);
    }
}
