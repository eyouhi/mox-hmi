use super::assets::{hex_color, standard_material};
use super::input;
use super::two_d::{HmiRotor, HmiScreenContent, HmiSpin, RotorAxis};
use bevy::asset::LoadState;
use bevy::gltf::prelude::GltfAssetLabel;
use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings, RayCastVisibility};
use bevy::picking::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::world_serialization::prelude::{WorldAsset, WorldAssetRoot};
use hmi_core::{DeviceModel, DeviceNode, HexColor, Screen};
use std::collections::HashMap;

pub(crate) const THREE_D_ROOM_SIZE: f32 = 32.0;

#[derive(Component, Debug, Clone)]
pub struct HmiPickable {
    pub node_id: String,
}

#[derive(Component, Debug, Clone)]
pub struct DeviceLabel3D {
    pub node_id: String,
    pub title: String,
    pub value: String,
    pub offset: Vec3,
}

#[derive(Component)]
pub struct HmiGlbPending {
    pub path: String,
    pub handle: Handle<WorldAsset>,
    pub placeholder: Entity,
}

#[derive(Component)]
pub struct SelectionRing;

#[derive(Component)]
pub struct HmiProgressTarget {
    pub base_rotation: Quat,
}

#[derive(Resource, Default)]
pub struct ThreeDPointerState {
    press_position: Option<Vec2>,
}

#[derive(Resource, Default)]
pub struct HoveredThreeD(pub Option<String>);

#[derive(Debug, Clone)]
pub struct DeviceVisual {
    pub root: Entity,
    pub materials: Vec<Handle<StandardMaterial>>,
    pub progress: Option<Entity>,
    pub rotors: Vec<Entity>,
    pub selection_ring: Entity,
}

struct PipeSpec<'a> {
    length: f32,
    diameter: f32,
    color: &'a HexColor,
}

#[derive(Resource, Default)]
pub struct ThreeDVisualIndex {
    pub nodes: HashMap<String, DeviceVisual>,
}

pub fn spawn_three_d_screen(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
    screen: &Screen,
    index: &mut ThreeDVisualIndex,
) {
    let Screen::ThreeD { nodes, .. } = screen else {
        return;
    };
    spawn_room(commands, meshes, materials);
    for node in nodes {
        if let Some(visual) = spawn_device(commands, meshes, materials, asset_server, node) {
            index.nodes.insert(node.id.clone(), visual);
        }
    }
}

