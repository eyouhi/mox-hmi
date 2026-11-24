use super::assets::{HmiFont, color_material};
use super::three_d::{DeviceLabel3D, spawn_device};
use super::two_d::{DESIGN_SIZE, HmiScreenContent, spawn_node};
#[cfg(not(target_arch = "wasm32"))]
use bevy::camera::RenderTarget;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use hmi_core::design_to_world;

use crate::bridge::DeleteFxTarget;

const OVERLAY_LAYER: usize = 1;
const PACMAN_RADIUS: f32 = 40.0;
const PACMAN_Z: f32 = 60.0;
const MOUTH_FRAME_SECS: f32 = 0.065;
const MOUTH_GAPS: [f32; 3] = [0.1, 0.95, 1.65];
const ROUTE_ENTRY_DISTANCE: f32 = PACMAN_RADIUS * 1.8;
const ROUTE_EXIT_DISTANCE: f32 = PACMAN_RADIUS * 1.8;
const GHOST_SHRINK_SECS: f32 = 0.16;
const MIN_EFFECT_SECS: f32 = 0.48;
const MAX_EFFECT_SECS: f32 = 1.15;

#[derive(Component)]
pub struct DeleteFxOverlayCamera;

#[derive(Component)]
pub(crate) struct DeleteFxPacMan;

#[derive(Component)]
pub(crate) struct DeleteFxMouth {
    frames: [Handle<Mesh>; 3],
    index: usize,
}

#[derive(Component)]
pub(crate) struct DeleteFxGhost {
    base_scale: Vec3,
    ring: Option<Entity>,
    eaten_at: Option<f32>,
}

#[derive(Resource, Default)]
pub struct DeleteFxState {
    last_revision: u64,
    active: Option<ActiveDeleteFx>,
}

struct ActiveDeleteFx {
    screen_id: String,
    document_revision: u64,
    pacman: Entity,
    mouth: Entity,
    ghost_entities: Vec<Entity>,
    rings: Vec<Entity>,
    target_points: Vec<Vec2>,
    target_eaten_at: Vec<Option<f32>>,
    route: Vec<Vec2>,
    route_length: f32,
    speed: f32,
    elapsed: f32,
    mouth_elapsed: f32,
    chomps: usize,
}

pub(crate) fn setup_delete_fx_camera(mut commands: Commands) {
    let _camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: 100,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::WindowSize,
                ..OrthographicProjection::default_2d()
            }),
            Transform::from_xyz(0.0, 0.0, 1000.0),
            RenderLayers::layer(OVERLAY_LAYER),
            DeleteFxOverlayCamera,
            Name::new("Delete FX overlay camera"),
        ))
        .id();
    #[cfg(not(target_arch = "wasm32"))]
    commands.entity(_camera).insert(RenderTarget::TextureView(
        super::plugin::NATIVE_TEXTURE_VIEW,
    ));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_delete_fx(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials_2d: ResMut<Assets<ColorMaterial>>,
    mut materials_3d: ResMut<Assets<StandardMaterial>>,
    font: Option<Res<HmiFont>>,
    asset_server: Res<AssetServer>,
    cameras_2d: Query<
        (&Camera, &GlobalTransform),
        (With<Camera2d>, Without<DeleteFxOverlayCamera>),
    >,
    cameras_3d: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut state: ResMut<DeleteFxState>,
) {
    let revision = crate::bridge::delete_fx_revision();
    if revision == state.last_revision {
        return;
    }
    state.last_revision = revision;
    if let Some(active) = state.active.take() {
        despawn_effect(&mut commands, &active);
    }
    if !crate::bridge::delete_fx_enabled() {
        return;
    }
    let Some(effect) = crate::bridge::delete_fx() else {
        return;
    };
    if effect.document_revision != crate::bridge::document_revision() {
        return;
    }
    let Some(active_screen) = crate::bridge::active_screen() else {
        return;
    };
    if effect.screen_id != active_screen {
        return;
    }
    let Some(font) = font.as_deref() else {
        return;
    };
    let (camera, camera_transform) = if effect.is_three_d() {
        let Ok((camera, transform)) = cameras_3d.single() else {
            return;
        };
        (camera, transform)
    } else {
        let Ok((camera, transform)) = cameras_2d.single() else {
            return;
        };
        (camera, transform)
    };

    let mut ghost_entities = Vec::new();
    let mut rings = Vec::new();
    let mut target_points = Vec::new();
    for target in &effect.targets {
        let Some(point) = project_target(camera, camera_transform, target) else {
            continue;
        };
        let spawned = match target {
            DeleteFxTarget::TwoD { node } => {
                let visual = spawn_node(&mut commands, &mut meshes, &mut materials_2d, font, node);
                commands.entity(visual.root).remove::<HmiScreenContent>();
                commands.entity(visual.root).insert(DeleteFxGhost {
                    base_scale: Vec3::ONE,
                    ring: None,
                    eaten_at: None,
                });
                Some((visual.root, None))
            }
            DeleteFxTarget::ThreeD { node } => spawn_device(
                &mut commands,
                &mut meshes,
                &mut materials_3d,
                &asset_server,
                node,
            )
            .map(|visual| {
                commands
                    .entity(visual.root)
                    .remove::<HmiScreenContent>()
                    .remove::<DeviceLabel3D>();
                commands
                    .entity(visual.selection_ring)
                    .remove::<HmiScreenContent>();
                commands.entity(visual.root).insert(DeleteFxGhost {
                    base_scale: Vec3::from_array(node.scale),
                    ring: Some(visual.selection_ring),
                    eaten_at: None,
                });
                (visual.root, Some(visual.selection_ring))
            }),
        };
        if let Some((root, ring)) = spawned {
            ghost_entities.push(root);
            if let Some(ring) = ring {
                rings.push(ring);
            }
            target_points.push(point);
        }
    }
    if target_points.is_empty() {
        for ghost in &ghost_entities {
            commands.entity(*ghost).despawn();
        }
        for ring in &rings {
            commands.entity(*ring).despawn();
        }
        return;
    }

    let route = build_route(&target_points);
    let route_length = polyline_length(&route);
    let desired_secs =
        (0.34 + 0.14 * target_points.len() as f32).clamp(MIN_EFFECT_SECS, MAX_EFFECT_SECS);
    let speed = (route_length / desired_secs).clamp(320.0, 4_200.0);
    let (pacman, mouth) = spawn_pacman(
        &mut commands,
        &mut meshes,
        &mut materials_2d,
        route.first().copied().unwrap_or_default(),
    );
    let target_count = target_points.len();
    state.active = Some(ActiveDeleteFx {
        screen_id: effect.screen_id,
        document_revision: effect.document_revision,
        pacman,
        mouth,
        ghost_entities,
        rings,
        target_points,
        target_eaten_at: vec![None; target_count],
        route,
        route_length,
        speed,
        elapsed: 0.0,
        mouth_elapsed: 0.0,
        chomps: 0,
    });
}

