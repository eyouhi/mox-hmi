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

pub(crate) fn spawn_node(
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
    let casing = color_material(materials, Color::srgb(0.035, 0.09, 0.105));
    let highlight = color_material(materials, Color::srgb(0.27, 0.56, 0.59));
    let mut segments = Vec::with_capacity(points.len().saturating_sub(1));
    for segment in points.windows(2) {
        let start = local_design_to_world(segment[0], width, height);
        let end = local_design_to_world(segment[1], width, height);
        let delta = end - start;
        let base_length = delta.length().max(0.001);
        let entity = parent
            .spawn((
                Transform::from_xyz((start.x + end.x) * 0.5, (start.y + end.y) * 0.5, 0.15)
                    .with_rotation(Quat::from_rotation_z(delta.y.atan2(delta.x))),
                Visibility::Inherited,
            ))
            .id();
        parent
            .commands()
            .entity(entity)
            .with_children(|segment_parent| {
                segment_parent.spawn((
                    Mesh2d(meshes.add(Rectangle::new(base_length, thickness * 1.36))),
                    MeshMaterial2d(casing.clone()),
                    Transform::from_xyz(0.0, 0.0, -0.004),
                ));
                segment_parent.spawn((
                    Mesh2d(meshes.add(Rectangle::new(base_length, thickness))),
                    MeshMaterial2d(material.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0),
                ));
                segment_parent.spawn((
                    Mesh2d(meshes.add(Rectangle::new(
                        base_length * 0.96,
                        (thickness * 0.16).max(1.2),
                    ))),
                    MeshMaterial2d(highlight.clone()),
                    Transform::from_xyz(0.0, thickness * 0.22, 0.006),
                ));
            });
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
                Transform::from_xyz(local.x, local.y, 0.16),
                Visibility::Inherited,
            ))
            .id();
        parent
            .commands()
            .entity(entity)
            .with_children(|joint_parent| {
                joint_parent.spawn((
                    Mesh2d(meshes.add(Circle::new(thickness * 0.78))),
                    MeshMaterial2d(casing.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0),
                ));
                joint_parent.spawn((
                    Mesh2d(meshes.add(Circle::new(thickness * 0.55))),
                    MeshMaterial2d(material.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.005),
                ));
                joint_parent.spawn((
                    Mesh2d(meshes.add(Circle::new((thickness * 0.16).max(1.2)))),
                    MeshMaterial2d(highlight.clone()),
                    Transform::from_xyz(0.0, thickness * 0.16, 0.01),
                ));
            });
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
            let center = Vec2::new(0.0, height * 0.06);
            let radius = (height * 0.3).clamp(22.0, 42.0);
            let housing_width = width * 0.82;
            let housing_height = height * 0.68;
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                housing_width + 6.0,
                housing_height + 6.0,
                0.0,
                center.y,
                0.05,
                Color::srgb(0.05, 0.13, 0.15),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                housing_width,
                housing_height,
                0.0,
                center.y,
                0.06,
                Color::srgb(0.15, 0.34, 0.38),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                housing_width - 6.0,
                housing_height - 6.0,
                0.0,
                center.y,
                0.07,
                inner_color,
            );
            for (sx, sy) in [(-1.0_f32, 1.0_f32), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                spawn_detail_circle(
                    parent,
                    meshes,
                    materials,
                    1.9,
                    sx * (housing_width * 0.5 - 5.0),
                    center.y + sy * (housing_height * 0.5 - 5.0),
                    0.11,
                    Color::srgb(0.34, 0.66, 0.68),
                );
            }
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.22,
                height * 0.1,
                0.0,
                center.y + housing_height * 0.5 + height * 0.04,
                0.08,
                Color::srgb(0.16, 0.38, 0.42),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.28,
                height * 0.025,
                0.0,
                center.y + housing_height * 0.5 + height * 0.088,
                0.09,
                Color::srgb(0.34, 0.68, 0.7),
            );
            spawn_detail_ring(
                parent,
                meshes,
                materials,
                radius + 1.5,
                radius + 4.5,
                center.x,
                center.y,
                0.1,
                Color::srgb(0.24, 0.52, 0.55),
            );
            spawn_detail_circle(
                parent,
                meshes,
                materials,
                radius + 1.2,
                center.x,
                center.y,
                0.105,
                inner_color,
            );
            spawn_fan_rotor(parent, meshes, materials, center, radius, rotors);
            for x in [-width * 0.26, width * 0.26] {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.16,
                    height * 0.045,
                    x,
                    -height * 0.32,
                    0.08,
                    Color::srgb(0.13, 0.3, 0.33),
                );
            }
        }
        EquipmentKind::Pump => {
            let center = Vec2::new(-width * 0.13, height * 0.04);
            let radius = (height * 0.22).clamp(20.0, 36.0);
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.82,
                height * 0.075,
                0.0,
                -height * 0.32,
                0.07,
                Color::srgb(0.05, 0.14, 0.16),
            );
            for x in [-width * 0.31, width * 0.31] {
                spawn_detail_circle(
                    parent,
                    meshes,
                    materials,
                    2.0,
                    x,
                    -height * 0.32,
                    0.1,
                    Color::srgb(0.34, 0.66, 0.68),
                );
            }
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.07,
                height * 0.08,
                width * 0.1,
                height * 0.04,
                0.08,
                Color::srgb(0.18, 0.42, 0.45),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.26,
                height * 0.36,
                width * 0.29,
                height * 0.05,
                0.09,
                Color::srgb(0.1, 0.27, 0.3),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.08,
                height * 0.07,
                width * 0.29,
                height * 0.28,
                0.11,
                Color::srgb(0.17, 0.4, 0.43),
            );
            for index in 0..4 {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.2,
                    1.6,
                    width * 0.29,
                    -height * 0.06 + index as f32 * height * 0.065,
                    0.12,
                    Color::srgb(0.22, 0.52, 0.55),
                );
            }
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.17,
                height * 0.08,
                -width * 0.35,
                height * 0.04,
                0.08,
                Color::srgb(0.16, 0.38, 0.42),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.055,
                height * 0.2,
                -width * 0.44,
                height * 0.04,
                0.11,
                Color::srgb(0.32, 0.64, 0.66),
            );
            for y in [height * 0.11, -height * 0.03] {
                spawn_detail_circle(
                    parent,
                    meshes,
                    materials,
                    1.3,
                    -width * 0.44,
                    y,
                    0.13,
                    Color::srgb(0.06, 0.16, 0.18),
                );
            }
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.06,
                height * 0.14,
                center.x,
                height * 0.33,
                0.09,
                Color::srgb(0.16, 0.38, 0.42),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.12,
                height * 0.035,
                center.x,
                height * 0.42,
                0.11,
                Color::srgb(0.32, 0.64, 0.66),
            );
            spawn_detail_ring(
                parent,
                meshes,
                materials,
                radius + 1.2,
                radius + 4.0,
                center.x,
                center.y,
                0.1,
                Color::srgb(0.24, 0.52, 0.55),
            );
            spawn_detail_circle(
                parent,
                meshes,
                materials,
                radius + 1.0,
                center.x,
                center.y,
                0.105,
                inner_color,
            );
            spawn_fan_rotor(parent, meshes, materials, center, radius, rotors);
            spawn_detail_circle(
                parent,
                meshes,
                materials,
                4.5,
                center.x + radius * 0.8,
                height * 0.3,
                0.16,
                Color::srgb(0.62, 0.78, 0.78),
            );
            spawn_detail_rect_rot(
                parent,
                meshes,
                materials,
                1.4,
                7.0,
                center.x + radius * 0.8,
                height * 0.3,
                0.17,
                Color::srgb(0.12, 0.32, 0.35),
                -0.55,
            );
        }
        EquipmentKind::Ahu => {
            let radius = (height * 0.18).clamp(18.0, 28.0);
            let fan_y = height * 0.06;
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.96,
                height * 0.045,
                0.0,
                height * 0.42,
                0.06,
                Color::srgb(0.06, 0.15, 0.17),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.96,
                height * 0.03,
                0.0,
                -height * 0.32,
                0.06,
                Color::srgb(0.06, 0.15, 0.17),
            );
            for x in [-width * 0.38, width * 0.38] {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    1.6,
                    height * 0.48,
                    x,
                    height * 0.07,
                    0.07,
                    Color::srgb(0.13, 0.31, 0.34),
                );
            }
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                1.6,
                height * 0.55,
                0.0,
                6.0,
                0.12,
                stroke_color,
            );
            for x in [-width * 0.23, width * 0.23] {
                let center = Vec2::new(x, fan_y);
                spawn_detail_ring(
                    parent,
                    meshes,
                    materials,
                    radius + 1.5,
                    radius + 4.5,
                    center.x,
                    center.y,
                    0.1,
                    Color::srgb(0.24, 0.52, 0.55),
                );
                spawn_detail_circle(
                    parent,
                    meshes,
                    materials,
                    radius + 1.2,
                    center.x,
                    center.y,
                    0.105,
                    inner_color,
                );
                spawn_fan_rotor(parent, meshes, materials, center, radius, rotors);
                for (sx, sy) in [(-1.0_f32, 1.0_f32), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                    spawn_detail_circle(
                        parent,
                        meshes,
                        materials,
                        1.6,
                        center.x + sx * (radius + 7.0),
                        center.y + sy * (radius + 7.0),
                        0.12,
                        Color::srgb(0.32, 0.64, 0.66),
                    );
                }
            }
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.44,
                height * 0.17,
                0.0,
                -height * 0.245,
                0.08,
                Color::srgb(0.04, 0.11, 0.13),
            );
            for index in 0..5 {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.4,
                    height * 0.012,
                    0.0,
                    -height * 0.295 + index as f32 * height * 0.026,
                    0.1,
                    Color::srgb(0.2, 0.48, 0.51),
                );
            }
            for (sx, side) in [(-1.0_f32, -1.0_f32), (1.0, 1.0)] {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.15,
                    height * 0.17,
                    sx * width * 0.33,
                    -height * 0.245,
                    0.08,
                    Color::srgb(0.07, 0.18, 0.2),
                );
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.018,
                    height * 0.04,
                    sx * width * 0.33 + side * width * 0.045,
                    -height * 0.245,
                    0.1,
                    Color::srgb(0.38, 0.72, 0.74),
                );
            }
            for x in [-width * 0.3, width * 0.3] {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.14,
                    height * 0.045,
                    x,
                    height * 0.465,
                    0.08,
                    Color::srgb(0.15, 0.35, 0.39),
                );
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.1,
                    height * 0.018,
                    x,
                    height * 0.465,
                    0.1,
                    Color::srgb(0.04, 0.11, 0.13),
                );
            }
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.16,
                height * 0.115,
                width * 0.36,
                height * 0.255,
                0.09,
                Color::srgb(0.035, 0.095, 0.115),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.11,
                height * 0.045,
                width * 0.36,
                height * 0.285,
                0.11,
                Color::srgb(0.13, 0.43, 0.46),
            );
            for (index, color) in [
                Color::srgb(0.2, 0.92, 0.8),
                Color::srgb(0.94, 0.68, 0.2),
                Color::srgb(0.26, 0.72, 0.95),
            ]
            .into_iter()
            .enumerate()
            {
                spawn_detail_circle(
                    parent,
                    meshes,
                    materials,
                    (height * 0.008).clamp(1.5, 2.4),
                    width * 0.32 + index as f32 * width * 0.04,
                    height * 0.225,
                    0.12,
                    color,
                );
            }
        }
        EquipmentKind::Valve => {
            let size = (height * 0.36).clamp(22.0, 40.0);
            let center_y = height * 0.02;
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.34,
                height * 0.1,
                -width * 0.3,
                center_y,
                0.08,
                Color::srgb(0.16, 0.38, 0.42),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.34,
                height * 0.1,
                width * 0.3,
                center_y,
                0.08,
                Color::srgb(0.16, 0.38, 0.42),
            );
            for x in [-width * 0.44, width * 0.44] {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.055,
                    height * 0.2,
                    x,
                    center_y,
                    0.11,
                    Color::srgb(0.32, 0.64, 0.66),
                );
                for y in [center_y + height * 0.065, center_y - height * 0.065] {
                    spawn_detail_circle(
                        parent,
                        meshes,
                        materials,
                        1.2,
                        x,
                        y,
                        0.13,
                        Color::srgb(0.06, 0.16, 0.18),
                    );
                }
            }
            spawn_detail_rect_rot(
                parent,
                meshes,
                materials,
                size,
                size,
                0.0,
                center_y,
                0.12,
                Color::srgb(0.1, 0.31, 0.34),
                std::f32::consts::FRAC_PI_4,
            );
            spawn_detail_rect_rot(
                parent,
                meshes,
                materials,
                size * 0.56,
                size * 0.56,
                0.0,
                center_y,
                0.135,
                Color::srgb(0.14, 0.4, 0.43),
                std::f32::consts::FRAC_PI_4,
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.03,
                height * 0.16,
                0.0,
                center_y + size * 0.55,
                0.14,
                Color::srgb(0.32, 0.64, 0.66),
            );
            let wheel_center = Vec2::new(0.0, center_y + size * 0.55 + height * 0.1);
            spawn_detail_ring(
                parent,
                meshes,
                materials,
                (width * 0.045).max(3.8),
                (width * 0.075).max(6.4),
                wheel_center.x,
                wheel_center.y,
                0.16,
                Color::srgb(0.34, 0.82, 0.8),
            );
            for angle in [0.0_f32, 1.047, 2.094] {
                spawn_detail_rect_rot(
                    parent,
                    meshes,
                    materials,
                    (width * 0.15).max(12.0),
                    1.6,
                    wheel_center.x,
                    wheel_center.y,
                    0.155,
                    Color::srgb(0.24, 0.62, 0.64),
                    angle,
                );
            }
            spawn_detail_circle(
                parent,
                meshes,
                materials,
                2.2,
                wheel_center.x,
                wheel_center.y,
                0.17,
                Color::srgb(0.37, 0.84, 0.8),
            );
        }
        EquipmentKind::Chiller => {
            let center_y = height * 0.06;
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.82,
                height * 0.52,
                0.0,
                center_y,
                0.1,
                Color::srgb(0.035, 0.09, 0.105),
            );
            for index in 0..5 {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.58,
                    height * 0.018,
                    -width * 0.04,
                    -height * 0.17 + index as f32 * height * 0.052,
                    0.14,
                    Color::srgb(0.16, 0.4, 0.43),
                );
            }
            let radius = (height * 0.13).clamp(14.0, 24.0);
            for x in [-width * 0.22, width * 0.22] {
                spawn_detail_circle(
                    parent,
                    meshes,
                    materials,
                    radius + 3.0,
                    x,
                    height * 0.12,
                    0.13,
                    Color::srgb(0.11, 0.3, 0.33),
                );
                spawn_detail_circle(
                    parent,
                    meshes,
                    materials,
                    radius,
                    x,
                    height * 0.12,
                    0.14,
                    Color::srgb(0.025, 0.07, 0.085),
                );
                spawn_fan_rotor(
                    parent,
                    meshes,
                    materials,
                    Vec2::new(x, height * 0.12),
                    radius,
                    rotors,
                );
            }
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.12,
                height * 0.16,
                width * 0.34,
                -height * 0.13,
                0.16,
                Color::srgb(0.03, 0.08, 0.09),
            );
            for (index, color) in [Color::srgb(0.2, 0.92, 0.8), Color::srgb(0.94, 0.68, 0.2)]
                .into_iter()
                .enumerate()
            {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.018,
                    height * 0.018,
                    width * 0.31 + index as f32 * width * 0.045,
                    -height * 0.1,
                    0.18,
                    color,
                );
            }
            for x in [-width * 0.47, width * 0.47] {
                spawn_detail_circle(
                    parent,
                    meshes,
                    materials,
                    height * 0.045,
                    x,
                    -height * 0.22,
                    0.16,
                    Color::srgb(0.3, 0.64, 0.65),
                );
            }
        }
        EquipmentKind::CoolingTower => {
            for (index, (body_width, body_height, y)) in [
                (width * 0.68, height * 0.16, -height * 0.16),
                (width * 0.56, height * 0.17, 0.0),
                (width * 0.46, height * 0.14, height * 0.15),
            ]
            .into_iter()
            .enumerate()
            {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    body_width,
                    body_height,
                    0.0,
                    y,
                    0.11 + index as f32 * 0.01,
                    Color::srgb(0.09, 0.25, 0.28),
                );
            }
            for index in 0..7 {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.58 - index as f32 * width * 0.018,
                    height * 0.012,
                    0.0,
                    -height * 0.22 + index as f32 * height * 0.036,
                    0.15,
                    Color::srgb(0.18, 0.46, 0.49),
                );
            }
            for x in [-width * 0.29, width * 0.29] {
                for y in [-height * 0.27, -height * 0.09] {
                    spawn_detail_rect(
                        parent,
                        meshes,
                        materials,
                        width * 0.045,
                        height * 0.2,
                        x,
                        y,
                        0.14,
                        Color::srgb(0.08, 0.2, 0.22),
                    );
                }
            }
            let fan_radius = (height * 0.14).clamp(12.0, 24.0);
            let fan_center = Vec2::new(0.0, height * 0.18);
            spawn_detail_circle(
                parent,
                meshes,
                materials,
                fan_radius + 4.0,
                fan_center.x,
                fan_center.y,
                0.16,
                Color::srgb(0.13, 0.36, 0.39),
            );
            spawn_detail_circle(
                parent,
                meshes,
                materials,
                fan_radius,
                fan_center.x,
                fan_center.y,
                0.17,
                Color::srgb(0.025, 0.07, 0.085),
            );
            spawn_fan_rotor(parent, meshes, materials, fan_center, fan_radius, rotors);
            for index in 0..4 {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.035,
                    height * 0.012,
                    -width * 0.36,
                    -height * 0.25 + index as f32 * height * 0.08,
                    0.18,
                    Color::srgb(0.24, 0.57, 0.59),
                );
            }
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.03,
                height * 0.46,
                -width * 0.36,
                -height * 0.08,
                0.17,
                Color::srgb(0.24, 0.57, 0.59),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.72,
                height * 0.035,
                0.0,
                -height * 0.31,
                0.2,
                Color::srgb(0.08, 0.2, 0.22),
            );
        }
        EquipmentKind::HeatExchanger => {
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.82,
                height * 0.48,
                0.0,
                4.0,
                0.1,
                Color::srgb(0.04, 0.11, 0.13),
            );
            for index in 0..7 {
                let x = -width * 0.28 + index as f32 * width * 0.0933;
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    (width * 0.024).clamp(3.0, 7.0),
                    height * 0.38,
                    x,
                    4.0,
                    0.14,
                    if index % 2 == 0 {
                        Color::srgb(0.18, 0.46, 0.49)
                    } else {
                        Color::srgb(0.13, 0.36, 0.39)
                    },
                );
            }
            for y in [-height * 0.2, height * 0.28] {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.76,
                    height * 0.035,
                    0.0,
                    y,
                    0.16,
                    Color::srgb(0.16, 0.4, 0.43),
                );
            }
            for x in [-width * 0.23, width * 0.23] {
                for y in [-height * 0.22, height * 0.22] {
                    spawn_detail_circle(
                        parent,
                        meshes,
                        materials,
                        height * 0.045,
                        x,
                        y,
                        0.18,
                        Color::srgb(0.34, 0.72, 0.73),
                    );
                    spawn_detail_circle(
                        parent,
                        meshes,
                        materials,
                        height * 0.026,
                        x,
                        y,
                        0.19,
                        Color::srgb(0.04, 0.11, 0.13),
                    );
                }
            }
            spawn_detail_circle(
                parent,
                meshes,
                materials,
                height * 0.055,
                width * 0.31,
                height * 0.24,
                0.2,
                Color::srgb(0.68, 0.84, 0.83),
            );
        }
        EquipmentKind::ControlCabinet => {
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.58,
                height * 0.64,
                0.0,
                height * 0.01,
                0.1,
                Color::srgb(0.055, 0.16, 0.18),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.62,
                height * 0.035,
                0.0,
                height * 0.34,
                0.13,
                Color::srgb(0.11, 0.3, 0.33),
            );
            for x in [-width * 0.14, width * 0.14] {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.24,
                    height * 0.54,
                    x,
                    0.0,
                    0.14,
                    Color::srgb(0.07, 0.2, 0.22),
                );
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.025,
                    height * 0.08,
                    x + width * 0.075,
                    -height * 0.01,
                    0.17,
                    Color::srgb(0.4, 0.74, 0.72),
                );
            }
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.25,
                height * 0.1,
                0.0,
                height * 0.2,
                0.17,
                Color::srgb(0.02, 0.055, 0.065),
            );
            for (index, color) in [
                Color::srgb(0.2, 0.92, 0.8),
                Color::srgb(0.94, 0.68, 0.2),
                Color::srgb(0.26, 0.72, 0.95),
            ]
            .into_iter()
            .enumerate()
            {
                spawn_detail_circle(
                    parent,
                    meshes,
                    materials,
                    (height * 0.012).clamp(2.5, 4.0),
                    -width * 0.1 + index as f32 * width * 0.1,
                    height * 0.09,
                    0.19,
                    color,
                );
            }
            for index in 0..5 {
                spawn_detail_rect(
                    parent,
                    meshes,
                    materials,
                    width * 0.24,
                    height * 0.008,
                    0.0,
                    -height * 0.28 + index as f32 * height * 0.023,
                    0.18,
                    Color::srgb(0.22, 0.51, 0.53),
                );
            }
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.66,
                height * 0.035,
                0.0,
                -height * 0.34,
                0.12,
                Color::srgb(0.04, 0.11, 0.13),
            );
            spawn_detail_rect(
                parent,
                meshes,
                materials,
                width * 0.035,
                height * 0.7,
                -width * 0.38,
                -height * 0.03,
                0.1,
                Color::srgb(0.16, 0.4, 0.43),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_detail_rect(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
    z: f32,
    color: Color,
) {
    parent.spawn((
        Mesh2d(meshes.add(Rectangle::new(width, height))),
        MeshMaterial2d(color_material(materials, color)),
        Transform::from_xyz(x, y, z),
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_detail_circle(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    radius: f32,
    x: f32,
    y: f32,
    z: f32,
    color: Color,
) {
    parent.spawn((
        Mesh2d(meshes.add(Circle::new(radius))),
        MeshMaterial2d(color_material(materials, color)),
        Transform::from_xyz(x, y, z),
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_detail_ring(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    inner_radius: f32,
    outer_radius: f32,
    x: f32,
    y: f32,
    z: f32,
    color: Color,
) {
    parent.spawn((
        Mesh2d(meshes.add(Annulus::new(inner_radius, outer_radius))),
        MeshMaterial2d(color_material(materials, color)),
        Transform::from_xyz(x, y, z),
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_detail_rect_rot(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
    z: f32,
    color: Color,
    rotation: f32,
) {
    parent.spawn((
        Mesh2d(meshes.add(Rectangle::new(width, height))),
        MeshMaterial2d(color_material(materials, color)),
        Transform::from_xyz(x, y, z).with_rotation(Quat::from_rotation_z(rotation)),
    ));
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