fn spawn_room(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let floor = materials.add(StandardMaterial {
        base_color: Color::srgb(0.026, 0.065, 0.077),
        perceptual_roughness: 0.93,
        ..default()
    });
    commands.spawn((
        HmiScreenContent,
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(THREE_D_ROOM_SIZE, THREE_D_ROOM_SIZE),
            ),
        ),
        MeshMaterial3d(floor),
        Transform::from_xyz(0.0, -0.01, 0.0),
        Pickable::IGNORE,
    ));

    commands.spawn((
        HmiScreenContent,
        DirectionalLight {
            illuminance: 5_200.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-4.0, 9.0, 6.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        HmiScreenContent,
        PointLight {
            color: Color::srgb(0.42, 0.88, 0.92),
            intensity: 120_000.0,
            range: 18.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(3.0, 5.0, 3.0),
    ));
}

pub(crate) fn spawn_device(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
    node: &DeviceNode,
) -> Option<DeviceVisual> {
    let rotation = Quat::from_euler(
        EulerRot::XYZ,
        node.rotation_deg[0].to_radians(),
        node.rotation_deg[1].to_radians(),
        node.rotation_deg[2].to_radians(),
    );
    let root = commands
        .spawn((
            HmiScreenContent,
            Transform::from_translation(Vec3::from_array(node.position))
                .with_rotation(rotation)
                .with_scale(Vec3::from_array(node.scale)),
            if node.visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ))
        .id();
    commands.entity(root).insert(DeviceLabel3D {
        node_id: node.id.clone(),
        title: node.label.clone().unwrap_or_else(|| node.name.clone()),
        value: String::new(),
        offset: Vec3::new(0.0, label_height(&node.model) + 0.45, 0.0),
    });

    let mut visual = DeviceVisual {
        root,
        materials: Vec::new(),
        progress: None,
        rotors: Vec::new(),
        selection_ring: Entity::PLACEHOLDER,
    };

    commands
        .entity(root)
        .with_children(|parent| match &node.model {
            DeviceModel::Ahu { size, color } => {
                spawn_ahu(parent, meshes, materials, node, *size, color, &mut visual)
            }
            DeviceModel::Fan { size, color } => {
                spawn_fan(parent, meshes, materials, node, *size, color, &mut visual)
            }
            DeviceModel::Pump { size, color } => {
                spawn_pump(parent, meshes, materials, node, *size, color, &mut visual)
            }
            DeviceModel::Duct {
                length,
                diameter,
                color,
            }
            | DeviceModel::Pipe {
                length,
                diameter,
                color,
            } => spawn_pipe_3d(
                parent,
                meshes,
                materials,
                node,
                PipeSpec {
                    length: *length,
                    diameter: *diameter,
                    color,
                },
                &mut visual,
            ),
            DeviceModel::Valve { size, color } => {
                spawn_valve(parent, meshes, materials, node, *size, color, &mut visual)
            }
            DeviceModel::Chiller { size, color } => {
                spawn_chiller(parent, meshes, materials, node, *size, color, &mut visual)
            }
            DeviceModel::CoolingTower { size, color } => {
                spawn_cooling_tower(parent, meshes, materials, node, *size, color, &mut visual)
            }
            DeviceModel::HeatExchanger { size, color } => {
                spawn_heat_exchanger(parent, meshes, materials, node, *size, color, &mut visual)
            }
            DeviceModel::ControlCabinet { size, color } => {
                spawn_control_cabinet(parent, meshes, materials, node, *size, color, &mut visual)
            }
            DeviceModel::Glb { path } => {
                spawn_glb(parent, meshes, materials, asset_server, node, path)
            }
        });

    let ring_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.95, 0.87),
        emissive: LinearRgba::rgb(0.05, 0.8, 0.72),
        unlit: true,
        ..default()
    });
    let ring = commands
        .spawn((
            HmiScreenContent,
            Mesh3d(meshes.add(Torus::new(0.36, 0.43))),
            MeshMaterial3d(ring_material),
            Transform::from_xyz(node.position[0], 0.025, node.position[2]),
            Visibility::Hidden,
            SelectionRing,
            HmiPickable {
                node_id: node.id.clone(),
            },
            Pickable::IGNORE,
        ))
        .id();
    visual.selection_ring = ring;
    Some(visual)
}

fn label_height(model: &DeviceModel) -> f32 {
    match model {
        DeviceModel::Ahu { size, .. }
        | DeviceModel::Fan { size, .. }
        | DeviceModel::Pump { size, .. }
        | DeviceModel::Chiller { size, .. }
        | DeviceModel::CoolingTower { size, .. }
        | DeviceModel::HeatExchanger { size, .. }
        | DeviceModel::ControlCabinet { size, .. } => size[1],
        DeviceModel::Duct { diameter, .. } | DeviceModel::Pipe { diameter, .. } => *diameter,
        DeviceModel::Valve { size, .. } => *size,
        DeviceModel::Glb { .. } => 2.0,
    }
}

fn spawn_part(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    mesh: impl Into<Mesh>,
    material: Handle<StandardMaterial>,
    node_id: &str,
    transform: Transform,
    material_list: &mut Vec<Handle<StandardMaterial>>,
) -> Entity {
    material_list.push(material.clone());
    parent
        .spawn((
            Mesh3d(meshes.add(mesh.into())),
            MeshMaterial3d(material),
            transform,
            HmiPickable {
                node_id: node_id.to_string(),
            },
        ))
        .id()
}

fn spawn_ahu(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node: &DeviceNode,
    size: [f32; 3],
    color: &HexColor,
    visual: &mut DeviceVisual,
) {
    let base = standard_material(materials, hex_color(color));
    spawn_part(
        parent,
        meshes,
        Cuboid::new(size[0], size[1], size[2]),
        base,
        &node.id,
        Transform::default(),
        &mut visual.materials,
    );
    let trim = standard_material(materials, Color::srgb(0.08, 0.19, 0.22));
    spawn_part(
        parent,
        meshes,
        Cuboid::new(size[0] * 1.04, 0.13, size[2] * 1.04),
        trim.clone(),
        &node.id,
        Transform::from_xyz(0.0, size[1] * 0.5 + 0.08, 0.0),
        &mut visual.materials,
    );
    let fan_y = size[1] * 0.12;
    let fan_x = size[0] * 0.22;
    for x in [-fan_x, fan_x] {
        let housing = standard_material(materials, Color::srgb(0.07, 0.15, 0.17));
        spawn_part(
            parent,
            meshes,
            Cylinder::new(size[1] * 0.16, 0.12),
            housing,
            &node.id,
            Transform::from_xyz(x, fan_y, size[2] * 0.5 + 0.07)
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            &mut visual.materials,
        );
        let rotor = spawn_rotor_group(parent, x, fan_y, size[2] * 0.5 + 0.16);
        commands_add_blades(
            parent,
            meshes,
            materials,
            rotor,
            size[1] * 0.12,
            &node.id,
            RotorAxis::LocalZ,
        );
        visual.rotors.push(rotor);
    }
    for x in [-1.0_f32, 1.0] {
        for z in [-1.0_f32, 1.0] {
            spawn_part(
                parent,
                meshes,
                Cuboid::new(0.18, 0.18, 0.18),
                trim.clone(),
                &node.id,
                Transform::from_xyz(x * size[0] * 0.4, -size[1] * 0.5 - 0.03, z * size[2] * 0.38),
                &mut visual.materials,
            );
        }
    }
}