fn project_target(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    target: &DeleteFxTarget,
) -> Option<Vec2> {
    let world = match target {
        DeleteFxTarget::TwoD { node } => {
            let center = [
                node.rect.x + node.rect.width * 0.5,
                node.rect.y + node.rect.height * 0.5,
            ];
            let world = design_to_world(center, DESIGN_SIZE);
            Vec3::new(world[0], world[1], node.z as f32)
        }
        DeleteFxTarget::ThreeD { node } => Vec3::from_array(node.position),
    };
    let viewport = camera.world_to_viewport(camera_transform, world).ok()?;
    let size = camera.logical_viewport_size()?;
    Some(Vec2::new(
        viewport.x - size.x * 0.5,
        size.y * 0.5 - viewport.y,
    ))
}

fn spawn_pacman(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    at: Vec2,
) -> (Entity, Entity) {
    let body = color_material(materials, Color::srgb(1.0, 0.84, 0.12));
    let eye = color_material(materials, Color::srgb(0.035, 0.055, 0.06));
    let glow = color_material(materials, Color::srgba(1.0, 0.84, 0.12, 0.16));
    let frames = MOUTH_GAPS.map(|gap| {
        meshes.add(CircularSector::from_radians(
            PACMAN_RADIUS,
            std::f32::consts::TAU - gap,
        ))
    });
    let root = commands
        .spawn((
            DeleteFxPacMan,
            Transform::from_xyz(at.x, at.y, PACMAN_Z),
            Visibility::Visible,
            RenderLayers::layer(OVERLAY_LAYER),
            Name::new("Delete FX Pac-Man"),
        ))
        .id();
    let mut mouth = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|parent| {
        parent.spawn((
            Mesh2d(meshes.add(Circle::new(PACMAN_RADIUS * 1.45))),
            MeshMaterial2d(glow),
            Transform::from_xyz(0.0, 0.0, -0.1),
            RenderLayers::layer(OVERLAY_LAYER),
        ));
        mouth = parent
            .spawn((
                Mesh2d(frames[0].clone()),
                MeshMaterial2d(body),
                Transform::from_xyz(0.0, 0.0, 0.1),
                DeleteFxMouth {
                    frames: frames.clone(),
                    index: 0,
                },
                RenderLayers::layer(OVERLAY_LAYER),
            ))
            .id();
        parent.spawn((
            Mesh2d(meshes.add(Circle::new(PACMAN_RADIUS * 0.15))),
            MeshMaterial2d(eye),
            Transform::from_xyz(PACMAN_RADIUS * 0.38, 0.0, 0.2),
            RenderLayers::layer(OVERLAY_LAYER),
        ));
    });
    (root, mouth)
}

