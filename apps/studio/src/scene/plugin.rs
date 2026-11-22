use super::assets::{HmiFont, hex_color, load_hmi_font};
use super::input::GamepadState;
use super::orbit::{OrbitCamera, orbit_system};
use super::three_d::{
    DeviceLabel3D, HmiProgressTarget, HoveredThreeD, SelectionRing, ThreeDVisualIndex,
    apply_material_patch, settle_glb, spawn_three_d_screen, tag_device_meshes,
};
use super::three_d_edit::{ThreeDEditState, three_d_edit};
use super::two_d::{
    HmiScreenContent, HmiSpin, TwoDVisualIndex, apply_color_patch, apply_gauge_progress,
    spawn_two_d_screen,
};
use bevy::camera::ScalingMode;
use bevy::camera::visibility::VisibilitySystems;
#[cfg(not(target_arch = "wasm32"))]
use bevy::camera::{ManualTextureViewHandle, RenderTarget};
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use hmi_core::{
    BindingTarget, ProjectDocument, Screen, VisualValue, format_tag_value, resolve_bindings,
};

#[cfg(not(target_arch = "wasm32"))]
pub const NATIVE_TEXTURE_VIEW: ManualTextureViewHandle = ManualTextureViewHandle(0x4d4f58);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenKind {
    TwoD,
    ThreeD,
}

#[derive(Resource, Default)]
pub struct CachedProject {
    pub project: Option<ProjectDocument>,
    pub screen_id: Option<String>,
    pub revision: u64,
    pub kind: Option<ScreenKind>,
}

impl CachedProject {
    pub fn is_three_d(&self) -> bool {
        self.kind == Some(ScreenKind::ThreeD)
    }

    pub fn is_two_d(&self) -> bool {
        self.kind == Some(ScreenKind::TwoD)
    }
}

#[derive(Resource)]
pub struct HmiCameras {
    pub two_d: Entity,
    pub three_d: Entity,
}

#[derive(Resource, Default)]
pub struct ScenePatchDirty(pub bool);

pub struct HmiScenePlugin;

impl Plugin for HmiScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin)
            .init_resource::<CachedProject>()
            .init_resource::<TwoDVisualIndex>()
            .init_resource::<ThreeDVisualIndex>()
            .init_resource::<ScenePatchDirty>()
            .init_resource::<OrbitCamera>()
            .init_resource::<GamepadState>()
            .init_resource::<super::controller_tuning::ControllerTuningState>()
            .init_resource::<ThreeDEditState>()
            .init_resource::<super::two_d_edit::TwoDEditState>()
            .init_resource::<super::two_d_edit::TwoDCameraState>()
            .init_resource::<super::three_d::ThreeDPointerState>()
            .init_resource::<HoveredThreeD>()
            .add_systems(
                Startup,
                (load_hmi_font, setup_cameras, super::announce_ready).chain(),
            )
            .add_systems(
                Update,
                (
                    (
                        crate::scene::tick_simulation,
                        sync_scene,
                        apply_visual_patches,
                        spin_rotors,
                        sync_active_camera,
                        sync_selection_rings,
                        settle_glb,
                        tag_device_meshes,
                        super::input::poll_gamepad,
                        super::input::pulse_gamepad_rumble,
                    )
                        .chain(),
                    (
                        super::input::gamepad_guide_controls,
                        super::controller_tuning::controller_tuning,
                        orbit_system,
                        super::three_d::pick_three_d,
                        super::three_d_edit::three_d_trackpad_edit,
                        three_d_edit,
                        super::two_d_edit::two_d_input,
                        super::input::gamepad_shortcuts,
                        super::two_d_edit::draw_two_d_overlay,
                        super::three_d_edit::draw_three_d_marquee,
                        super::three_d_edit::draw_three_d_hover,
                        super::two_d_edit::publish_view_state,
                    )
                        .chain(),
                )
                    .chain(),
            );
        app.add_systems(
            PostUpdate,
            publish_labels_3d
                .after(TransformSystems::Propagate)
                .after(VisibilitySystems::VisibilityPropagate),
        );
    }
}