fn spawn_fan(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node: &DeviceNode,
    size: [f32; 3],
    color: &HexColor,
    visual: &mut DeviceVisual,
) {
    let housing = standard_material(materials, hex_color(color));
    spawn_part(
        parent,
        meshes,
        Cuboid::new(size[0], size[1] * 0.82, size[2]),
        housing,
        &node.id,
        Transform::default(),
        &mut visual.materials,
    );
    let inlet = standard_material(materials, Color::srgb(0.045, 0.11, 0.13));
    spawn_part(
        parent,
        meshes,
        Cylinder::new(size[1] * 0.32, 0.08),
        inlet,
        &node.id,
        Transform::from_xyz(0.0, 0.0, size[2] * 0.5 + 0.05)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        &mut visual.materials,
    );
    let rotor = spawn_rotor_group(parent, 0.0, 0.0, size[2] * 0.5 + 0.13);
    commands_add_blades(
        parent,
        meshes,
        materials,
        rotor,
        size[1] * 0.25,
        &node.id,
        RotorAxis::LocalZ,
    );
    visual.rotors.push(rotor);
    let foot = standard_material(materials, Color::srgb(0.07, 0.15, 0.17));
    spawn_part(
        parent,
        meshes,
        Cuboid::new(size[0] * 0.7, 0.16, size[2] * 0.8),
        foot,
        &node.id,
        Transform::from_xyz(0.0, -size[1] * 0.5, 0.0),
        &mut visual.materials,
    );
}

fn spawn_pump(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node: &DeviceNode,
    size: [f32; 3],
    color: &HexColor,
    visual: &mut DeviceVisual,
) {
    let base = standard_material(materials, Color::srgb(0.07, 0.15, 0.17));
    spawn_part(
        parent,
        meshes,
        Cuboid::new(size[0], 0.16, size[2]),
        base.clone(),
        &node.id,
        Transform::from_xyz(0.0, -size[1] * 0.5 + 0.08, 0.0),
        &mut visual.materials,
    );
    let body = standard_material(materials, hex_color(color));
    spawn_part(
        parent,
        meshes,
        Cylinder::new(size[0] * 0.34, size[1] * 0.8),
        body,
        &node.id,
        Transform::default(),
        &mut visual.materials,
    );
    let outlet = standard_material(materials, Color::srgb(0.16, 0.38, 0.42));
    spawn_part(
        parent,
        meshes,
        Cylinder::new(size[0] * 0.1, size[2] * 0.62),
        outlet,
        &node.id,
        Transform::from_xyz(size[0] * 0.43, 0.0, 0.0)
            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        &mut visual.materials,
    );
    let rotor = spawn_rotor_group(parent, 0.0, size[1] * 0.5, 0.0);
    commands_add_blades(
        parent,
        meshes,
        materials,
        rotor,
        size[0] * 0.3,
        &node.id,
        RotorAxis::LocalY,
    );
    visual.rotors.push(rotor);
}

fn spawn_pipe_3d(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node: &DeviceNode,
    spec: PipeSpec,
    visual: &mut DeviceVisual,
) {
    let PipeSpec {
        length,
        diameter,
        color,
    } = spec;
    let pipe_material = standard_material(materials, hex_color(color));
    spawn_part(
        parent,
        meshes,
        Cylinder::new(diameter * 0.5, length),
        pipe_material.clone(),
        &node.id,
        Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        &mut visual.materials,
    );
    let flange = standard_material(materials, Color::srgb(0.1, 0.27, 0.3));
    for x in [-length * 0.5, length * 0.5] {
        spawn_part(
            parent,
            meshes,
            Cylinder::new(diameter * 0.68, 0.08),
            flange.clone(),
            &node.id,
            Transform::from_xyz(x, 0.0, 0.0)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
            &mut visual.materials,
        );
    }
}

