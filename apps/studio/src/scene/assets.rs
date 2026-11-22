use bevy::prelude::*;
use hmi_core::HexColor;

const FONT_BYTES: &[u8] = include_bytes!("../../public/assets/fonts/NotoSansSC-Subset.otf");

#[derive(Resource, Clone)]
pub struct HmiFont(pub Option<Handle<Font>>);

impl HmiFont {
    pub fn text_font(&self, size: f32) -> TextFont {
        let font = TextFont::from_font_size(size);
        match &self.0 {
            Some(handle) => font.with_font(handle.clone()),
            None => font,
        }
    }
}

pub fn load_hmi_font(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let handle = fonts.add(Font::from_bytes(FONT_BYTES.to_vec()));
    commands.insert_resource(HmiFont(Some(handle)));
}

pub fn hex_color(color: &HexColor) -> Color {
    match color.parse_rgb() {
        Some([red, green, blue]) => Color::srgb(
            f32::from(red) / 255.0,
            f32::from(green) / 255.0,
            f32::from(blue) / 255.0,
        ),
        None => Color::srgb(0.7, 0.8, 0.82),
    }
}

pub fn color_material(
    materials: &mut Assets<ColorMaterial>,
    color: Color,
) -> Handle<ColorMaterial> {
    materials.add(ColorMaterial::from_color(color))
}

pub fn standard_material(
    materials: &mut Assets<StandardMaterial>,
    color: Color,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        emissive: LinearRgba::rgb(0.0, 0.0, 0.0),
        metallic: 0.12,
        perceptual_roughness: 0.48,
        ..default()
    })
}

pub fn panel_border(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    width: f32,
    height: f32,
    thickness: f32,
    color: Color,
) {
    let thickness = thickness.max(0.5);
    let material = color_material(materials, color);
    for (size, x, y) in [
        (Vec2::new(width, thickness), 0.0, height * 0.5),
        (Vec2::new(width, thickness), 0.0, -height * 0.5),
        (Vec2::new(thickness, height), -width * 0.5, 0.0),
        (Vec2::new(thickness, height), width * 0.5, 0.0),
    ] {
        parent.spawn((
            Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(x, y, 0.02),
        ));
    }
}
