use super::CanvasPinch;
use super::delete_fx::DeleteFxOverlayCamera;
use super::input::{self, GamepadState, PadButton};
use super::plugin::{CachedProject, HmiCameras};
use super::two_d::{DESIGN_SIZE, HmiNodeRoot, TwoDVisualIndex};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use hmi_core::{
    CenterSnap, EditorCommand, Rect, ResizeCorner, Screen, UiNode, Widget2D, design_to_world,
    nearest_point_on_rect_boundary, point_in_rotated_rect, point_near_polyline, resize_rect,
    snap_rect_center, world_to_design,
};

const CENTER_SNAP_SCREEN_PX: f32 = 8.0;
const POINT_SNAP_SCREEN_PX: f32 = 10.0;
const PIPE_AXIS_SNAP_DEGREES: f32 = 7.0;
const PIPE_AXIS_SNAP_MIN_SCREEN_PX: f32 = 4.0;
const PIPE_HANDLE_SCREEN_PX: f32 = 9.0;
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
    (
        With<Camera2d>,
        Without<HmiNodeRoot>,
        Without<DeleteFxOverlayCamera>,
    ),
>;

type TwoDRootQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static HmiNodeRoot, &'static mut Transform), Without<Camera2d>>;

type TwoDChildTransformQuery<'w, 's> =
    Query<'w, 's, &'static mut Transform, (Without<HmiNodeRoot>, Without<Camera2d>)>;

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