fn setup_cameras(mut commands: Commands) {
    let two_d = commands
        .spawn((
            Camera2d,
            Camera {
                is_active: false,
                ..default()
            },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::AutoMin {
                    min_width: 1600.0,
                    min_height: 900.0,
                },
                ..OrthographicProjection::default_2d()
            }),
            Transform::from_xyz(0.0, 0.0, 1000.0),
        ))
        .id();
    let mut three_d_transform = Transform::default();
    OrbitCamera::default().apply(&mut three_d_transform);
    let three_d = commands
        .spawn((
            Camera3d::default(),
            Camera {
                is_active: false,
                ..default()
            },
            AmbientLight {
                color: Color::srgb(0.72, 0.82, 0.86),
                brightness: 105.0,
                ..default()
            },
            three_d_transform,
        ))
        .id();
    #[cfg(not(target_arch = "wasm32"))]
    {
        commands
            .entity(two_d)
            .insert(RenderTarget::TextureView(NATIVE_TEXTURE_VIEW));
        commands
            .entity(three_d)
            .insert(RenderTarget::TextureView(NATIVE_TEXTURE_VIEW));
    }
    commands.insert_resource(HmiCameras { two_d, three_d });
}

#[allow(clippy::too_many_arguments)]
fn sync_scene(
    mut commands: Commands,
    mut cached: ResMut<CachedProject>,
    mut dirty: ResMut<ScenePatchDirty>,
    mut clear_color: ResMut<ClearColor>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials_2d: ResMut<Assets<ColorMaterial>>,
    mut materials_3d: ResMut<Assets<StandardMaterial>>,
    font: Option<Res<HmiFont>>,
    asset_server: Res<AssetServer>,
    mut two_d_index: ResMut<TwoDVisualIndex>,
    mut three_d_index: ResMut<ThreeDVisualIndex>,
    content: Query<Entity, With<HmiScreenContent>>,
) {
    let revision = crate::bridge::DOCUMENT_REVISION();
    let screen_id = crate::bridge::ACTIVE_SCREEN();
    if cached.revision == revision && cached.screen_id == screen_id {
        return;
    }

    let project = crate::bridge::project();
    let active_screen = screen_id
        .clone()
        .or_else(|| Some(project.startup_screen.clone()))
        .and_then(|id| project.screen(&id).cloned());

    for entity in &content {
        commands.entity(entity).despawn();
    }
    two_d_index.nodes.clear();
    three_d_index.nodes.clear();
    if !matches!(active_screen.as_ref(), Some(Screen::ThreeD { .. })) {
        crate::bridge::clear_scene_labels_3d();
    }

    cached.project = Some(project.clone());
    cached.screen_id = screen_id;
    cached.revision = revision;
    cached.kind = active_screen.as_ref().map(|screen| {
        if screen.is_two_d() {
            ScreenKind::TwoD
        } else {
            ScreenKind::ThreeD
        }
    });

    let Some(screen) = active_screen else {
        *clear_color = ClearColor(Color::srgb(0.02, 0.04, 0.05));
        dirty.0 = true;
        return;
    };

    let background = match &screen {
        Screen::TwoD { background, .. } | Screen::ThreeD { background, .. } => background,
    };
    *clear_color = ClearColor(hex_color(background));

    if let Some(font) = font.as_deref() {
        spawn_two_d_screen(
            &mut commands,
            &mut meshes,
            &mut materials_2d,
            font,
            &screen,
            &mut two_d_index,
        );
    }
    spawn_three_d_screen(
        &mut commands,
        &mut meshes,
        &mut materials_3d,
        &asset_server,
        &screen,
        &mut three_d_index,
    );
    dirty.0 = true;
    info!("MOX HMI scene: screen={}", screen.id());
}

fn sync_active_camera(
    cached: Res<CachedProject>,
    cameras: Res<HmiCameras>,
    mut query: Query<(&mut Camera, Has<Camera2d>)>,
) {
    let is_2d = cached.is_two_d();
    for (mut camera, is_camera_2d) in &mut query {
        let entity_is_active = (is_2d && is_camera_2d) || (!is_2d && !is_camera_2d);
        camera.is_active = entity_is_active;
    }
    let _ = (&cameras.two_d, &cameras.three_d);
}