fn build_route(points: &[Vec2]) -> Vec<Vec2> {
    let leftmost = points
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.x.total_cmp(&right.x))
        .map(|(index, _)| index)
        .unwrap_or(0);
    let start = points[leftmost] - Vec2::new(ROUTE_ENTRY_DISTANCE, 0.0);
    let order = nearest_neighbor_order(points, start);
    let mut route = Vec::with_capacity(order.len() + 2);
    route.push(start);
    route.extend(order.into_iter().map(|index| points[index]));
    let last = *route.last().unwrap_or(&start);
    let previous = route
        .get(route.len().saturating_sub(2))
        .copied()
        .unwrap_or(start);
    let direction = (last - previous).normalize_or_zero();
    route.push(last + direction * ROUTE_EXIT_DISTANCE);
    route
}

fn nearest_neighbor_order(points: &[Vec2], start: Vec2) -> Vec<usize> {
    let mut remaining = (0..points.len()).collect::<Vec<_>>();
    let mut order = Vec::with_capacity(points.len());
    let mut cursor = start;
    while !remaining.is_empty() {
        let (slot, index) = remaining
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                points[**left]
                    .distance_squared(cursor)
                    .total_cmp(&points[**right].distance_squared(cursor))
            })
            .map(|(slot, index)| (slot, *index))
            .unwrap_or((0, remaining[0]));
        order.push(index);
        remaining.remove(slot);
        cursor = points[index];
    }
    order
}

fn polyline_length(points: &[Vec2]) -> f32 {
    points
        .windows(2)
        .map(|segment| segment[0].distance(segment[1]))
        .sum()
}

fn sample_route(points: &[Vec2], distance: f32) -> (Vec2, Vec2) {
    let Some(first) = points.first().copied() else {
        return (Vec2::ZERO, Vec2::X);
    };
    let mut remaining = distance.max(0.0);
    for segment in points.windows(2) {
        let delta = segment[1] - segment[0];
        let length = delta.length();
        if length <= f32::EPSILON {
            continue;
        }
        if remaining <= length {
            return (segment[0] + delta * (remaining / length), delta / length);
        }
        remaining -= length;
    }
    let direction = points
        .len()
        .checked_sub(2)
        .map(|index| (points[index + 1] - points[index]).normalize_or_zero())
        .unwrap_or(Vec2::X);
    (*points.last().unwrap_or(&first), direction)
}

fn pacman_rotation(direction: Vec2) -> Quat {
    if direction.length_squared() <= f32::EPSILON {
        return Quat::IDENTITY;
    }
    Quat::from_rotation_z(direction.y.atan2(direction.x) + std::f32::consts::FRAC_PI_2)
}

fn mouth_frame(elapsed: f32) -> usize {
    let step = (elapsed / MOUTH_FRAME_SECS) as usize % 4;
    [1, 0, 2, 0][step]
}