#[derive(Default)]
pub struct CameraSync {
    reset_revision: u32,
    command_revision: u64,
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
    PipePoint {
        node_id: String,
        index: usize,
        start: [f32; 2],
        original: Box<UiNode>,
        preview: Box<UiNode>,
        activated: bool,
        snap: PointSnap,
        last_snap: PointSnapKey,
        last_feedback_s: f64,
        targets: CenterSnapTargets,
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
        start_world: Vec2,
        clear_on_click: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct PointSnap {
    point: [f32; 2],
    x: Option<f32>,
    y: Option<f32>,
    device_id: Option<String>,
    axis: PipeAxisSnap,
}

impl PointSnap {
    fn key(&self) -> PointSnapKey {
        PointSnapKey {
            center_x: self.x,
            center_y: self.y,
            device_id: self.device_id.clone(),
            axis: self.axis.clone(),
        }
    }

    fn engaged(&self) -> bool {
        self.x.is_some() || self.y.is_some() || self.device_id.is_some() || self.axis.engaged()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct PointSnapKey {
    center_x: Option<f32>,
    center_y: Option<f32>,
    device_id: Option<String>,
    axis: PipeAxisSnap,
}

impl PointSnapKey {
    fn engaged(&self) -> bool {
        self.center_x.is_some()
            || self.center_y.is_some()
            || self.device_id.is_some()
            || self.axis.engaged()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct PipeAxisSnap {
    horizontal_from: Option<[f32; 2]>,
    vertical_from: Option<[f32; 2]>,
}

impl PipeAxisSnap {
    fn engaged(&self) -> bool {
        self.horizontal_from.is_some() || self.vertical_from.is_some()
    }

    fn apply(&self, raw: [f32; 2]) -> [f32; 2] {
        [
            self.vertical_from.map_or(raw[0], |anchor| anchor[0]),
            self.horizontal_from.map_or(raw[1], |anchor| anchor[1]),
        ]
    }
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

fn node_local_to_design(node: &UiNode, local: [f32; 2]) -> [f32; 2] {
    let center = [
        node.rect.x + node.rect.width * 0.5,
        node.rect.y + node.rect.height * 0.5,
    ];
    let offset = [
        local[0] - node.rect.width * 0.5,
        local[1] - node.rect.height * 0.5,
    ];
    let angle = node.rotation_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    [
        center[0] + offset[0] * cos - offset[1] * sin,
        center[1] + offset[0] * sin + offset[1] * cos,
    ]
}

fn design_to_node_local(node: &UiNode, point: [f32; 2]) -> [f32; 2] {
    design_to_rect_local(node.rect, node.rotation_deg, point)
}

fn design_to_rect_local(rect: Rect, rotation_deg: f32, point: [f32; 2]) -> [f32; 2] {
    let center = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    let offset = [point[0] - center[0], point[1] - center[1]];
    let angle = -rotation_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    let local = [
        offset[0] * cos - offset[1] * sin,
        offset[0] * sin + offset[1] * cos,
    ];
    [local[0] + rect.width * 0.5, local[1] + rect.height * 0.5]
}

fn pipe_points(node: &UiNode) -> Option<&[[f32; 2]]> {
    match &node.widget {
        Widget2D::Pipe { points, .. } => Some(points),
        _ => None,
    }
}

fn pipe_handle_at(node: &UiNode, point: [f32; 2], radius: f32) -> Option<usize> {
    let points = pipe_points(node)?;
    points
        .iter()
        .enumerate()
        .filter_map(|(index, local)| {
            let position = node_local_to_design(node, *local);
            let distance = (point[0] - position[0]).hypot(point[1] - position[1]);
            (distance <= radius).then_some((index, distance))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(index, _)| index)
}

fn nearest_axis(value: f32, targets: &[f32], tolerance: f32) -> Option<f32> {
    targets
        .iter()
        .copied()
        .filter(|target| target.is_finite())
        .filter_map(|target| {
            let distance = (target - value).abs();
            (distance <= tolerance).then_some((distance, target))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, target)| target)
}

fn pipe_axis_snap(
    node: &UiNode,
    index: usize,
    raw: [f32; 2],
    min_segment_length: f32,
) -> PipeAxisSnap {
    let Some(points) = pipe_points(node) else {
        return PipeAxisSnap::default();
    };
    let angle_tolerance = PIPE_AXIS_SNAP_DEGREES.to_radians();
    let mut horizontal: Option<(f32, [f32; 2])> = None;
    let mut vertical: Option<(f32, [f32; 2])> = None;
    for neighbor in [
        index.checked_sub(1),
        (index + 1 < points.len()).then_some(index + 1),
    ]
    .into_iter()
    .flatten()
    {
        let anchor = node_local_to_design(node, points[neighbor]);
        let delta = [raw[0] - anchor[0], raw[1] - anchor[1]];
        if delta[0].hypot(delta[1]) < min_segment_length {
            continue;
        }
        let angle = delta[1].atan2(delta[0]).abs();
        let horizontal_deviation = angle.min(std::f32::consts::PI - angle);
        let vertical_deviation = (std::f32::consts::FRAC_PI_2 - angle).abs();
        if horizontal_deviation <= angle_tolerance
            && horizontal.is_none_or(|(current, _)| horizontal_deviation < current)
        {
            horizontal = Some((horizontal_deviation, anchor));
        }
        if vertical_deviation <= angle_tolerance
            && vertical.is_none_or(|(current, _)| vertical_deviation < current)
        {
            vertical = Some((vertical_deviation, anchor));
        }
    }
    PipeAxisSnap {
        horizontal_from: horizontal.map(|(_, anchor)| anchor),
        vertical_from: vertical.map(|(_, anchor)| anchor),
    }
}

#[allow(clippy::too_many_arguments)]
fn snap_pipe_point(
    node: &UiNode,
    index: usize,
    raw: [f32; 2],
    is_endpoint: bool,
    nodes: &[UiNode],
    targets: &CenterSnapTargets,
    zoom: f32,
    allow_axis_snap: bool,
) -> PointSnap {
    let zoom = zoom.max(f32::EPSILON);
    let tolerance = POINT_SNAP_SCREEN_PX / zoom;
    let raw = [
        raw[0].clamp(0.0, DESIGN_SIZE[0] as f32),
        raw[1].clamp(0.0, DESIGN_SIZE[1] as f32),
    ];
    if is_endpoint {
        let mut best: Option<(f32, [f32; 2], String)> = None;
        for device in nodes.iter().filter(|candidate| {
            candidate.visible
                && candidate.id != node.id
                && matches!(candidate.widget, Widget2D::EquipmentSymbol { .. })
        }) {
            let Some(boundary) =
                nearest_point_on_rect_boundary(raw, device.rect, device.rotation_deg)
            else {
                continue;
            };
            let distance = (raw[0] - boundary[0]).hypot(raw[1] - boundary[1]);
            let inside = point_in_rotated_rect(raw, device.rect, device.rotation_deg);
            if (inside || distance <= tolerance)
                && best
                    .as_ref()
                    .is_none_or(|(current, _, _)| distance < *current)
            {
                best = Some((distance, boundary, device.id.clone()));
            }
        }
        if let Some((_, point, device_id)) = best {
            return PointSnap {
                point,
                x: None,
                y: None,
                device_id: Some(device_id),
                axis: PipeAxisSnap::default(),
            };
        }
    }

    let axis = if allow_axis_snap {
        pipe_axis_snap(node, index, raw, PIPE_AXIS_SNAP_MIN_SCREEN_PX / zoom)
    } else {
        PipeAxisSnap::default()
    };
    let mut point = axis.apply(raw);
    let x = if axis.vertical_from.is_some() {
        None
    } else {
        nearest_axis(raw[0], &targets.x, tolerance)
    };
    let y = if axis.horizontal_from.is_some() {
        None
    } else {
        nearest_axis(raw[1], &targets.y, tolerance)
    };
    if let Some(x) = x {
        point[0] = x;
    }
    if let Some(y) = y {
        point[1] = y;
    }
    PointSnap {
        point,
        x,
        y,
        device_id: None,
        axis,
    }
}

fn normalized_pipe_node(mut node: UiNode) -> UiNode {
    let original = node.clone();
    let Widget2D::Pipe {
        points, thickness, ..
    } = &original.widget
    else {
        return node;
    };
    if points.is_empty() {
        return node;
    }
    let pad = (*thickness * 0.5).max(1.0);
    let min_x = points
        .iter()
        .map(|point| point[0])
        .fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|point| point[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = points
        .iter()
        .map(|point| point[1])
        .fold(f32::INFINITY, f32::min);
    let max_y = points
        .iter()
        .map(|point| point[1])
        .fold(f32::NEG_INFINITY, f32::max);
    let offset = [node.rect.x + min_x - pad, node.rect.y + min_y - pad];
    let rect = Rect {
        x: offset[0],
        y: offset[1],
        width: (max_x - min_x + pad * 2.0).max(1.0),
        height: (max_y - min_y + pad * 2.0).max(1.0),
    };
    let points = points
        .iter()
        .map(|point| {
            let design = node_local_to_design(&original, *point);
            design_to_rect_local(rect, original.rotation_deg, design)
        })
        .collect();
    node.rect = rect;
    if let Widget2D::Pipe {
        points: current, ..
    } = &mut node.widget
    {
        *current = points;
    }
    node
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
            let local = design_to_node_local(node, point);
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
    visual_index: Res<TwoDVisualIndex>,
    time: Res<Time>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    pad: Res<GamepadState>,
    mut wheel_events: MessageReader<MouseWheel>,
    mut pinch_events: MessageReader<CanvasPinch>,
    mut camera_query: TwoDCameraQuery<'_, '_>,
    mut root_query: TwoDRootQuery<'_, '_>,
    mut child_transforms: TwoDChildTransformQuery<'_, '_>,
    mut edit_state: ResMut<TwoDEditState>,
    mut camera_state: ResMut<TwoDCameraState>,
    mut camera_sync: Local<CameraSync>,
) {
    let current_command = crate::bridge::camera_command_revision();
    let command = if current_command != camera_sync.command_revision {
        camera_sync.command_revision = current_command;
        crate::bridge::camera_command()
    } else {
        None
    };
    if !cached.is_two_d() {
        reset_two_d_input(
            &cached,
            &mut wheel_events,
            &mut pinch_events,
            &mut edit_state,
        );
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
    let current_reset = crate::bridge::camera_reset_revision();
    if current_reset != camera_sync.reset_revision {
        camera_sync.reset_revision = current_reset;
        reset_two_d_camera(&mut transform, &mut projection, &mut camera_state);
    }
    match command {
        Some(crate::bridge::CameraCommand::View2d { center, zoom }) => {
            apply_camera_view_2d(
                center,
                zoom,
                &mut transform,
                &mut projection,
                &mut camera_state,
            );
        }
        Some(crate::bridge::CameraCommand::Focus2d) => {
            frame_selected_2d(nodes, &mut transform, &mut projection, &mut camera_state);
        }
        _ => {}
    }
    let cursor = window.cursor_position();
    let pointer_on_canvas = cursor.is_some_and(input::pointer_over_canvas);
    let viewport_center = camera.logical_viewport_size().map(|size| size * 0.5);
    let anchor = cursor.or(viewport_center);
    let Some(point) = anchor.and_then(|anchor| cursor_design(camera, camera_transform, anchor))
    else {
        return;
    };
    let run_mode = crate::bridge::WORKSPACE_MODE() == crate::bridge::WorkspaceMode::Run;

    if update_pan(
        &mouse_buttons,
        &keyboard,
        pointer_on_canvas,
        point,
        &mut transform,
        &mut edit_state,
    ) {
        return;
    }

    if pointer_on_canvas && mouse_buttons.just_pressed(MouseButton::Left) {
        begin_left_press(
            nodes,
            point,
            run_mode,
            editor_modifier_pressed(&keyboard),
            input::alt_pressed(&keyboard),
            camera_state.zoom,
            &mut edit_state,
        );
    }

    let continuing_drag = matches!(
        edit_state.drag,
        Some(DragState::Move { .. })
            | Some(DragState::Resize { .. })
            | Some(DragState::PipePoint { .. })
            | Some(DragState::Marquee { .. })
    );
    if (pointer_on_canvas || continuing_drag)
        && mouse_buttons.pressed(MouseButton::Left)
        && !update_active_drag(
            nodes,
            &mut edit_state,
            &mut root_query,
            point,
            time.elapsed_secs_f64(),
            camera_state.zoom,
            input::shift_pressed(&keyboard),
            &visual_index,
            &mut child_transforms,
        )
    {
        return;
    }

    if mouse_buttons.just_released(MouseButton::Left) {
        finish_left_release(&cached, nodes, &mut edit_state);
    }

    apply_wheel_2d(
        &mut wheel_events,
        pointer_on_canvas,
        &keyboard,
        camera,
        camera_transform,
        &mut transform,
        &mut projection,
        &mut camera_state,
        cursor,
    );
    apply_pinch_2d(
        &mut pinch_events,
        pointer_on_canvas,
        camera,
        camera_transform,
        &mut transform,
        &mut projection,
        &mut camera_state,
        cursor,
    );

    if !apply_keyboard_2d(
        &cached,
        nodes,
        &keyboard,
        time.elapsed_secs_f64(),
        &mut edit_state,
        &mut camera_state,
        &mut transform,
        &mut projection,
    ) {
        return;
    }

    apply_gamepad_2d(
        &cached,
        nodes,
        &pad,
        time.delta_secs().min(0.05),
        time.elapsed_secs_f64(),
        anchor,
        camera,
        camera_transform,
        &mut transform,
        &mut projection,
        &mut camera_state,
        &mut edit_state,
        &mut root_query,
    );

    finish_pan_release(&mouse_buttons, &keyboard, &mut edit_state);
}

fn reset_two_d_input(
    cached: &CachedProject,
    wheel_events: &mut MessageReader<'_, '_, MouseWheel>,
    pinch_events: &mut MessageReader<'_, '_, CanvasPinch>,
    edit_state: &mut TwoDEditState,
) {
    finish_gamepad_move(cached, edit_state);
    for _event in wheel_events.read() {}
    for _event in pinch_events.read() {}
    edit_state.drag = None;
    edit_state.keyboard_snap = (None, None);
}

fn update_pan(
    mouse_buttons: &ButtonInput<MouseButton>,
    keyboard: &ButtonInput<KeyCode>,
    pointer_on_canvas: bool,
    point: [f32; 2],
    transform: &mut Transform,
    edit_state: &mut TwoDEditState,
) -> bool {
    let continuing_pan = matches!(edit_state.drag, Some(DragState::Pan { .. }));
    let pan_requested = (pointer_on_canvas || continuing_pan)
        && (mouse_buttons.pressed(MouseButton::Middle)
            || (mouse_buttons.pressed(MouseButton::Left)
                && (keyboard.pressed(KeyCode::Space) || continuing_pan)));
    if !pan_requested {
        if matches!(edit_state.drag, Some(DragState::Pan { .. }))
            && let Some(DragState::Pan {
                last_world,
                start_world,
                clear_on_click,
            }) = edit_state.drag.take()
            && clear_on_click
            && (last_world - start_world).length_squared() <= f32::EPSILON
        {
            crate::bridge::select_node(None);
        }
        return false;
    }

    let current = Vec2::new(point[0], point[1]);
    let last = match edit_state.drag.as_ref() {
        Some(DragState::Pan { last_world, .. }) => *last_world,
        _ => {
            edit_state.drag = Some(DragState::Pan {
                last_world: current,
                start_world: current,
                clear_on_click: false,
            });
            current
        }
    };
    if (current - last).length_squared() > f32::EPSILON {
        let previous_world = design_to_world([last.x, last.y], DESIGN_SIZE);
        let current_world = design_to_world([current.x, current.y], DESIGN_SIZE);
        transform.translation.x -= current_world[0] - previous_world[0];
        transform.translation.y -= current_world[1] - previous_world[1];
        if let Some(DragState::Pan { last_world, .. }) = edit_state.drag.as_mut() {
            *last_world = current;
        }
    }
    clamp_two_d_camera_view(transform);
    true
}

fn begin_left_press(
    nodes: &[UiNode],
    point: [f32; 2],
    run_mode: bool,
    additive: bool,
    marquee: bool,
    zoom: f32,
    edit_state: &mut TwoDEditState,
) {
    let selected = crate::bridge::selected_nodes();
    if !run_mode && marquee {
        if !additive {
            crate::bridge::select_node(None);
        }
        edit_state.drag = Some(DragState::Marquee {
            start: point,
            current: point,
            additive,
        });
        return;
    }
    if !run_mode
        && !additive
        && selected.len() == 1
        && let Some(selected_node) = nodes.iter().find(|node| node.id == selected[0])
        && !selected_node.locked
    {
        if matches!(selected_node.widget, Widget2D::Pipe { .. }) {
            if let Some(index) = pipe_handle_at(selected_node, point, PIPE_HANDLE_SCREEN_PX / zoom)
            {
                let targets = CenterSnapTargets::for_nodes(nodes, &selected);
                super::snap_feedback::prime();
                edit_state.drag = Some(DragState::PipePoint {
                    node_id: selected_node.id.clone(),
                    index,
                    start: point,
                    original: Box::new(selected_node.clone()),
                    preview: Box::new(selected_node.clone()),
                    activated: false,
                    snap: PointSnap {
                        point,
                        x: None,
                        y: None,
                        device_id: None,
                        axis: PipeAxisSnap::default(),
                    },
                    last_snap: PointSnapKey::default(),
                    last_feedback_s: f64::NEG_INFINITY,
                    targets,
                });
                return;
            }
        } else if let Some(corner) = resize_corner_at(
            selected_node.rect,
            selected_node.rotation_deg,
            point,
            12.0 / zoom,
        ) {
            edit_state.drag = Some(DragState::Resize {
                node_id: selected_node.id.clone(),
                corner,
                start: point,
                original: selected_node.rect,
                preview: selected_node.rect,
            });
            return;
        }
    }

    if let Some(node) = hit_node(nodes, point, zoom) {
        handle_node_press(nodes, node, point, run_mode, additive, edit_state);
        return;
    }

    if run_mode {
        crate::bridge::select_node(None);
        return;
    }
    edit_state.drag = Some(DragState::Pan {
        last_world: Vec2::new(point[0], point[1]),
        start_world: Vec2::new(point[0], point[1]),
        clear_on_click: !additive,
    });
}

fn handle_node_press(
    nodes: &[UiNode],
    node: &UiNode,
    point: [f32; 2],
    run_mode: bool,
    additive: bool,
    edit_state: &mut TwoDEditState,
) {
    if run_mode {
        crate::bridge::select_node(Some(node.id.clone()));
        if let Widget2D::Button { action, .. } = &node.widget
            && let Err(error) = crate::bridge::invoke_button_action(action)
        {
            crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error.to_string());
        }
        return;
    }
    if additive {
        crate::bridge::toggle_node_selection(&node.id);
        return;
    }

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
    if let Some(bounds_original) = union_rect(moving.iter().map(|node| &node.rect)) {
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

#[allow(clippy::too_many_arguments)]
fn update_active_drag(
    nodes: &[UiNode],
    edit_state: &mut TwoDEditState,
    root_query: &mut TwoDRootQuery<'_, '_>,
    point: [f32; 2],
    now: f64,
    zoom: f32,
    axis_lock: bool,
    visual_index: &TwoDVisualIndex,
    child_transforms: &mut TwoDChildTransformQuery<'_, '_>,
) -> bool {
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
        }) => update_move_drag(
            originals,
            *bounds_original,
            bounds_preview,
            activated,
            snap,
            last_snap,
            last_feedback_s,
            targets,
            [point[0] - start[0], point[1] - start[1]],
            now,
            zoom,
            root_query,
        ),
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
            true
        }
        Some(DragState::PipePoint {
            index,
            start,
            original,
            preview,
            activated,
            snap,
            last_snap,
            last_feedback_s,
            targets,
            ..
        }) => {
            let mut delta = [point[0] - start[0], point[1] - start[1]];
            if !*activated {
                if !drag_activation_reached(delta, zoom) {
                    return false;
                }
                *activated = true;
            }
            if axis_lock {
                if delta[0].abs() >= delta[1].abs() {
                    delta[1] = 0.0;
                } else {
                    delta[0] = 0.0;
                }
            }
            let original_points = pipe_points(original).unwrap_or_default();
            let Some(original_point) = original_points.get(*index).copied() else {
                return true;
            };
            let handle = node_local_to_design(original, original_point);
            let raw = [handle[0] + delta[0], handle[1] + delta[1]];
            let is_endpoint = *index == 0 || *index + 1 == original_points.len();
            *snap = snap_pipe_point(
                original,
                *index,
                raw,
                is_endpoint,
                nodes,
                targets,
                zoom,
                !axis_lock,
            );
            if let Widget2D::Pipe { points, .. } = &mut preview.widget
                && let Some(target) = points.get_mut(*index)
            {
                *target = design_to_node_local(original, snap.point);
            }
            let key = snap.key();
            if key != *last_snap
                && key.engaged()
                && now - *last_feedback_s >= SNAP_FEEDBACK_COOLDOWN_SECS
            {
                super::snap_feedback::trigger();
                *last_feedback_s = now;
            }
            *last_snap = key;
            apply_pipe_preview(root_query, child_transforms, visual_index, preview);
            true
        }
        Some(DragState::Marquee { current, .. }) => {
            *current = point;
            true
        }
        _ => true,
    }
}

#[allow(clippy::too_many_arguments)]
fn update_move_drag(
    originals: &[(String, Rect)],
    bounds_original: Rect,
    bounds_preview: &mut Rect,
    activated: &mut bool,
    snap: &mut CenterSnap,
    last_snap: &mut (Option<f32>, Option<f32>),
    last_feedback_s: &mut f64,
    targets: &CenterSnapTargets,
    delta: [f32; 2],
    now: f64,
    zoom: f32,
    root_query: &mut TwoDRootQuery<'_, '_>,
) -> bool {
    if !*activated {
        if !drag_activation_reached(delta, zoom) {
            return false;
        }
        *activated = true;
    }
    let raw = clamped_group_move(bounds_original, delta);
    *snap = targets.snap(raw, zoom);
    *bounds_preview = snap.rect;
    let engaged = snap.x.is_some_and(|target| last_snap.0 != Some(target))
        || snap.y.is_some_and(|target| last_snap.1 != Some(target));
    if engaged && now - *last_feedback_s >= SNAP_FEEDBACK_COOLDOWN_SECS {
        super::snap_feedback::trigger();
        *last_feedback_s = now;
    }
    *last_snap = (snap.x, snap.y);
    for (node_id, rect) in grouped_previews(originals, bounds_original, *bounds_preview) {
        apply_rect_preview(root_query, &node_id, rect);
    }
    true
}

fn finish_left_release(cached: &CachedProject, nodes: &[UiNode], edit_state: &mut TwoDEditState) {
    let drag = edit_state.drag.take();
    match drag {
        Some(DragState::Move {
            originals,
            bounds_preview,
            bounds_original,
            ..
        }) if bounds_preview != bounds_original => {
            commit_rects(
                cached,
                &grouped_previews(&originals, bounds_original, bounds_preview),
            );
        }
        Some(DragState::Resize {
            node_id,
            preview,
            original,
            ..
        }) if preview != original => {
            commit_rect(cached, &node_id, preview);
        }
        Some(DragState::PipePoint {
            node_id: _,
            original,
            preview,
            ..
        }) if preview.widget != original.widget => {
            commit_node(cached, normalized_pipe_node(*preview));
        }
        Some(drag @ DragState::GamepadMove { .. }) => {
            edit_state.drag = Some(drag);
        }
        Some(DragState::Marquee {
            start,
            current,
            additive,
        }) if start != current => {
            finish_marquee_selection(nodes, start, current, additive);
        }
        _ => {}
    }
}

fn finish_marquee_selection(nodes: &[UiNode], start: [f32; 2], current: [f32; 2], additive: bool) {
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

#[allow(clippy::too_many_arguments)]
fn apply_wheel_2d(
    wheel_events: &mut MessageReader<'_, '_, MouseWheel>,
    pointer_on_canvas: bool,
    keyboard: &ButtonInput<KeyCode>,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    transform: &mut Transform,
    projection: &mut Projection,
    camera_state: &mut TwoDCameraState,
    cursor: Option<Vec2>,
) {
    for event in wheel_events.read() {
        if !pointer_on_canvas {
            continue;
        }
        if input::wheel_intent(event, keyboard) == input::WheelIntent::Zoom {
            if event.y.abs() < f32::EPSILON {
                continue;
            }
            apply_zoom_2d(
                camera,
                camera_transform,
                transform,
                projection,
                camera_state,
                cursor,
                input::wheel_zoom_factor(event.y, event.unit),
            );
        } else if event.x.abs() > f32::EPSILON || event.y.abs() > f32::EPSILON {
            let offset = wheel_pan_translation(Vec2::new(event.x, event.y), camera_state.zoom);
            transform.translation.x += offset.x;
            transform.translation.y += offset.y;
        }
    }
    clamp_two_d_camera_view(transform);
}

fn apply_pinch_2d(
    pinch_events: &mut MessageReader<'_, '_, CanvasPinch>,
    pointer_on_canvas: bool,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    transform: &mut Transform,
    projection: &mut Projection,
    camera_state: &mut TwoDCameraState,
    cursor: Option<Vec2>,
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
        apply_zoom_2d(
            camera,
            camera_transform,
            transform,
            projection,
            camera_state,
            cursor,
            input::pinch_zoom_factor(event.delta),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_keyboard_2d(
    cached: &CachedProject,
    nodes: &[UiNode],
    keyboard: &ButtonInput<KeyCode>,
    now: f64,
    edit_state: &mut TwoDEditState,
    camera_state: &mut TwoDCameraState,
    transform: &mut Transform,
    projection: &mut Projection,
) -> bool {
    if !input::keyboard_input_enabled() || input::command_modifier_pressed(keyboard) {
        return true;
    }
    if keyboard.just_pressed(KeyCode::KeyF) {
        frame_selected_2d(nodes, transform, projection, camera_state);
    }
    if keyboard.just_pressed(KeyCode::Home) {
        reset_two_d_camera(transform, projection, camera_state);
    }
    let selected = crate::bridge::selected_nodes();
    let unlocked = selected_unlocked_nodes(nodes, &selected);
    if unlocked.is_empty() {
        return true;
    }

    let step = if input::shift_pressed(keyboard) {
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

    let mut updates = Vec::new();
    if delta != [0.0, 0.0] {
        let Some(bounds) = union_rect(unlocked.iter().map(|node| &node.rect)) else {
            return false;
        };
        let raw = clamped_group_move(bounds, delta);
        let targets = CenterSnapTargets::for_nodes(nodes, &selected);
        let snap = fine_nudge_snap(targets.snap(raw, camera_state.zoom), raw, delta, bounds);
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
        edit_state.keyboard_snap_until_s = now + 0.45;
        updates = grouped_previews(&node_rects(&unlocked), bounds, snap.rect);
    }

    let rotation_step = if input::shift_pressed(keyboard) {
        15.0
    } else {
        1.0
    };
    let rotation: f32 = match (
        selected.len(),
        keyboard.just_pressed(KeyCode::KeyQ),
        keyboard.just_pressed(KeyCode::KeyE),
    ) {
        (1, true, _) => -rotation_step,
        (1, _, true) => rotation_step,
        _ => 0.0,
    };
    let primary = crate::bridge::selected_node();
    if rotation.abs() > f32::EPSILON
        && let Some(primary_id) = primary.as_ref()
        && let Some(primary_node) = unlocked.iter().find(|node| &node.id == primary_id)
        && !updates.iter().any(|(id, _)| id == primary_id)
    {
        updates.push((primary_node.id.clone(), primary_node.rect));
    }
    if updates.is_empty() {
        return true;
    }

    let Some(screen_id) = cached.screen_id.clone() else {
        return false;
    };
    let commands = updates
        .into_iter()
        .filter_map(|(id, rect)| {
            let node = nodes.iter().find(|node| node.id == id)?;
            let mut updated = node.clone();
            updated.rect = rect;
            if rotation.abs() > f32::EPSILON && primary.as_deref() == Some(id.as_str()) {
                updated.rotation_deg = (updated.rotation_deg + rotation).rem_euclid(360.0);
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
    true
}

#[allow(clippy::too_many_arguments)]
fn apply_gamepad_2d(
    cached: &CachedProject,
    nodes: &[UiNode],
    pad: &GamepadState,
    dt: f32,
    now: f64,
    anchor: Option<Vec2>,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    transform: &mut Transform,
    projection: &mut Projection,
    camera_state: &mut TwoDCameraState,
    edit_state: &mut TwoDEditState,
    root_query: &mut TwoDRootQuery<'_, '_>,
) {
    if crate::bridge::controller_tuning_active()
        || !input::gamepad_input_enabled()
        || !pad.connected
    {
        finish_gamepad_move(cached, edit_state);
        return;
    }

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
            transform,
            projection,
            camera_state,
            anchor,
            (zoom * 1.8 * dt).exp(),
        );
    }
    clamp_two_d_camera_view(transform);
    for (node_id, rect) in
        gamepad_edit_2d(cached, nodes, pad, edit_state, dt, now, camera_state.zoom)
    {
        apply_rect_preview(root_query, &node_id, rect);
    }
    if pad.just_pressed(PadButton::Cross) {
        frame_selected_2d(nodes, transform, projection, camera_state);
    }
}

fn finish_pan_release(
    mouse_buttons: &ButtonInput<MouseButton>,
    keyboard: &ButtonInput<KeyCode>,
    edit_state: &mut TwoDEditState,
) {
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

fn fine_nudge_snap(snap: CenterSnap, raw: Rect, delta: [f32; 2], original: Rect) -> CenterSnap {
    let mut snap = snap_moved_axes(snap, raw, delta);
    let center_x = original.x + original.width * 0.5;
    let center_y = original.y + original.height * 0.5;
    if delta[0].abs() > f32::EPSILON
        && snap
            .x
            .is_some_and(|target| !moving_toward(center_x, delta[0], target))
    {
        snap.rect.x = raw.x;
        snap.x = None;
    }
    if delta[1].abs() > f32::EPSILON
        && snap
            .y
            .is_some_and(|target| !moving_toward(center_y, delta[1], target))
    {
        snap.rect.y = raw.y;
        snap.y = None;
    }
    snap
}

fn moving_toward(center: f32, delta: f32, target: f32) -> bool {
    (target - center) * delta > f32::EPSILON
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
    clamp_two_d_camera_view(transform);
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
    clamp_two_d_camera_view(transform);
}

fn apply_camera_view_2d(
    center: [f32; 2],
    zoom: f32,
    transform: &mut Transform,
    projection: &mut Projection,
    camera_state: &mut TwoDCameraState,
) {
    let zoom = zoom.clamp(0.5, 4.0);
    camera_state.zoom = zoom;
    if let Projection::Orthographic(orthographic) = projection {
        orthographic.scale = 1.0 / zoom;
    }
    let world = design_to_world(center, DESIGN_SIZE);
    transform.translation.x = world[0];
    transform.translation.y = world[1];
    clamp_two_d_camera_view(transform);
}

fn reset_two_d_camera(
    transform: &mut Transform,
    projection: &mut Projection,
    camera_state: &mut TwoDCameraState,
) {
    camera_state.zoom = 1.0;
    if let Projection::Orthographic(orthographic) = projection {
        orthographic.scale = 1.0;
    }
    transform.translation.x = 0.0;
    transform.translation.y = 0.0;
    clamp_two_d_camera_view(transform);
}

fn clamp_two_d_camera_view(transform: &mut Transform) {
    let limit = Vec2::new(DESIGN_SIZE[0] as f32, DESIGN_SIZE[1] as f32) * 0.5;
    transform.translation.x = transform.translation.x.clamp(-limit.x, limit.x);
    transform.translation.y = transform.translation.y.clamp(-limit.y, limit.y);
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

fn apply_pipe_preview(
    root_query: &mut TwoDRootQuery<'_, '_>,
    child_transforms: &mut TwoDChildTransformQuery<'_, '_>,
    visual_index: &TwoDVisualIndex,
    node: &UiNode,
) {
    let Widget2D::Pipe { points, .. } = &node.widget else {
        return;
    };
    if let Some((_, _, mut root_transform)) = root_query
        .iter_mut()
        .find(|(_, root, _)| root.node_id == node.id)
    {
        let center = [
            node.rect.x + node.rect.width * 0.5,
            node.rect.y + node.rect.height * 0.5,
        ];
        let world = design_to_world(center, DESIGN_SIZE);
        root_transform.translation.x = world[0];
        root_transform.translation.y = world[1];
        root_transform.rotation = Quat::from_rotation_z(-node.rotation_deg.to_radians());
    }
    let Some(pipe) = visual_index
        .nodes
        .get(&node.id)
        .and_then(|visual| visual.pipe.as_ref())
    else {
        return;
    };
    for (segment_visual, segment) in pipe.segments.iter().zip(points.windows(2)) {
        let Ok(mut transform) = child_transforms.get_mut(segment_visual.entity) else {
            continue;
        };
        let start = local_design_to_world(segment[0], node.rect.width, node.rect.height);
        let end = local_design_to_world(segment[1], node.rect.width, node.rect.height);
        let delta = end - start;
        let length = delta.length();
        transform.translation = Vec3::new((start.x + end.x) * 0.5, (start.y + end.y) * 0.5, 0.15);
        transform.rotation = Quat::from_rotation_z(delta.y.atan2(delta.x));
        let scale = (length / segment_visual.base_length).max(0.001);
        transform.scale = Vec3::new(scale, 1.0, 1.0);
    }
    for (entity, point) in pipe.joints.iter().zip(points) {
        let Ok(mut transform) = child_transforms.get_mut(*entity) else {
            continue;
        };
        let local = local_design_to_world(*point, node.rect.width, node.rect.height);
        transform.translation.x = local.x;
        transform.translation.y = local.y;
    }
}

fn local_design_to_world(point: [f32; 2], width: f32, height: f32) -> Vec2 {
    Vec2::new(point[0] - width * 0.5, height * 0.5 - point[1])
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
        if drag_ids != unlocked_ids
            && let Some(DragState::GamepadMove {
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
    let snap = fine_nudge_snap(targets.snap(raw, zoom), raw, delta, bounds);
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

fn commit_node(cached: &CachedProject, node: UiNode) {
    let Some(screen_id) = cached.screen_id.clone() else {
        return;
    };
    let Some(project) = cached.project.as_ref() else {
        return;
    };
    let Some(Screen::TwoD { nodes, .. }) = project.screen(&screen_id) else {
        return;
    };
    let Some(current) = nodes.iter().find(|current| current.id == node.id) else {
        return;
    };
    if *current == node {
        return;
    }
    if let Err(error) = crate::bridge::execute(EditorCommand::Update2dNode { screen_id, node }) {
        crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error.to_string());
    }
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
    camera_query: Query<
        (&Camera, &GlobalTransform),
        (With<Camera2d>, Without<DeleteFxOverlayCamera>),
    >,
    mut gizmos: Gizmos,
) {
    if !cached.is_two_d() || crate::bridge::WORKSPACE_MODE() == crate::bridge::WorkspaceMode::Run {
        return;
    }
    draw_grid(&mut gizmos);

    let Some(nodes) = active_nodes(&cached) else {
        return;
    };
    let selected = crate::bridge::selected_nodes();
    if !matches!(
        edit_state.drag,
        Some(DragState::Move { .. })
            | Some(DragState::Resize { .. })
            | Some(DragState::PipePoint { .. })
            | Some(DragState::Marquee { .. })
            | Some(DragState::Pan { .. })
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
    let previews = selection_previews(&edit_state, &selected_nodes);
    let preview_rects = previews.iter().map(|(_, rect)| rect).collect::<Vec<_>>();
    let preview_bounds = union_rect(preview_rects.into_iter());
    let selection_center = preview_bounds.map(|rect| {
        design_to_world(
            [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5],
            DESIGN_SIZE,
        )
    });
    draw_selection(
        &mut gizmos,
        &selected_nodes,
        &previews,
        preview_bounds,
        camera_state.zoom,
        &edit_state,
    );

    let snap = active_center_snap(&edit_state, time.elapsed_secs_f64(), preview_bounds);
    if let Some(snap) = snap {
        draw_center_snap(&mut gizmos, snap, selection_center, camera_state.zoom);
    }
    draw_pipe_snap(&mut gizmos, &edit_state, nodes, camera_state.zoom);
    draw_marquee(&mut gizmos, edit_state.drag.as_ref());
}

fn draw_grid(gizmos: &mut Gizmos) {
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
}

fn selection_previews(
    edit_state: &TwoDEditState,
    selected_nodes: &[&UiNode],
) -> Vec<(String, Rect)> {
    match &edit_state.drag {
        Some(DragState::Move {
            originals,
            bounds_original,
            bounds_preview,
            ..
        })
        | Some(DragState::GamepadMove {
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
    }
}

fn draw_selection(
    gizmos: &mut Gizmos,
    selected_nodes: &[&UiNode],
    previews: &[(String, Rect)],
    preview_bounds: Option<Rect>,
    zoom: f32,
    edit_state: &TwoDEditState,
) {
    for node in selected_nodes {
        if pipe_points(node).is_some() {
            let preview = pipe_preview_node(edit_state, node).unwrap_or_else(|| (*node).clone());
            draw_pipe_selection(gizmos, &preview, zoom);
            continue;
        }
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
        if pipe_points(node).is_some() {
            return;
        }
        let rect = previews
            .iter()
            .find(|(id, _)| id == &node.id)
            .map(|(_, rect)| *rect)
            .unwrap_or(node.rect);
        for corner in rotated_corners(rect, node.rotation_deg) {
            let world = design_to_world(corner, DESIGN_SIZE);
            gizmos.circle_2d(
                Vec2::new(world[0], world[1]),
                5.0 / zoom,
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
}

fn active_center_snap(
    edit_state: &TwoDEditState,
    now: f64,
    preview_bounds: Option<Rect>,
) -> Option<CenterSnap> {
    match &edit_state.drag {
        Some(DragState::Move { snap, .. }) | Some(DragState::GamepadMove { snap, .. }) => {
            Some(*snap)
        }
        _ if now <= edit_state.keyboard_snap_until_s => preview_bounds
            .filter(|_| {
                edit_state.keyboard_snap.0.is_some() || edit_state.keyboard_snap.1.is_some()
            })
            .map(|rect| CenterSnap {
                rect,
                x: edit_state.keyboard_snap.0,
                y: edit_state.keyboard_snap.1,
            }),
        _ => None,
    }
}

fn pipe_preview_node(edit_state: &TwoDEditState, node: &UiNode) -> Option<UiNode> {
    match &edit_state.drag {
        Some(DragState::PipePoint {
            node_id, preview, ..
        }) if node_id == &node.id => Some((**preview).clone()),
        _ => None,
    }
}

fn draw_pipe_selection(gizmos: &mut Gizmos, node: &UiNode, zoom: f32) {
    let Some(points) = pipe_points(node) else {
        return;
    };
    let color = Color::srgb(0.36, 0.95, 0.88);
    let world_points = points
        .iter()
        .map(|point| {
            let design = node_local_to_design(node, *point);
            design_to_world(design, DESIGN_SIZE)
        })
        .collect::<Vec<_>>();
    for segment in world_points.windows(2) {
        gizmos.line_2d(
            Vec2::new(segment[0][0], segment[0][1]),
            Vec2::new(segment[1][0], segment[1][1]),
            Color::srgba(0.36, 0.95, 0.88, 0.45),
        );
    }
    for (index, point) in world_points.iter().enumerate() {
        let endpoint = index == 0 || index + 1 == world_points.len();
        gizmos.circle_2d(
            Vec2::new(point[0], point[1]),
            (if endpoint { 7.0 } else { 5.5 }) / zoom.max(f32::EPSILON),
            color,
        );
    }
}

fn draw_pipe_snap(gizmos: &mut Gizmos, edit_state: &TwoDEditState, nodes: &[UiNode], zoom: f32) {
    let Some(DragState::PipePoint { snap, .. }) = &edit_state.drag else {
        return;
    };
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
    let point = design_to_world(snap.point, DESIGN_SIZE);
    if let Some(anchor) = snap.axis.horizontal_from {
        let anchor = design_to_world(anchor, DESIGN_SIZE);
        gizmos.line_2d(
            Vec2::new(anchor[0], anchor[1]),
            Vec2::new(point[0], point[1]),
            guide,
        );
    }
    if let Some(anchor) = snap.axis.vertical_from {
        let anchor = design_to_world(anchor, DESIGN_SIZE);
        gizmos.line_2d(
            Vec2::new(anchor[0], anchor[1]),
            Vec2::new(point[0], point[1]),
            guide,
        );
    }
    if snap.engaged() {
        gizmos.circle_2d(
            Vec2::new(point[0], point[1]),
            9.0 / zoom.max(f32::EPSILON),
            guide,
        );
    }
    if let Some(device_id) = snap.device_id.as_deref()
        && let Some(device) = nodes.iter().find(|node| node.id == device_id)
    {
        let center = design_to_world(
            [
                device.rect.x + device.rect.width * 0.5,
                device.rect.y + device.rect.height * 0.5,
            ],
            DESIGN_SIZE,
        );
        gizmos.rect_2d(
            Isometry2d::new(
                Vec2::new(center[0], center[1]),
                Rot2::radians(-device.rotation_deg.to_radians()),
            ),
            Vec2::new(
                device.rect.width + 8.0 / zoom,
                device.rect.height + 8.0 / zoom,
            ),
            guide,
        );
    }
}

fn draw_center_snap(
    gizmos: &mut Gizmos,
    snap: CenterSnap,
    selection_center: Option<[f32; 3]>,
    zoom: f32,
) {
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
        gizmos.circle_2d(Vec2::new(center[0], center[1]), 4.0 / zoom, guide);
    }
}

fn draw_marquee(gizmos: &mut Gizmos, drag: Option<&DragState>) {
    let Some(DragState::Marquee { start, current, .. }) = drag else {
        return;
    };
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
    two_d_cameras: Query<
        (&Camera, &GlobalTransform),
        (With<Camera2d>, Without<DeleteFxOverlayCamera>),
    >,
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
    use super::{
        CenterSnapTargets, DragState, TwoDEditState, apply_camera_view_2d, clamp_two_d_camera_view,
        drag_activation_reached, fine_nudge_snap, node_local_to_design, normalized_pipe_node,
        reset_two_d_camera, snap_pipe_point, update_pan, wheel_pan_translation,
    };
    use bevy::input::{ButtonInput, mouse::MouseButton};
    use bevy::prelude::{KeyCode, OrthographicProjection, Projection, Transform, Vec2};
    use hmi_core::{CenterSnap, EquipmentKind, HexColor, Rect, UiNode, Widget2D};

    fn pipe_node(points: Vec<[f32; 2]>) -> UiNode {
        UiNode {
            id: "pipe".to_string(),
            name: "Pipe".to_string(),
            rect: Rect {
                x: 100.0,
                y: 100.0,
                width: 200.0,
                height: 80.0,
            },
            rotation_deg: 0.0,
            z: 2,
            visible: true,
            locked: false,
            widget: Widget2D::Pipe {
                points,
                thickness: 8.0,
                color: HexColor("#2d6d77".to_string()),
            },
            bindings: Vec::new(),
        }
    }

    fn equipment(id: &str, rect: Rect) -> UiNode {
        UiNode {
            id: id.to_string(),
            name: "Equipment".to_string(),
            rect,
            rotation_deg: 0.0,
            z: 1,
            visible: true,
            locked: false,
            widget: Widget2D::EquipmentSymbol {
                equipment: EquipmentKind::Fan,
                label: "Fan".to_string(),
            },
            bindings: Vec::new(),
        }
    }

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
    fn plain_left_pan_continues_until_release() {
        let mut mouse = ButtonInput::<MouseButton>::default();
        mouse.press(MouseButton::Left);
        let keyboard = ButtonInput::<KeyCode>::default();
        let mut edit_state = TwoDEditState {
            drag: Some(DragState::Pan {
                last_world: Vec2::new(10.0, 10.0),
                start_world: Vec2::new(10.0, 10.0),
                clear_on_click: true,
            }),
            ..TwoDEditState::default()
        };
        let mut transform = Transform::default();

        assert!(update_pan(
            &mouse,
            &keyboard,
            false,
            [20.0, 10.0],
            &mut transform,
            &mut edit_state,
        ));
        assert!(transform.translation.x < 0.0);
        assert!(matches!(edit_state.drag, Some(DragState::Pan { .. })));
    }

    #[test]
    fn pan_check_preserves_marquee_drag() {
        let mouse = ButtonInput::<MouseButton>::default();
        let keyboard = ButtonInput::<KeyCode>::default();
        let mut edit_state = TwoDEditState {
            drag: Some(DragState::Marquee {
                start: [10.0, 10.0],
                current: [20.0, 20.0],
                additive: false,
            }),
            ..TwoDEditState::default()
        };
        let mut transform = Transform::default();

        assert!(!update_pan(
            &mouse,
            &keyboard,
            false,
            [30.0, 30.0],
            &mut transform,
            &mut edit_state,
        ));
        assert!(matches!(edit_state.drag, Some(DragState::Marquee { .. })));
    }

    #[test]
    fn two_d_camera_stays_within_the_design_canvas() {
        let mut transform = Transform::from_xyz(2_000.0, -2_000.0, 0.0);

        clamp_two_d_camera_view(&mut transform);

        assert_eq!(transform.translation.x, 800.0);
        assert_eq!(transform.translation.y, -450.0);
    }

    #[test]
    fn reset_two_d_camera_restores_the_full_design_view() {
        let mut transform = Transform::from_xyz(320.0, -180.0, 1000.0);
        let mut projection = Projection::Orthographic(OrthographicProjection::default_2d());
        let mut camera_state = super::TwoDCameraState { zoom: 2.5 };

        reset_two_d_camera(&mut transform, &mut projection, &mut camera_state);

        assert_eq!(transform.translation.x, 0.0);
        assert_eq!(transform.translation.y, 0.0);
        assert_eq!(transform.translation.z, 1000.0);
        assert_eq!(camera_state.zoom, 1.0);
        let Projection::Orthographic(orthographic) = projection else {
            panic!("2D camera must use an orthographic projection");
        };
        assert_eq!(orthographic.scale, 1.0);
    }

    #[test]
    fn camera_view_command_sets_center_and_zoom() {
        let mut transform = Transform::default();
        let mut projection = Projection::Orthographic(OrthographicProjection::default_2d());
        let mut camera_state = super::TwoDCameraState { zoom: 1.0 };

        apply_camera_view_2d(
            [500.0, 300.0],
            2.0,
            &mut transform,
            &mut projection,
            &mut camera_state,
        );

        assert_eq!(transform.translation.x, -300.0);
        assert_eq!(transform.translation.y, 150.0);
        assert_eq!(camera_state.zoom, 2.0);
        let Projection::Orthographic(orthographic) = projection else {
            panic!("2D camera must use an orthographic projection");
        };
        assert_eq!(orthographic.scale, 0.5);
    }

    #[test]
    fn fine_nudge_escapes_an_existing_center_snap() {
        let original = Rect {
            x: 670.0,
            y: 250.0,
            width: 260.0,
            height: 180.0,
        };
        let raw = Rect {
            x: 671.0,
            y: 250.0,
            width: 260.0,
            height: 180.0,
        };
        let sticky_snap = CenterSnap {
            rect: Rect {
                x: 670.0,
                y: 250.0,
                width: 260.0,
                height: 180.0,
            },
            x: Some(800.0),
            y: Some(340.0),
        };
        let snapped = fine_nudge_snap(sticky_snap, raw, [1.0, 0.0], original);
        assert_eq!(snapped.rect.x, 671.0);
        assert_eq!(snapped.rect.y, 250.0);
        assert_eq!(snapped.x, None);
        assert_eq!(snapped.y, None);
    }

    #[test]
    fn fine_nudge_reengages_after_leaving_and_returning_to_center() {
        let start = Rect {
            x: 670.0,
            y: 250.0,
            width: 260.0,
            height: 180.0,
        };
        let moved = Rect {
            x: 671.0,
            y: 250.0,
            width: 260.0,
            height: 180.0,
        };
        let leaving = fine_nudge_snap(
            CenterSnap {
                rect: moved,
                x: Some(800.0),
                y: None,
            },
            moved,
            [1.0, 0.0],
            start,
        );
        assert_eq!(leaving.x, None);
        assert_eq!(leaving.rect.x, 671.0);

        let returning = fine_nudge_snap(
            CenterSnap {
                rect: start,
                x: Some(800.0),
                y: None,
            },
            start,
            [-1.0, 0.0],
            moved,
        );
        assert_eq!(returning.x, Some(800.0));
        assert_eq!(returning.rect.x, 670.0);
    }

    #[test]
    fn normalized_pipe_keeps_every_design_point_in_place() {
        let mut original = pipe_node(vec![[10.0, 20.0], [180.0, 20.0], [180.0, 60.0]]);
        original.rotation_deg = 37.0;
        if let Widget2D::Pipe { points, .. } = &mut original.widget {
            points[0] = [-40.0, -25.0];
        }
        let expected = match &original.widget {
            Widget2D::Pipe { points, .. } => points
                .iter()
                .map(|point| node_local_to_design(&original, *point))
                .collect::<Vec<_>>(),
            _ => unreachable!(),
        };
        let normalized = normalized_pipe_node(original);
        let actual = match &normalized.widget {
            Widget2D::Pipe { points, .. } => points
                .iter()
                .map(|point| node_local_to_design(&normalized, *point))
                .collect::<Vec<_>>(),
            _ => unreachable!(),
        };
        for (expected, actual) in expected.iter().zip(actual) {
            assert!((expected[0] - actual[0]).abs() < 0.001);
            assert!((expected[1] - actual[1]).abs() < 0.001);
        }
    }

    #[test]
    fn pipe_endpoint_snaps_to_nearest_equipment_edge() {
        let pipe = pipe_node(vec![[0.0, 20.0], [180.0, 20.0]]);
        let device = equipment(
            "fan",
            Rect {
                x: 320.0,
                y: 180.0,
                width: 100.0,
                height: 80.0,
            },
        );
        let nodes = [pipe.clone(), device];
        let targets = CenterSnapTargets::for_nodes(&nodes, std::slice::from_ref(&pipe.id));
        let snapped = snap_pipe_point(&pipe, 1, [316.0, 220.0], true, &nodes, &targets, 1.0, false);
        assert_eq!(snapped.device_id.as_deref(), Some("fan"));
        assert_eq!(snapped.point, [320.0, 220.0]);
    }

    #[test]
    fn pipe_point_snaps_to_center_line() {
        let pipe = pipe_node(vec![[0.0, 20.0], [100.0, 40.0], [180.0, 20.0]]);
        let nodes = [pipe.clone()];
        let targets = CenterSnapTargets::for_nodes(&nodes, std::slice::from_ref(&pipe.id));
        let snapped = snap_pipe_point(
            &pipe,
            1,
            [794.0, 350.0],
            false,
            &nodes,
            &targets,
            1.0,
            false,
        );
        assert_eq!(snapped.point[0], 800.0);
        assert_eq!(snapped.x, Some(800.0));
        assert_eq!(snapped.y, None);
    }

    #[test]
    fn pipe_point_snaps_segment_to_horizontal() {
        let pipe = pipe_node(vec![[0.0, 20.0], [180.0, 20.0]]);
        let nodes = [pipe.clone()];
        let targets = CenterSnapTargets::for_nodes(&nodes, std::slice::from_ref(&pipe.id));
        let snapped = snap_pipe_point(&pipe, 1, [300.0, 128.0], true, &nodes, &targets, 1.0, true);
        assert_eq!(snapped.point, [300.0, 120.0]);
        assert!(snapped.axis.horizontal_from.is_some());
        assert!(snapped.engaged());
    }

    #[test]
    fn pipe_point_snaps_segment_to_vertical() {
        let pipe = pipe_node(vec![[0.0, 20.0], [180.0, 20.0]]);
        let nodes = [pipe.clone()];
        let targets = CenterSnapTargets::for_nodes(&nodes, std::slice::from_ref(&pipe.id));
        let snapped = snap_pipe_point(&pipe, 0, [288.0, 300.0], true, &nodes, &targets, 1.0, true);
        assert_eq!(snapped.point, [280.0, 300.0]);
        assert!(snapped.axis.vertical_from.is_some());
    }

    #[test]
    fn interior_pipe_point_can_snap_both_axis_segments() {
        let pipe = pipe_node(vec![[0.0, 20.0], [100.0, 20.0], [100.0, 80.0]]);
        let nodes = [pipe.clone()];
        let targets = CenterSnapTargets::for_nodes(&nodes, std::slice::from_ref(&pipe.id));
        let snapped = snap_pipe_point(&pipe, 1, [206.0, 127.0], false, &nodes, &targets, 1.0, true);
        assert_eq!(snapped.point, [200.0, 120.0]);
        assert!(snapped.axis.horizontal_from.is_some());
        assert!(snapped.axis.vertical_from.is_some());
    }

    #[test]
    fn pipe_axis_snap_releases_outside_angle_tolerance() {
        let pipe = pipe_node(vec![[0.0, 20.0], [180.0, 20.0]]);
        let nodes = [pipe.clone()];
        let targets = CenterSnapTargets::for_nodes(&nodes, std::slice::from_ref(&pipe.id));
        let snapped = snap_pipe_point(&pipe, 1, [300.0, 180.0], true, &nodes, &targets, 1.0, true);
        assert_eq!(snapped.point, [300.0, 180.0]);
        assert!(!snapped.axis.engaged());
    }

    #[test]
    fn explicit_axis_lock_disables_automatic_angle_snap() {
        let pipe = pipe_node(vec![[0.0, 20.0], [180.0, 20.0]]);
        let nodes = [pipe.clone()];
        let targets = CenterSnapTargets::for_nodes(&nodes, std::slice::from_ref(&pipe.id));
        let snapped = snap_pipe_point(&pipe, 1, [300.0, 128.0], true, &nodes, &targets, 1.0, false);
        assert_eq!(snapped.point, [300.0, 128.0]);
        assert!(!snapped.axis.engaged());
    }
}
