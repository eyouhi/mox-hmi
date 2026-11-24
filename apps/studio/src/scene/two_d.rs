use super::assets::{HmiFont, color_material, hex_color, panel_border};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use hmi_core::{
    BarDirection, EquipmentKind, HexColor, Screen, TextAlign, UiNode, Widget2D, design_to_world,
};
use std::collections::HashMap;

pub const DESIGN_SIZE: [u32; 2] = [1600, 900];

#[derive(Component)]
pub struct HmiScreenContent;

#[derive(Component, Debug, Clone)]
pub struct HmiNodeRoot {
    pub node_id: String,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotorAxis {
    #[allow(dead_code)]
    LocalX,
    LocalY,
    LocalZ,
}

impl RotorAxis {
    pub fn direction(self) -> Dir3 {
        match self {
            RotorAxis::LocalX => Dir3::X,
            RotorAxis::LocalY => Dir3::Y,
            RotorAxis::LocalZ => Dir3::Z,
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct HmiRotor {
    pub axis: RotorAxis,
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct HmiSpin {
    pub rad_per_sec: f32,
}

#[derive(Debug, Clone)]
pub struct GaugeVisual {
    pub entity: Entity,
    pub base_size: Vec2,
    pub direction: BarDirection,
}

#[derive(Debug, Clone)]
pub struct NodeVisual {
    pub root: Entity,
    pub materials: Vec<Handle<ColorMaterial>>,
    pub text: Option<Entity>,
    pub gauge: Option<GaugeVisual>,
    pub rotors: Vec<Entity>,
    pub pipe: Option<PipeVisual>,
}

#[derive(Debug, Clone)]
pub struct PipeVisual {
    pub segments: Vec<PipeSegmentVisual>,
    pub joints: Vec<Entity>,
}

#[derive(Debug, Clone, Copy)]
pub struct PipeSegmentVisual {
    pub entity: Entity,
    pub base_length: f32,
}

#[derive(Resource, Default)]
pub struct TwoDVisualIndex {
    pub nodes: HashMap<String, NodeVisual>,
}

pub fn spawn_two_d_screen(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    font: &HmiFont,
    screen: &Screen,
    index: &mut TwoDVisualIndex,
) {
    let Screen::TwoD { nodes, .. } = screen else {
        return;
    };
    for node in nodes {
        let visual = spawn_node(commands, meshes, materials, font, node);
        index.nodes.insert(node.id.clone(), visual);
    }
}

fn spawn_node(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    font: &HmiFont,
    node: &UiNode,
) -> NodeVisual {
    let center = [
        node.rect.x + node.rect.width * 0.5,
        node.rect.y + node.rect.height * 0.5,
    ];
    let translation = design_to_world(center, DESIGN_SIZE);
    let root = commands
        .spawn((
            HmiScreenContent,
            HmiNodeRoot {
                node_id: node.id.clone(),
            },
            Transform::from_xyz(translation[0], translation[1], f32::from(node.z as i16))
                .with_rotation(Quat::from_rotation_z(-node.rotation_deg.to_radians())),
            Visibility::Visible,
        ))
        .id();

    let mut visual = NodeVisual {
        root,
        materials: Vec::new(),
        text: None,
        gauge: None,
        rotors: Vec::new(),
        pipe: None,
    };

    let width = node.rect.width;
    let height = node.rect.height;
    commands
        .entity(root)
        .with_children(|parent| match &node.widget {
            Widget2D::Panel {
                fill,
                stroke,
                stroke_width,
            } => {
                spawn_rect(
                    parent,
                    meshes,
                    materials,
                    width,
                    height,
                    Vec2::ZERO,
                    hex_color(fill),
                    0.0,
                    &mut visual.materials,
                );
                if *stroke_width > 0.0 {
                    panel_border(
                        parent,
                        meshes,
                        materials,
                        width,
                        height,
                        *stroke_width,
                        hex_color(stroke),
                    );
                }
            }
            Widget2D::Label {
                text,
                font_size,
                color,
                align,
            } => {
                let (anchor, x) = match align {
                    TextAlign::Left => (Anchor::CENTER_LEFT, -width * 0.5 + 4.0),
                    TextAlign::Center => (Anchor::CENTER, 0.0),
                    TextAlign::Right => (Anchor::CENTER_RIGHT, width * 0.5 - 4.0),
                };
                visual.text = Some(spawn_text(
                    parent,
                    font,
                    text,
                    *font_size,
                    hex_color(color),
                    anchor,
                    Vec2::new(x, 0.0),
                    0.2,
                ));
            }
            Widget2D::ValueCard { label } => {
                spawn_card_surface(parent, meshes, materials, width, height);
                spawn_text(
                    parent,
                    font,
                    label,
                    14.0,
                    Color::srgb(0.53, 0.63, 0.67),
                    Anchor::TOP_LEFT,
                    Vec2::new(-width * 0.5 + 16.0, height * 0.5 - 14.0),
                    0.2,
                );
                visual.text = Some(spawn_text(
                    parent,
                    font,
                    "--",
                    30.0,
                    Color::srgb(0.91, 0.96, 0.96),
                    Anchor::CENTER_LEFT,
                    Vec2::new(-width * 0.5 + 16.0, -height * 0.5 + 30.0),
                    0.2,
                ));
            }
            Widget2D::StatusLamp { label } => {
                spawn_card_surface(parent, meshes, materials, width, height);
                let radius = (height * 0.22).clamp(8.0, 16.0);
                let lamp_handle = color_material(materials, Color::srgb(0.19, 0.26, 0.29));
                visual.materials.push(lamp_handle.clone());
                parent.spawn((
                    Mesh2d(meshes.add(Circle::new(radius))),
                    MeshMaterial2d(lamp_handle),
                    Transform::from_xyz(-width * 0.5 + 24.0, 0.0, 0.2),
                ));
                spawn_text(
                    parent,
                    font,
                    label,
                    17.0,
                    Color::srgb(0.86, 0.92, 0.92),
                    Anchor::CENTER_LEFT,
                    Vec2::new(-width * 0.5 + 46.0, 0.0),
                    0.2,
                );
            }
            Widget2D::BarGauge {
                min,
                max,
                direction,
            } => {
                spawn_card_surface(parent, meshes, materials, width, height);
                spawn_text(
                    parent,
                    font,
                    &node.name,
                    14.0,
                    Color::srgb(0.53, 0.63, 0.67),
                    Anchor::TOP_LEFT,
                    Vec2::new(-width * 0.5 + 14.0, height * 0.5 - 12.0),
                    0.2,
                );
                let track_color = color_material(materials, Color::srgb(0.07, 0.13, 0.16));
                let fill_color = color_material(materials, Color::srgb(0.22, 0.9, 0.83));
                let track_size = Vec2::new(width - 28.0, 16.0);
                let track_pos = Vec2::new(0.0, -height * 0.5 + 22.0);
                parent.spawn((
                    Mesh2d(meshes.add(Rectangle::new(track_size.x, track_size.y))),
                    MeshMaterial2d(track_color),
                    Transform::from_xyz(track_pos.x, track_pos.y, 0.2),
                ));
                let fill = parent
                    .spawn((
                        Mesh2d(meshes.add(Rectangle::new(track_size.x, track_size.y - 4.0))),
                        MeshMaterial2d(fill_color.clone()),
                        Transform::from_xyz(track_pos.x - track_size.x * 0.5, track_pos.y, 0.3),
                    ))
                    .id();
                visual.materials.push(fill_color);
                visual.gauge = Some(GaugeVisual {
                    entity: fill,
                    base_size: Vec2::new(track_size.x, track_size.y - 4.0),
                    direction: *direction,
                });
                let value_text = spawn_text(
                    parent,
                    font,
                    "--",
                    13.0,
                    Color::srgb(0.78, 0.9, 0.9),
                    Anchor::CENTER_RIGHT,
                    Vec2::new(width * 0.5 - 14.0, height * 0.5 - 12.0),
                    0.2,
                );
                visual.text = Some(value_text);
                let range = spawn_text(
                    parent,
                    font,
                    &format!("{min:.0} – {max:.0}"),
                    11.0,
                    Color::srgb(0.42, 0.53, 0.57),
                    Anchor::CENTER_LEFT,
                    Vec2::new(-width * 0.5 + 14.0, -height * 0.5 + 7.0),
                    0.2,
                );
                let _ = range;
            }
            Widget2D::Button { label, .. } => {
                let button_material = color_material(materials, Color::srgb(0.055, 0.25, 0.27));
                visual.materials.push(button_material.clone());
                parent.spawn((
                    Mesh2d(meshes.add(Rectangle::new(width, height))),
                    MeshMaterial2d(button_material),
                    Transform::from_xyz(0.0, 0.0, 0.05),
                ));
                panel_border(
                    parent,
                    meshes,
                    materials,
                    width,
                    height,
                    1.5,
                    Color::srgb(0.22, 0.9, 0.83),
                );
                spawn_text(
                    parent,
                    font,
                    label,
                    18.0,
                    Color::srgb(0.8, 0.98, 0.95),
                    Anchor::CENTER,
                    Vec2::ZERO,
                    0.2,
                );
            }
            Widget2D::Pipe {
                points,
                thickness,
                color,
            } => {
                visual.pipe = Some(spawn_pipe(
                    parent,
                    meshes,
                    materials,
                    width,
                    height,
                    points,
                    *thickness,
                    hex_color(color),
                    &mut visual.materials,
                ));
            }
            Widget2D::EquipmentSymbol { equipment, label } => {
                spawn_equipment(
                    parent,
                    meshes,
                    materials,
                    font,
                    width,
                    height,
                    *equipment,
                    label,
                    &mut visual.materials,
                    &mut visual.rotors,
                );
            }
        });

    visual
}

fn spawn_card_surface(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    width: f32,
    height: f32,
) {
    let surface = color_material(materials, Color::srgb(0.055, 0.105, 0.13));
    parent.spawn((
        Mesh2d(meshes.add(Rectangle::new(width, height))),
        MeshMaterial2d(surface),
        Transform::from_xyz(0.0, 0.0, 0.05),
    ));
    panel_border(
        parent,
        meshes,
        materials,
        width,
        height,
        1.0,
        Color::srgb(0.12, 0.22, 0.26),
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_rect(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    width: f32,
    height: f32,
    position: Vec2,
    color: Color,
    z: f32,
    material_list: &mut Vec<Handle<ColorMaterial>>,
) -> Entity {
    let material = color_material(materials, color);
    material_list.push(material.clone());
    parent
        .spawn((
            Mesh2d(meshes.add(Rectangle::new(width, height))),
            MeshMaterial2d(material),
            Transform::from_xyz(position.x, position.y, z),
        ))
        .id()
}

#[allow(clippy::too_many_arguments)]
fn spawn_text(
    parent: &mut ChildSpawnerCommands,
    font: &HmiFont,
    text: &str,
    size: f32,
    color: Color,
    anchor: Anchor,
    position: Vec2,
    z: f32,
) -> Entity {
    parent
        .spawn((
            Text2d::new(text.to_string()),
            font.text_font(size),
            TextColor(color),
            anchor,
            Transform::from_xyz(position.x, position.y, z),
        ))
        .id()
}

#[allow(clippy::too_many_arguments)]
fn spawn_pipe(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    width: f32,
    height: f32,
    points: &[[f32; 2]],
    thickness: f32,
    color: Color,
    material_list: &mut Vec<Handle<ColorMaterial>>,
) -> PipeVisual {
    let material = color_material(materials, color);
    material_list.push(material.clone());
    let mut segments = Vec::with_capacity(points.len().saturating_sub(1));
    for segment in points.windows(2) {
        let start = local_design_to_world(segment[0], width, height);
        let end = local_design_to_world(segment[1], width, height);
        let delta = end - start;
        let base_length = delta.length().max(0.001);
        let entity = parent
            .spawn((
                Mesh2d(meshes.add(Rectangle::new(base_length, thickness))),
                MeshMaterial2d(material.clone()),
                Transform::from_xyz((start.x + end.x) * 0.5, (start.y + end.y) * 0.5, 0.15)
                    .with_rotation(Quat::from_rotation_z(delta.y.atan2(delta.x))),
            ))
            .id();
        segments.push(PipeSegmentVisual {
            entity,
            base_length,
        });
    }
    let mut joints = Vec::with_capacity(points.len());
    for point in points {
        let local = local_design_to_world(*point, width, height);
        let entity = parent
            .spawn((
                Mesh2d(meshes.add(Circle::new(thickness * 0.5))),
                MeshMaterial2d(material.clone()),
                Transform::from_xyz(local.x, local.y, 0.16),
            ))
            .id();
        joints.push(entity);
    }
    PipeVisual { segments, joints }
}

fn local_design_to_world(point: [f32; 2], width: f32, height: f32) -> Vec2 {
    Vec2::new(point[0] - width * 0.5, height * 0.5 - point[1])
}

#[allow(clippy::too_many_arguments)]
fn spawn_equipment(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    font: &HmiFont,
    width: f32,
    height: f32,
    equipment: EquipmentKind,
    label: &str,
    material_list: &mut Vec<Handle<ColorMaterial>>,
    rotors: &mut Vec<Entity>,
) {
    let body_color = Color::srgb(0.09, 0.2, 0.23);
    let stroke_color = Color::srgb(0.18, 0.39, 0.44);
    let inner_color = Color::srgb(0.04, 0.1, 0.12);
    let body = color_material(materials, body_color);
    material_list.push(body.clone());
    parent.spawn((
        Mesh2d(meshes.add(Rectangle::new(width, height))),
        MeshMaterial2d(body),
        Transform::from_xyz(0.0, 0.0, 0.03),
    ));
    panel_border(parent, meshes, materials, width, height, 1.5, stroke_color);

    let label_y = -height * 0.5 + 16.0;
    spawn_text(
        parent,
        font,
        label,
        14.0,
        Color::srgb(0.78, 0.9, 0.91),
        Anchor::CENTER,
        Vec2::new(0.0, label_y),
        0.3,
    );

    match equipment {
        EquipmentKind::Fan => {
            let center = Vec2::new(0.0, 8.0);
            let radius = (height * 0.3).clamp(22.0, 42.0);
            parent.spawn((
                Mesh2d(meshes.add(Circle::new(radius))),
                MeshMaterial2d(color_material(materials, inner_color)),
                Transform::from_xyz(center.x, center.y, 0.1),
            ));
            spawn_fan_rotor(parent, meshes, materials, center, radius, rotors);
        }
        EquipmentKind::Pump => {
            let center = Vec2::new(0.0, 8.0);
            let radius = (height * 0.27).clamp(20.0, 38.0);
            parent.spawn((
                Mesh2d(meshes.add(Circle::new(radius))),
                MeshMaterial2d(color_material(materials, inner_color)),
                Transform::from_xyz(center.x, center.y, 0.1),
            ));
            let rotor = parent
                .spawn((
                    Transform::from_xyz(center.x, center.y, 0.2),
                    HmiRotor {
                        axis: RotorAxis::LocalZ,
                    },
                    HmiSpin::default(),
                    Visibility::Inherited,
                ))
                .id();
            rotors.push(rotor);
            parent
                .commands()
                .entity(rotor)
                .with_children(|rotor_parent| {
                    let blade = color_material(materials, Color::srgb(0.25, 0.72, 0.72));
                    for angle in [0.0_f32, 2.094, 4.188] {
                        rotor_parent.spawn((
                            Mesh2d(meshes.add(Rectangle::new(radius * 0.82, 5.0))),
                            MeshMaterial2d(blade.clone()),
                            Transform::from_xyz(0.0, 0.0, 0.0)
                                .with_rotation(Quat::from_rotation_z(angle)),
                        ));
                    }
                });
        }
        EquipmentKind::Ahu => {
            let radius = (height * 0.18).clamp(18.0, 28.0);
            for x in [-width * 0.23, width * 0.23] {
                let center = Vec2::new(x, 8.0);
                parent.spawn((
                    Mesh2d(meshes.add(Circle::new(radius))),
                    MeshMaterial2d(color_material(materials, inner_color)),
                    Transform::from_xyz(center.x, center.y, 0.1),
                ));
                spawn_fan_rotor(parent, meshes, materials, center, radius, rotors);
            }
            parent.spawn((
                Mesh2d(meshes.add(Rectangle::new(1.0, height * 0.55))),
                MeshMaterial2d(color_material(materials, stroke_color)),
                Transform::from_xyz(0.0, 6.0, 0.12),
            ));
        }
        EquipmentKind::Valve => {
            let size = (width.min(height) * 0.52).clamp(24.0, 44.0);
            parent.spawn((
                Mesh2d(meshes.add(Rectangle::new(size, size))),
                MeshMaterial2d(color_material(materials, Color::srgb(0.1, 0.31, 0.34))),
                Transform::from_xyz(0.0, 8.0, 0.12)
                    .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)),
            ));
            parent.spawn((
                Mesh2d(meshes.add(Rectangle::new(width * 0.7, 2.0))),
                MeshMaterial2d(color_material(materials, Color::srgb(0.34, 0.67, 0.69))),
                Transform::from_xyz(0.0, 8.0, 0.15),
            ));
        }
    }
}

fn spawn_fan_rotor(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    center: Vec2,
    radius: f32,
    rotors: &mut Vec<Entity>,
) {
    let rotor = parent
        .spawn((
            Transform::from_xyz(center.x, center.y, 0.2),
            HmiRotor {
                axis: RotorAxis::LocalZ,
            },
            HmiSpin::default(),
            Visibility::Inherited,
        ))
        .id();
    rotors.push(rotor);
    parent
        .commands()
        .entity(rotor)
        .with_children(|rotor_parent| {
            let blade = color_material(materials, Color::srgb(0.24, 0.62, 0.64));
            for angle in [0.0_f32, 2.094, 4.188] {
                rotor_parent.spawn((
                    Mesh2d(meshes.add(Rectangle::new(radius * 0.88, 5.0))),
                    MeshMaterial2d(blade.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0).with_rotation(Quat::from_rotation_z(angle)),
                ));
            }
            rotor_parent.spawn((
                Mesh2d(meshes.add(Circle::new(radius * 0.13))),
                MeshMaterial2d(color_material(materials, Color::srgb(0.37, 0.87, 0.82))),
                Transform::from_xyz(0.0, 0.0, 0.02),
            ));
        });
}

pub fn apply_gauge_progress(
    camera: &mut Query<&mut Transform>,
    gauge: &GaugeVisual,
    progress: f32,
) {
    if let Ok(mut transform) = camera.get_mut(gauge.entity) {
        let progress = progress.clamp(0.0, 1.0);
        match gauge.direction {
            BarDirection::Horizontal => {
                transform.scale.x = progress.max(0.001);
                transform.translation.x =
                    -gauge.base_size.x * 0.5 + gauge.base_size.x * transform.scale.x * 0.5;
            }
            BarDirection::Vertical => {
                transform.scale.y = progress.max(0.001);
                transform.translation.y =
                    -gauge.base_size.y * 0.5 + gauge.base_size.y * transform.scale.y * 0.5;
            }
        }
    }
}

pub fn apply_color_patch(
    materials: &mut Assets<ColorMaterial>,
    handles: &[Handle<ColorMaterial>],
    color: &HexColor,
) {
    let color = hex_color(color);
    for handle in handles {
        if let Some(mut material) = materials.get_mut(handle) {
            material.color = color;
        }
    }
}