fn spawn_valve(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node: &DeviceNode,
    size: f32,
    color: &HexColor,
    visual: &mut DeviceVisual,
) {
    let body = standard_material(materials, hex_color(color));
    spawn_part(
        parent,
        meshes,
        Cuboid::new(size, size * 0.7, size),
        body,
        &node.id,
        Transform::default(),
        &mut visual.materials,
    );
    let stem = standard_material(materials, Color::srgb(0.16, 0.42, 0.45));
    spawn_part(
        parent,
        meshes,
        Cylinder::new(size * 0.08, size * 0.7),
        stem,
        &node.id,
        Transform::from_xyz(0.0, size * 0.65, 0.0),
        &mut visual.materials,
    );
    let wheel_material = standard_material(materials, Color::srgb(0.26, 0.82, 0.79));
    let wheel = parent
        .spawn((
            Mesh3d(meshes.add(Torus::new(size * 0.24, size * 0.33))),
            MeshMaterial3d(wheel_material.clone()),
            Transform::from_xyz(0.0, size * 1.0, 0.0),
            HmiPickable {
                node_id: node.id.clone(),
            },
            HmiProgressTarget {
                base_rotation: Quat::IDENTITY,
            },
        ))
        .id();
    visual.materials.push(wheel_material);
    visual.progress = Some(wheel);
}

fn spawn_detail_part(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node_id: &str,
    mesh: impl Into<Mesh>,
    color: Color,
    transform: Transform,
) -> Entity {
    parent
        .spawn((
            Mesh3d(meshes.add(mesh.into())),
            MeshMaterial3d(standard_material(materials, color)),
            transform,
            HmiPickable {
                node_id: node_id.to_string(),
            },
        ))
        .id()
}

fn spawn_chiller(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node: &DeviceNode,
    size: [f32; 3],
    color: &HexColor,
    visual: &mut DeviceVisual,
) {
    let [width, height, depth] = size;
    let body = standard_material(materials, hex_color(color));
    spawn_part(
        parent,
        meshes,
        Cuboid::new(width * 0.9, height * 0.64, depth * 0.82),
        body,
        &node.id,
        Transform::from_xyz(0.0, -height * 0.04, 0.0),
        &mut visual.materials,
    );

    let frame = Color::srgb(0.05, 0.12, 0.14);
    for z in [-depth * 0.32, depth * 0.32] {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cuboid::new(width * 0.98, height * 0.08, depth * 0.12),
            frame,
            Transform::from_xyz(0.0, -height * 0.46, z),
        );
    }

    let coil = Color::srgb(0.035, 0.09, 0.105);
    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cuboid::new(width * 0.58, height * 0.44, 0.035),
        coil,
        Transform::from_xyz(-width * 0.08, -height * 0.03, depth * 0.415),
    );
    for index in 0..6 {
        let y = -height * 0.21 + index as f32 * height * 0.075;
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cuboid::new(width * 0.54, height * 0.018, 0.025),
            Color::srgb(0.18, 0.43, 0.46),
            Transform::from_xyz(-width * 0.08, y, depth * 0.44),
        );
    }

    let compressor = Color::srgb(0.16, 0.38, 0.42);
    let compressor_radius = height * 0.095;
    let compressor_length = width * 0.28;
    for x in [-width * 0.23, width * 0.23] {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cylinder::new(compressor_radius, compressor_length),
            compressor,
            Transform::from_xyz(x, height * 0.29, -depth * 0.12)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        );
        for offset in [-compressor_length * 0.34, compressor_length * 0.34] {
            spawn_detail_part(
                parent,
                meshes,
                materials,
                &node.id,
                Torus::new(compressor_radius * 0.82, compressor_radius * 1.04),
                Color::srgb(0.08, 0.19, 0.21),
                Transform::from_xyz(x + offset, height * 0.29, -depth * 0.12)
                    .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
            );
        }
    }

    let fan_y = height * 0.4;
    for x in [-width * 0.23, width * 0.23] {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cylinder::new(height * 0.15, height * 0.06),
            Color::srgb(0.035, 0.085, 0.1),
            Transform::from_xyz(x, fan_y, depth * 0.18),
        );
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Torus::new(height * 0.13, height * 0.155),
            Color::srgb(0.22, 0.56, 0.58),
            Transform::from_xyz(x, fan_y + height * 0.035, depth * 0.18),
        );
        let rotor = spawn_rotor_group(parent, x, fan_y + height * 0.045, depth * 0.18);
        commands_add_blades(
            parent,
            meshes,
            materials,
            rotor,
            height * 0.11,
            &node.id,
            RotorAxis::LocalY,
        );
        visual.rotors.push(rotor);
    }

    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cuboid::new(width * 0.14, height * 0.19, 0.035),
        Color::srgb(0.035, 0.08, 0.095),
        Transform::from_xyz(width * 0.33, -height * 0.03, depth * 0.425),
    );
    for (index, light) in [
        Color::srgb(0.18, 0.9, 0.79),
        Color::srgb(0.95, 0.68, 0.2),
        Color::srgb(0.2, 0.8, 0.95),
    ]
    .into_iter()
    .enumerate()
    {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cuboid::new(width * 0.018, height * 0.018, 0.02),
            light,
            Transform::from_xyz(
                width * 0.3 + index as f32 * width * 0.026,
                height * 0.02,
                depth * 0.45,
            ),
        );
    }

    let pipe = Color::srgb(0.12, 0.3, 0.33);
    for side in [-1.0_f32, 1.0] {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cylinder::new(height * 0.055, width * 0.08),
            pipe,
            Transform::from_xyz(side * width * 0.46, -height * 0.16, 0.0)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        );
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Torus::new(height * 0.055, height * 0.075),
            Color::srgb(0.22, 0.55, 0.57),
            Transform::from_xyz(side * width * 0.5, -height * 0.16, 0.0)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        );
    }
}

