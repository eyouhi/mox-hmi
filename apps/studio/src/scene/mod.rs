pub mod assets;
pub mod controller_tuning;
pub mod gamepad_haptics;
pub mod gamepad_menu;
pub mod input;
pub mod orbit;
pub mod plugin;
pub mod shader_compat;
pub mod snap_feedback;
pub mod three_d;
pub mod three_d_edit;
pub mod two_d;
pub mod two_d_edit;

use bevy::prelude::*;
#[cfg(target_arch = "wasm32")]
use plugin::HmiScenePlugin;
#[cfg(target_arch = "wasm32")]
use shader_compat::ShaderCompatPlugin;

#[derive(Default)]
pub(crate) struct SimulationAccumulator(f64);

#[cfg(target_arch = "wasm32")]
pub fn start_bevy() {
    wasm_bindgen_futures::spawn_local(async {
        let base = dioxus_cli_config::base_path().unwrap_or_default();
        let asset_path = format!("{base}/assets").trim_end_matches('/').to_string();
        App::new()
            .add_plugins(
                DefaultPlugins
                    .set(WindowPlugin {
                        primary_window: Some(Window {
                            canvas: Some("#bevy-canvas".into()),
                            fit_canvas_to_parent: true,
                            prevent_default_event_handling: false,
                            ..default()
                        }),
                        ..default()
                    })
                    .set(AssetPlugin {
                        file_path: asset_path,
                        meta_check: bevy::asset::AssetMetaCheck::Never,
                        ..default()
                    }),
            )
            .insert_resource(ClearColor(Color::srgb(0.025, 0.045, 0.06)))
            .add_plugins(ShaderCompatPlugin)
            .add_plugins(HmiScenePlugin)
            .run();
    });
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn announce_ready(mut done: Local<bool>) {
    if !*done {
        *done = true;
        crate::bridge::mark_engine_ready();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn announce_ready() {}

pub(crate) fn tick_simulation(time: Res<Time>, mut accumulator: Local<SimulationAccumulator>) {
    accumulator.0 += time.delta_secs_f64();
    let steps = (accumulator.0 / 0.25).floor() as u64;
    if steps > 0 {
        accumulator.0 -= steps as f64 * 0.25;
        crate::bridge::tick_tags(steps * 250);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn build_native_scene_app(
    render_creation: bevy::render::settings::RenderCreation,
    asset_path: String,
    width: u32,
    height: u32,
    scale: f32,
) -> App {
    use bevy::{
        asset::AssetMetaCheck,
        render::RenderPlugin,
        window::{ExitCondition, WindowPlugin, WindowResolution},
        winit::WinitPlugin,
    };

    let mut resolution = WindowResolution::new(width.max(1), height.max(1));
    resolution.set_scale_factor_override(Some(scale.max(0.01)));

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "MOX HMI Studio".to_string(),
                    resolution,
                    ..default()
                }),
                exit_condition: ExitCondition::DontExit,
                close_when_requested: false,
                ..default()
            })
            .set(RenderPlugin {
                render_creation,
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .set(AssetPlugin {
                file_path: asset_path,
                meta_check: AssetMetaCheck::Never,
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .insert_resource(ClearColor(Color::srgb(0.025, 0.045, 0.06)))
    .add_plugins(plugin::HmiScenePlugin);

    // Dioxus Native keeps its runtime guard and editor session on the UI thread.
    // Bevy must not dispatch systems that touch the bridge onto its compute pool.
    use bevy::ecs::schedule::{ScheduleLabel, SingleThreadedExecutor};
    for label in [
        bevy::app::PreStartup.intern(),
        bevy::app::Startup.intern(),
        bevy::app::PostStartup.intern(),
        bevy::app::First.intern(),
        bevy::app::PreUpdate.intern(),
        bevy::app::Update.intern(),
        bevy::app::PostUpdate.intern(),
        bevy::app::Last.intern(),
    ] {
        app.edit_schedule(label, |schedule| {
            schedule.set_executor(SingleThreadedExecutor::new());
        });
    }
    app
}
