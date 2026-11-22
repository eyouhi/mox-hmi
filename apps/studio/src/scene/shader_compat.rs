use bevy::prelude::*;
use bevy::shader::{Shader, Source};

const PATCHED_SHADERS: &[(&str, &str)] = &[
    (
        "embedded://bevy_pbr/render/pbr_fragment.wesl",
        include_str!("safari_shaders/pbr_fragment.wesl"),
    ),
    (
        "embedded://bevy_pbr/render/pbr_prepass.wesl",
        include_str!("safari_shaders/pbr_prepass.wesl"),
    ),
    (
        "embedded://bevy_pbr/render/pbr_prepass_functions.wesl",
        include_str!("safari_shaders/pbr_prepass_functions.wesl"),
    ),
];

#[derive(Resource)]
struct PendingShaderPatch {
    shaders: Vec<(Handle<Shader>, &'static str)>,
}

pub struct ShaderCompatPlugin;

impl Plugin for ShaderCompatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, queue_shader_patches)
            .add_systems(Update, apply_shader_patches);
    }
}

fn queue_shader_patches(mut commands: Commands, asset_server: Res<AssetServer>) {
    let shaders = PATCHED_SHADERS
        .iter()
        .map(|(path, source)| (asset_server.load(*path), *source))
        .collect();
    commands.insert_resource(PendingShaderPatch { shaders });
}

fn apply_shader_patches(
    mut pending: ResMut<PendingShaderPatch>,
    mut shaders: ResMut<Assets<Shader>>,
) {
    pending.shaders.retain_mut(|(handle, source)| {
        let Some(mut shader) = shaders.get_mut(handle) else {
            return true;
        };
        if shader.source.as_str() != *source {
            shader.source = Source::Wesl((*source).into());
            info!(
                "MOX HMI applied Safari WGSL compatibility patch: {}",
                shader.path
            );
        }
        false
    });
}