fn spawn_cooling_tower(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node: &DeviceNode,
    size: [f32; 3],
    color: &HexColor,
    visual: &mut DeviceVisual,
) {
    let [width, height, depth] = size;
    let foot = width.min(depth);
    let shell_height = height * 0.54;
    let bottom_radius = foot * 0.38;
    let top_radius = foot * 0.3;
    let shell_center_y = -height * 0.07;
    let body = standard_material(materials, hex_color(color));
    spawn_part(
        parent,
        meshes,
        ConicalFrustum {
            radius_top: top_radius,
            radius_bottom: bottom_radius,
            height: shell_height,
        },
        body,
        &node.id,
        Transform::from_xyz(0.0, shell_center_y, 0.0),
        &mut visual.materials,
    );

    let frame = Color::srgb(0.045, 0.11, 0.13);
    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cuboid::new(width * 0.92, height * 0.08, depth * 0.92),
        frame,
        Transform::from_xyz(0.0, -height * 0.44, 0.0),
    );
    for x in [-width * 0.32, width * 0.32] {
        for z in [-depth * 0.32, depth * 0.32] {
            spawn_detail_part(
                parent,
                meshes,
                materials,
                &node.id,
                Cuboid::new(width * 0.055, height * 0.2, depth * 0.055),
                frame,
                Transform::from_xyz(x, -height * 0.34, z),
            );
        }
    }
    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cylinder::new(top_radius * 1.04, height * 0.06),
        Color::srgb(0.06, 0.15, 0.17),
        Transform::from_xyz(0.0, height * 0.22, 0.0),
    );
    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cylinder::new(top_radius * 0.76, height * 0.07),
        Color::srgb(0.025, 0.07, 0.085),
        Transform::from_xyz(0.0, height * 0.28, 0.0),
    );
    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Torus::new(top_radius * 0.65, top_radius * 0.8),
        Color::srgb(0.2, 0.55, 0.57),
        Transform::from_xyz(0.0, height * 0.32, 0.0),
    );
    let rotor = spawn_rotor_group(parent, 0.0, height * 0.34, 0.0);
    commands_add_blades(
        parent,
        meshes,
        materials,
        rotor,
        top_radius * 0.6,
        &node.id,
        RotorAxis::LocalY,
    );
    visual.rotors.push(rotor);

    for (y, radius) in [
        (-height * 0.28, bottom_radius * 0.94),
        (-height * 0.1, bottom_radius * 0.84),
        (height * 0.08, bottom_radius * 0.74),
    ] {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Torus::new(radius * 0.97, radius * 1.012),
            Color::srgb(0.13, 0.34, 0.37),
            Transform::from_xyz(0.0, y, 0.0),
        );
    }

    for z in [-depth * 0.07, depth * 0.07] {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cuboid::new(width * 0.035, height * 0.55, depth * 0.035),
            Color::srgb(0.24, 0.58, 0.6),
            Transform::from_xyz(-width * 0.4, -height * 0.04, z),
        );
    }
    for index in 0..6 {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cuboid::new(width * 0.045, height * 0.018, depth * 0.16),
            Color::srgb(0.24, 0.58, 0.6),
            Transform::from_xyz(
                -width * 0.4,
                -height * 0.28 + index as f32 * height * 0.09,
                0.0,
            ),
        );
    }

    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cylinder::new(height * 0.028, width * 0.12),
        Color::srgb(0.12, 0.31, 0.34),
        Transform::from_xyz(width * 0.45, -height * 0.31, 0.0)
            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
    );
}