pub(crate) fn tick_delete_fx(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<DeleteFxState>,
    mut pacman_query: Query<&mut Transform, (With<DeleteFxPacMan>, Without<DeleteFxGhost>)>,
    mut mouth_query: Query<(&mut Mesh2d, &mut DeleteFxMouth)>,
    mut ghost_query: Query<
        (&mut Transform, &mut Visibility, &mut DeleteFxGhost),
        Without<DeleteFxPacMan>,
    >,
) {
    if state.active.is_none() {
        return;
    }
    let enabled = crate::bridge::delete_fx_enabled();
    let screen_matches = state.active.as_ref().is_some_and(|active| {
        crate::bridge::active_screen().as_deref() == Some(active.screen_id.as_str())
    });
    let revision_matches = state
        .active
        .as_ref()
        .is_some_and(|active| crate::bridge::document_revision() == active.document_revision);
    if !enabled || !screen_matches || !revision_matches {
        if let Some(active) = state.active.take() {
            despawn_effect(&mut commands, &active);
        }
        return;
    }

    let dt = time.delta_secs().min(0.05);
    let mut finished = false;
    {
        let active = state.active.as_mut().expect("checked above");
        active.elapsed += dt;
        active.mouth_elapsed += dt;
        let distance = active.elapsed * active.speed;
        let (position, direction) = sample_route(&active.route, distance);
        if distance >= active.route_length {
            finished = true;
        }

        if let Ok(mut transform) = pacman_query.get_mut(active.pacman) {
            transform.translation.x = position.x;
            transform.translation.y = position.y;
            transform.rotation = pacman_rotation(direction);
            transform.scale = Vec3::splat(1.0 + 0.07 * (active.mouth_elapsed * 19.0).sin());
        }

        let frame = mouth_frame(active.mouth_elapsed);
        if let Ok((mut mesh, mut mouth)) = mouth_query.get_mut(active.mouth)
            && mouth.index != frame
        {
            mesh.0 = mouth.frames[frame].clone();
            mouth.index = frame;
        }

        for index in 0..active.target_points.len() {
            if active.target_eaten_at[index].is_some() {
                continue;
            }
            if position.distance(active.target_points[index]) > PACMAN_RADIUS * 1.55 {
                continue;
            }
            active.target_eaten_at[index] = Some(active.elapsed);
            if let Some(ghost) = active.ghost_entities.get(index)
                && let Ok((_, _, mut marker)) = ghost_query.get_mut(*ghost)
            {
                marker.eaten_at = Some(active.elapsed);
                if let Some(ring) = marker.ring {
                    commands.entity(ring).insert(Visibility::Hidden);
                }
            }
            if active.chomps < 6 {
                active.chomps += 1;
                super::snap_feedback::chomp();
            }
        }

        for index in 0..active.target_points.len() {
            let Some(eaten_at) = active.target_eaten_at[index] else {
                continue;
            };
            let Some(entity) = active.ghost_entities.get(index).copied() else {
                continue;
            };
            let Ok((mut transform, mut visibility, marker)) = ghost_query.get_mut(entity) else {
                continue;
            };
            let remaining = 1.0 - ((active.elapsed - eaten_at) / GHOST_SHRINK_SECS).clamp(0.0, 1.0);
            transform.scale = marker.base_scale * remaining;
            if remaining <= 0.02 {
                *visibility = Visibility::Hidden;
            }
        }
    }

    if finished {
        if let Some(active) = state.active.take() {
            despawn_effect(&mut commands, &active);
        }
    }
}

fn despawn_effect(commands: &mut Commands, active: &ActiveDeleteFx) {
    commands.entity(active.pacman).despawn();
    for ghost in &active.ghost_entities {
        commands.entity(*ghost).despawn();
    }
    for ring in &active.rings {
        commands.entity(*ring).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_route, mouth_frame, nearest_neighbor_order, pacman_rotation, polyline_length,
        sample_route,
    };
    use bevy::prelude::{Vec2, Vec3};

    #[test]
    fn nearest_neighbor_order_visits_each_target_once() {
        let points = [
            Vec2::new(120.0, 40.0),
            Vec2::new(-80.0, 30.0),
            Vec2::new(20.0, -60.0),
        ];
        let order = nearest_neighbor_order(&points, Vec2::new(-200.0, 30.0));
        assert_eq!(order, vec![1, 2, 0]);
    }

    #[test]
    fn sample_route_clamps_to_the_end() {
        let route = [Vec2::ZERO, Vec2::new(100.0, 0.0), Vec2::new(100.0, 80.0)];
        let (position, direction) = sample_route(&route, 140.0);
        assert_eq!(position, Vec2::new(100.0, 40.0));
        assert_eq!(direction, Vec2::Y);
        assert_eq!(sample_route(&route, 10_000.0).0, Vec2::new(100.0, 80.0));
    }

    #[test]
    fn route_starts_near_the_target_cluster_and_ends_shortly_after() {
        let points = [Vec2::new(700.0, 120.0), Vec2::new(-300.0, 80.0)];
        let route = build_route(&points);
        let start = *route.first().unwrap();
        let last = *route.last().unwrap();
        assert!(start.distance(points[1]) < 120.0);
        assert!(last.distance(points[0]) < 120.0);
    }

    #[test]
    fn mouth_cycles_between_closed_and_open() {
        let frames = (0..4)
            .map(|step| mouth_frame(step as f32 * 0.065))
            .collect::<Vec<_>>();
        assert_eq!(frames, vec![1, 0, 2, 0]);
    }

    #[test]
    fn route_length_sums_segments() {
        let route = [Vec2::ZERO, Vec2::new(3.0, 4.0), Vec2::new(3.0, 14.0)];
        assert!((polyline_length(&route) - 15.0).abs() < 1e-5);
    }

    #[test]
    fn pacman_mouth_faces_the_travel_direction() {
        let transform = pacman_rotation(Vec2::X);
        let mouth = transform * Vec3::NEG_Y;
        assert!(mouth.x > 0.99, "mouth should face right, got {mouth:?}");
    }
}