#[allow(clippy::too_many_arguments)]
fn apply_visual_patches(
    cached: Res<CachedProject>,
    mut dirty: ResMut<ScenePatchDirty>,
    mut last_tag_revision: Local<u64>,
    mut materials_2d: ResMut<Assets<ColorMaterial>>,
    mut materials_3d: ResMut<Assets<StandardMaterial>>,
    two_d_index: Res<TwoDVisualIndex>,
    three_d_index: Res<ThreeDVisualIndex>,
    mut text_query: Query<&mut Text2d>,
    mut visibility_query: Query<&mut Visibility>,
    mut transform_query: Query<&mut Transform>,
    progress_targets: Query<&HmiProgressTarget>,
    mut spin_query: Query<&mut HmiSpin>,
    mut label_query: Query<&mut DeviceLabel3D>,
) {
    let tag_revision = crate::bridge::TAG_REVISION();
    if !dirty.0 && tag_revision == *last_tag_revision {
        return;
    }
    *last_tag_revision = tag_revision;
    dirty.0 = false;

    let Some(project) = cached.project.as_ref() else {
        return;
    };
    let Some(screen_id) = cached.screen_id.as_ref() else {
        return;
    };
    let Some(screen) = project.screen(screen_id) else {
        return;
    };
    let tags = project.tag_map();
    let snapshot = crate::bridge::tag_snapshot();

    match screen {
        Screen::TwoD { nodes, .. } => {
            for node in nodes {
                let Some(visual) = two_d_index.nodes.get(&node.id) else {
                    continue;
                };
                for patch in resolve_bindings(&node.id, &node.bindings, &tags, &snapshot) {
                    match patch.value {
                        VisualValue::Text(value) => {
                            if let Some(entity) = visual.text
                                && let Ok(mut text) = text_query.get_mut(entity)
                            {
                                text.0 = value;
                            }
                        }
                        VisualValue::Color(color) => {
                            apply_color_patch(&mut materials_2d, &visual.materials, &color);
                        }
                        VisualValue::Visible(visible) => {
                            if let Ok(mut visibility) = visibility_query.get_mut(visual.root) {
                                *visibility = if visible {
                                    Visibility::Visible
                                } else {
                                    Visibility::Hidden
                                };
                            }
                        }
                        VisualValue::Progress(progress) => {
                            if let Some(gauge) = &visual.gauge {
                                apply_gauge_progress(&mut transform_query, gauge, progress);
                            }
                            if matches!(node.widget, hmi_core::Widget2D::BarGauge { .. })
                                && let Some(entity) = visual.text
                                && let Ok(mut text) = text_query.get_mut(entity)
                            {
                                text.0 = progress_value_text(node, &tags, &snapshot, progress);
                            }
                        }
                        VisualValue::RotationSpeed(speed) => {
                            for rotor in &visual.rotors {
                                if let Ok(mut spin) = spin_query.get_mut(*rotor) {
                                    spin.rad_per_sec = speed;
                                }
                            }
                        }
                    }
                }
            }
        }
        Screen::ThreeD { nodes, .. } => {
            for node in nodes {
                let Some(visual) = three_d_index.nodes.get(&node.id) else {
                    continue;
                };
                for patch in resolve_bindings(&node.id, &node.bindings, &tags, &snapshot) {
                    match patch.value {
                        VisualValue::Text(value) => {
                            if let Ok(mut label) = label_query.get_mut(visual.root) {
                                label.value = value;
                            }
                        }
                        VisualValue::Color(color) => {
                            apply_material_patch(&mut materials_3d, &visual.materials, &color);
                        }
                        VisualValue::Visible(visible) => {
                            if let Ok(mut visibility) = visibility_query.get_mut(visual.root) {
                                *visibility = if visible {
                                    Visibility::Visible
                                } else {
                                    Visibility::Hidden
                                };
                            }
                        }
                        VisualValue::Progress(progress) => {
                            if let Some(entity) = visual.progress
                                && let Ok(target) = progress_targets.get(entity)
                                && let Ok(mut transform) = transform_query.get_mut(entity)
                            {
                                transform.rotation = target.base_rotation
                                    * Quat::from_rotation_y(
                                        progress.clamp(0.0, 1.0) * std::f32::consts::TAU,
                                    );
                            }
                        }
                        VisualValue::RotationSpeed(speed) => {
                            for rotor in &visual.rotors {
                                if let Ok(mut spin) = spin_query.get_mut(*rotor) {
                                    spin.rad_per_sec = speed;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn spin_rotors(
    time: Res<Time>,
    mut query: Query<(&super::two_d::HmiRotor, &HmiSpin, &mut Transform)>,
) {
    for (rotor, spin, mut transform) in &mut query {
        if spin.rad_per_sec.abs() > f32::EPSILON {
            transform
                .rotate_local_axis(rotor.axis.direction(), spin.rad_per_sec * time.delta_secs());
        }
    }
}

fn sync_selection_rings(
    cached: Res<CachedProject>,
    three_d_index: Res<ThreeDVisualIndex>,
    mut rings: Query<&mut Visibility, With<SelectionRing>>,
) {
    let selected = crate::bridge::selected_nodes();
    for mut visibility in &mut rings {
        *visibility = Visibility::Hidden;
    }
    if cached.is_three_d() {
        for node_id in selected {
            if let Some(visual) = three_d_index.nodes.get(&node_id)
                && let Ok(mut visibility) = rings.get_mut(visual.selection_ring)
            {
                *visibility = Visibility::Visible;
            }
        }
    }
}

fn progress_value_text(
    node: &hmi_core::UiNode,
    tags: &std::collections::BTreeMap<String, hmi_core::TagDefinition>,
    snapshot: &hmi_core::TagSnapshot,
    progress: f32,
) -> String {
    node.bindings
        .iter()
        .find(|binding| binding.target == BindingTarget::Progress)
        .and_then(|binding| {
            let tag = tags.get(&binding.tag_id)?;
            let value = snapshot.get(&binding.tag_id)?;
            Some(format_tag_value(value, tag.precision, &tag.unit))
        })
        .unwrap_or_else(|| format!("{:.0}%", progress.clamp(0.0, 1.0) * 100.0))
}

fn publish_labels_3d(
    time: Res<Time>,
    cached: Res<CachedProject>,
    mut accumulator: Local<f32>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    labels: Query<(&DeviceLabel3D, &GlobalTransform, &InheritedVisibility)>,
) {
    *accumulator += time.delta_secs();
    if *accumulator < 0.05 {
        return;
    }
    *accumulator = 0.0;
    if !cached.is_three_d() {
        crate::bridge::clear_scene_labels_3d();
        return;
    }
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Some(project) = cached.project.as_ref() else {
        return;
    };
    let screen_id = cached
        .screen_id
        .as_deref()
        .unwrap_or(project.startup_screen.as_str());
    let expected = match project.screen(screen_id) {
        Some(Screen::ThreeD { nodes, .. }) => nodes.len(),
        _ => 0,
    };
    let mut next = Vec::new();
    for (label, transform, visibility) in &labels {
        let anchor = transform.translation() + label.offset;
        let projected = camera.world_to_ndc(camera_transform, anchor);
        let (left_pct, top_pct, on_screen) = match projected {
            Some(ndc) => (
                (ndc.x * 0.5 + 0.5) * 100.0,
                (0.5 - ndc.y * 0.5) * 100.0,
                ndc.x >= -1.1
                    && ndc.x <= 1.1
                    && ndc.y >= -1.1
                    && ndc.y <= 1.1
                    && ndc.z >= 0.0
                    && ndc.z <= 1.0,
            ),
            None => (-10_000.0, -10_000.0, false),
        };
        let (point_left_pct, point_top_pct) = camera
            .world_to_ndc(camera_transform, transform.translation())
            .map(|ndc| ((ndc.x * 0.5 + 0.5) * 100.0, (0.5 - ndc.y * 0.5) * 100.0))
            .unwrap_or((-10_000.0, -10_000.0));
        next.push(crate::bridge::SceneLabel3D {
            node_id: label.node_id.clone(),
            title: label.title.clone(),
            value: label.value.clone(),
            left_pct,
            top_pct,
            point_left_pct,
            point_top_pct,
            visible: visibility.get() && on_screen,
        });
    }
    if next.len() < expected {
        return;
    }
    next.sort_by(|left, right| left.node_id.cmp(&right.node_id));
    crate::bridge::set_scene_labels_3d(next);
}