fn spawn_heat_exchanger(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node: &DeviceNode,
    size: [f32; 3],
    color: &HexColor,
    visual: &mut DeviceVisual,
) {
    let [width, height, depth] = size;
    let base = Color::srgb(0.045, 0.11, 0.13);
    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cuboid::new(width * 0.88, height * 0.1, depth * 0.82),
        base,
        Transform::from_xyz(0.0, -height * 0.44, 0.0),
    );
    for x in [-width * 0.42, width * 0.42] {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cuboid::new(width * 0.06, height * 0.7, depth * 0.78),
            Color::srgb(0.07, 0.17, 0.19),
            Transform::from_xyz(x, 0.0, 0.0),
        );
    }
    for y in [-height * 0.33, height * 0.33] {
        for z in [-depth * 0.31, depth * 0.31] {
            spawn_detail_part(
                parent,
                meshes,
                materials,
                &node.id,
                Cuboid::new(width * 0.84, height * 0.055, depth * 0.07),
                Color::srgb(0.09, 0.22, 0.24),
                Transform::from_xyz(0.0, y, z),
            );
        }
    }

    let plate_count = 9;
    let plate_span = width * 0.56;
    let plate_material = standard_material(materials, hex_color(color));
    for index in 0..plate_count {
        let x = -plate_span * 0.5 + index as f32 * plate_span / (plate_count - 1) as f32;
        spawn_part(
            parent,
            meshes,
            Cuboid::new(width * 0.024, height * 0.58, depth * 0.64),
            plate_material.clone(),
            &node.id,
            Transform::from_xyz(x, 0.0, 0.0),
            &mut visual.materials,
        );
    }

    let rod = Color::srgb(0.32, 0.62, 0.63);
    for y in [-height * 0.31, height * 0.31] {
        for z in [-depth * 0.29, depth * 0.29] {
            spawn_detail_part(
                parent,
                meshes,
                materials,
                &node.id,
                Cylinder::new(height * 0.016, width * 0.92),
                rod,
                Transform::from_xyz(0.0, y, z)
                    .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
            );
        }
    }

    let nozzle = Color::srgb(0.14, 0.36, 0.39);
    let flange = Color::srgb(0.26, 0.62, 0.63);
    for x in [-width * 0.2, width * 0.2] {
        for y in [-height * 0.2, height * 0.2] {
            spawn_detail_part(
                parent,
                meshes,
                materials,
                &node.id,
                Cylinder::new(height * 0.06, height * 0.1),
                nozzle,
                Transform::from_xyz(x, y, depth * 0.42)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            );
            spawn_detail_part(
                parent,
                meshes,
                materials,
                &node.id,
                Torus::new(height * 0.065, height * 0.085),
                flange,
                Transform::from_xyz(x, y, depth * 0.49)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            );
        }
    }
    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cylinder::new(height * 0.07, height * 0.035),
        Color::srgb(0.7, 0.86, 0.85),
        Transform::from_xyz(width * 0.29, height * 0.2, depth * 0.445)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
    );
}

fn spawn_control_cabinet(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node: &DeviceNode,
    size: [f32; 3],
    color: &HexColor,
    visual: &mut DeviceVisual,
) {
    let [width, height, depth] = size;
    let dark = Color::srgb(0.04, 0.1, 0.12);
    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cuboid::new(width * 0.92, height * 0.07, depth * 0.88),
        dark,
        Transform::from_xyz(0.0, -height * 0.45, 0.0),
    );
    let body = standard_material(materials, hex_color(color));
    spawn_part(
        parent,
        meshes,
        Cuboid::new(width * 0.82, height * 0.82, depth * 0.74),
        body,
        &node.id,
        Transform::from_xyz(0.0, 0.02 * height, 0.0),
        &mut visual.materials,
    );
    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cuboid::new(width * 0.9, height * 0.05, depth * 0.82),
        Color::srgb(0.09, 0.22, 0.24),
        Transform::from_xyz(0.0, height * 0.44, 0.0),
    );

    let front = depth * 0.375;
    for x in [-width * 0.2, width * 0.2] {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cuboid::new(width * 0.36, height * 0.72, 0.025),
            Color::srgb(0.08, 0.2, 0.22),
            Transform::from_xyz(x, 0.01 * height, front),
        );
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cylinder::new(height * 0.015, height * 0.09),
            Color::srgb(0.42, 0.76, 0.74),
            Transform::from_xyz(x + width * 0.12, 0.0, front + 0.03)
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        );
    }

    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cuboid::new(width * 0.34, height * 0.14, 0.03),
        Color::srgb(0.02, 0.055, 0.065),
        Transform::from_xyz(0.0, height * 0.23, front + 0.018),
    );
    for (index, light) in [
        Color::srgb(0.2, 0.95, 0.8),
        Color::srgb(0.95, 0.7, 0.2),
        Color::srgb(0.26, 0.72, 0.95),
    ]
    .into_iter()
    .enumerate()
    {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cylinder::new(height * 0.014, 0.02),
            light,
            Transform::from_xyz(
                -width * 0.12 + index as f32 * width * 0.12,
                height * 0.08,
                front + 0.025,
            )
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        );
    }

    for index in 0..5 {
        spawn_detail_part(
            parent,
            meshes,
            materials,
            &node.id,
            Cuboid::new(width * 0.28, height * 0.012, 0.025),
            Color::srgb(0.19, 0.48, 0.5),
            Transform::from_xyz(
                0.0,
                -height * 0.28 + index as f32 * height * 0.032,
                front + 0.02,
            ),
        );
    }
    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cuboid::new(width * 0.08, height * 0.05, 0.02),
        Color::srgb(0.8, 0.63, 0.18),
        Transform::from_xyz(width * 0.28, -height * 0.28, front + 0.025),
    );
    spawn_detail_part(
        parent,
        meshes,
        materials,
        &node.id,
        Cylinder::new(width * 0.022, height * 0.78),
        Color::srgb(0.1, 0.25, 0.27),
        Transform::from_xyz(-width * 0.45, -0.02 * height, -depth * 0.18),
    );
}

fn spawn_glb(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
    node: &DeviceNode,
    path: &str,
) {
    let placeholder_material = standard_material(materials, Color::srgb(0.12, 0.31, 0.34));
    let placeholder = parent
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.6, 1.6, 1.6))),
            MeshMaterial3d(placeholder_material),
            Transform::default(),
            HmiPickable {
                node_id: node.id.clone(),
            },
        ))
        .id();
    let handle: Handle<WorldAsset> =
        asset_server.load(GltfAssetLabel::Scene(0).from_asset(path.to_string()));
    parent.spawn((
        WorldAssetRoot(handle.clone()),
        HmiGlbPending {
            path: path.to_string(),
            handle,
            placeholder,
        },
    ));
}

fn spawn_rotor_group(parent: &mut ChildSpawnerCommands, x: f32, y: f32, z: f32) -> Entity {
    parent
        .spawn((
            Transform::from_xyz(x, y, z),
            HmiRotor {
                axis: RotorAxis::LocalZ,
            },
            HmiSpin::default(),
            Visibility::Inherited,
        ))
        .id()
}

fn commands_add_blades(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    rotor: Entity,
    radius: f32,
    node_id: &str,
    axis: RotorAxis,
) {
    parent
        .commands()
        .entity(rotor)
        .insert(HmiRotor { axis })
        .with_children(|rotor_parent| {
            let blade = standard_material(materials, Color::srgb(0.18, 0.61, 0.62));
            for angle in [0.0_f32, 2.094, 4.188] {
                let (mesh, transform) = match axis {
                    RotorAxis::LocalZ => (
                        Cuboid::new(radius * 1.35, 0.1, 0.05),
                        Transform::from_xyz(0.0, 0.0, 0.0)
                            .with_rotation(Quat::from_rotation_z(angle)),
                    ),
                    RotorAxis::LocalY => (
                        Cuboid::new(radius * 1.35, 0.05, 0.1),
                        Transform::from_xyz(0.0, 0.0, 0.0)
                            .with_rotation(Quat::from_rotation_y(angle)),
                    ),
                    RotorAxis::LocalX => (
                        Cuboid::new(0.05, radius * 1.35, 0.1),
                        Transform::from_xyz(0.0, 0.0, 0.0)
                            .with_rotation(Quat::from_rotation_x(angle)),
                    ),
                };
                rotor_parent.spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(blade.clone()),
                    transform,
                    HmiPickable {
                        node_id: node_id.to_string(),
                    },
                ));
            }
        });
}

#[allow(clippy::too_many_arguments)]
pub fn pick_three_d(
    cached: Res<super::plugin::CachedProject>,
    cameras: Res<super::plugin::HmiCameras>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    pickables: Query<&HmiPickable>,
    mut ray_cast: MeshRayCast,
    mut pointer_state: ResMut<ThreeDPointerState>,
    mut hovered: ResMut<HoveredThreeD>,
) {
    if !cached.is_three_d() {
        pointer_state.press_position = None;
        hovered.0 = None;
        return;
    }
    let Ok(window) = window_query.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        hovered.0 = None;
        return;
    };
    let pointer_on_canvas = input::pointer_over_canvas(cursor);
    let hovered_node = if pointer_on_canvas {
        let Ok((camera, camera_transform)) = camera_query.get(cameras.three_d) else {
            hovered.0 = None;
            return;
        };
        let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else {
            hovered.0 = None;
            return;
        };
        let settings = MeshRayCastSettings {
            visibility: RayCastVisibility::VisibleInView,
            filter: &|entity| pickables.contains(entity),
            early_exit_test: &|_| true,
        };
        let hits = ray_cast.cast_ray(ray, &settings);
        hits.first()
            .and_then(|(entity, _)| pickables.get(*entity).ok())
            .map(|pickable| pickable.node_id.clone())
    } else {
        None
    };
    hovered.0 = hovered_node.clone();
    if pointer_on_canvas && mouse_buttons.just_pressed(MouseButton::Left) {
        pointer_state.press_position = Some(cursor);
    }
    if !mouse_buttons.just_released(MouseButton::Left) {
        return;
    }
    let Some(press_position) = pointer_state.press_position.take() else {
        return;
    };
    if !pointer_on_canvas {
        return;
    }
    if press_position.distance(cursor) > 6.0 {
        return;
    }
    if let Some(node_id) = hovered_node {
        if input::shift_pressed(&keyboard)
            || keyboard.pressed(KeyCode::SuperLeft)
            || keyboard.pressed(KeyCode::SuperRight)
            || keyboard.pressed(KeyCode::ControlLeft)
            || keyboard.pressed(KeyCode::ControlRight)
        {
            crate::bridge::toggle_node_selection(&node_id);
        } else {
            crate::bridge::select_node(Some(node_id));
        }
    } else if !input::shift_pressed(&keyboard)
        && !keyboard.pressed(KeyCode::SuperLeft)
        && !keyboard.pressed(KeyCode::SuperRight)
        && !keyboard.pressed(KeyCode::ControlLeft)
        && !keyboard.pressed(KeyCode::ControlRight)
    {
        crate::bridge::select_node(None);
    }
}

pub fn tag_device_meshes(
    mut commands: Commands,
    roots: Query<(Entity, &DeviceLabel3D)>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    pickables: Query<(), With<HmiPickable>>,
) {
    for (root, label) in &roots {
        tag_descendants(
            &mut commands,
            root,
            &label.node_id,
            &children,
            &meshes,
            &pickables,
        );
    }
}

fn tag_descendants(
    commands: &mut Commands,
    entity: Entity,
    node_id: &str,
    children: &Query<&Children>,
    meshes: &Query<(), With<Mesh3d>>,
    pickables: &Query<(), With<HmiPickable>>,
) {
    if meshes.contains(entity) && !pickables.contains(entity) {
        commands.entity(entity).insert(HmiPickable {
            node_id: node_id.to_string(),
        });
    }
    if let Ok(descendants) = children.get(entity) {
        for &child in descendants {
            tag_descendants(commands, child, node_id, children, meshes, pickables);
        }
    }
}

pub fn settle_glb(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    pending: Query<(Entity, &HmiGlbPending)>,
) {
    for (entity, glb) in &pending {
        match asset_server.load_state(&glb.handle) {
            LoadState::Loaded => {
                commands.entity(glb.placeholder).despawn();
                commands.entity(entity).remove::<HmiGlbPending>();
            }
            LoadState::Failed(error) => {
                warn!("MOX HMI GLB load failed: {} ({error})", glb.path);
                commands.entity(entity).remove::<HmiGlbPending>();
            }
            _ => {}
        }
    }
}

pub fn apply_material_patch(
    materials: &mut Assets<StandardMaterial>,
    handles: &[Handle<StandardMaterial>],
    color: &HexColor,
) {
    let color = hex_color(color);
    let linear = color.to_linear();
    for handle in handles {
        if let Some(mut material) = materials.get_mut(handle) {
            material.base_color = color;
            material.emissive =
                LinearRgba::rgb(linear.red * 0.45, linear.green * 0.45, linear.blue * 0.45);
        }
    }
}
