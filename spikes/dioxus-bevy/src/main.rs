use bevy::{
    picking::{mesh_picking::MeshPickingPlugin, prelude::*},
    prelude::*,
};
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

const MAIN_CSS: dioxus::prelude::Asset = asset!("/assets/main.css");

static SELECTED_OBJECT: GlobalSignal<i32> = Signal::global(|| -1);
static CAMERA_COMMAND: GlobalSignal<u32> = Signal::global(|| 0);

fn main() {
    console_error_panic_hook::set_once();
    dioxus_logger::init(tracing::Level::INFO).expect("failed to initialize logger");
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let selected = SELECTED_OBJECT();
    let selected_label = if selected < 0 {
        "未选择".to_string()
    } else {
        format!("DEV-{:02}", selected + 1)
    };

    use_hook(|| {
        spawn_local(async {
            run_bevy_app().await;
        });
    });

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        div { class: "spike-shell",
            canvas { id: "bevy-canvas", class: "spike-canvas" }

            div { class: "spike-overlay",
                header { class: "spike-topbar",
                    div { class: "brand",
                        div { class: "brand-mark", "M" }
                        div {
                            div { class: "brand-title", "MOX HMI Studio" }
                            div { class: "brand-subtitle", "Dioxus + Bevy 0.20 · WebGPU spike" }
                        }
                    }
                    div { class: "topbar-spacer" }
                    div { class: "status-pill",
                        span { class: "status-dot" }
                        "WebGPU build · 双向状态桥"
                    }
                }

                aside { class: "left-panel",
                    div { class: "panel-kicker", "Bridge test" }
                    h2 { class: "panel-title", "Dioxus → Bevy" }
                    p { class: "panel-copy",
                        "点击按钮，把选中对象写入全局 Signal；Bevy 每帧读取并更新 3D 场景。"
                    }
                    div { class: "action-list",
                        button {
                            class: "action-button primary",
                            onclick: move |_| *SELECTED_OBJECT.write() = 0,
                            "选中 DEV-01"
                        }
                        button {
                            class: "action-button",
                            onclick: move |_| *SELECTED_OBJECT.write() = 1,
                            "选中 DEV-02"
                        }
                        button {
                            class: "action-button",
                            onclick: move |_| *CAMERA_COMMAND.write() += 1,
                            "切换相机机位"
                        }
                    }
                }

                aside { class: "right-panel",
                    div { class: "panel-kicker", "Bridge test" }
                    h2 { class: "panel-title", "Bevy → Dioxus" }
                    p { class: "panel-copy",
                        "在画布中点击任意设备，Bevy 会回写选中状态并刷新这个面板。"
                    }
                    div { class: "selection-card",
                        div { class: "panel-kicker", "Selected entity" }
                        div { class: "selection-id", "{selected_label}" }
                        div { class: "selection-note",
                            "点击 3D 画布或左侧按钮，两侧状态应保持同步。"
                        }
                    }
                }

                div { class: "bottom-hint",
                    strong { "Spike" }
                    " 验证 Dioxus 外壳、Bevy Canvas、WebGPU 渲染和双向通信"
                }
            }
        }
    }
}

#[derive(Component)]
struct SceneObject {
    id: i32,
    base_color: Color,
}

async fn run_bevy_app() {
    bevy::prelude::App::new()
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
                    meta_check: bevy::asset::AssetMetaCheck::Never,
                    ..default()
                }),
        )
        .add_plugins(MeshPickingPlugin)
        .insert_resource(ClearColor(Color::srgb(0.025, 0.045, 0.06)))
        .add_systems(Startup, setup_scene)
        .add_systems(
            Update,
            (sync_selection, handle_camera_command, gently_rotate_objects),
        )
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(7.5, 5.2, 9.5).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));

    commands.spawn((
        PointLight {
            intensity: 2_500_000.0,
            range: 40.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.0, 9.0, 5.0),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 1_500.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-4.0, 8.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(40.0, 40.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.035, 0.07, 0.085),
            perceptual_roughness: 0.92,
            ..default()
        })),
        Pickable::IGNORE,
        Transform::from_xyz(0.0, -0.01, 0.0),
    ));

    let devices = [
        (
            meshes.add(Cuboid::new(1.5, 1.5, 1.5)),
            Transform::from_xyz(-2.7, 0.9, 0.0),
            Color::srgb(0.12, 0.72, 0.67),
        ),
        (
            meshes.add(Cylinder::new(0.8, 1.8)),
            Transform::from_xyz(0.0, 0.9, 0.0),
            Color::srgb(0.16, 0.46, 0.78),
        ),
        (
            meshes.add(Sphere::new(0.9)),
            Transform::from_xyz(2.8, 1.0, 0.0),
            Color::srgb(0.92, 0.48, 0.18),
        ),
    ];

    let ring_mesh = meshes.add(Torus::new(1.35, 1.45));
    let ring_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.95, 0.87),
        emissive: LinearRgba::rgb(0.05, 0.8, 0.72),
        ..default()
    });

    for (id, (mesh, transform, base_color)) in devices.into_iter().enumerate() {
        commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color,
                    metallic: 0.18,
                    perceptual_roughness: 0.36,
                    ..default()
                })),
                transform,
                SceneObject {
                    id: id as i32,
                    base_color,
                },
            ))
            .observe(on_object_click);
    }

    commands
        .spawn((
            Mesh3d(ring_mesh),
            MeshMaterial3d(ring_material),
            Transform::from_xyz(-2.7, 0.05, 0.0)
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            SelectionRing,
        ))
        .insert(Pickable::IGNORE);

    info!("MOX HMI spike: Bevy scene initialized");
}

fn on_object_click(click: On<PointerClick>, objects: Query<&SceneObject>) {
    if let Ok(object) = objects.get(click.entity) {
        *SELECTED_OBJECT.write() = object.id;
        info!("MOX HMI spike: selected object {}", object.id);
    }
}

fn sync_selection(
    mut last_selected: Local<i32>,
    mut objects: Query<(&SceneObject, &mut MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ring: Query<&mut Transform, With<SelectionRing>>,
    mut ring_visible: Query<&mut Visibility, With<SelectionRing>>,
) {
    let selected = *SELECTED_OBJECT.read();
    if selected == *last_selected {
        return;
    }
    *last_selected = selected;

    for (object, material_handle) in &mut objects {
        if let Some(mut material) = materials.get_mut(&material_handle.0) {
            material.base_color = if object.id == selected {
                Color::srgb(0.26, 1.0, 0.91)
            } else {
                object.base_color
            };
        }
    }

    if selected >= 0 {
        let x = -2.7 + selected as f32 * 2.75;
        for mut transform in &mut ring {
            transform.translation.x = x;
        }
        for mut visibility in &mut ring_visible {
            *visibility = Visibility::Visible;
        }
    } else {
        for mut visibility in &mut ring_visible {
            *visibility = Visibility::Hidden;
        }
    }
}

fn handle_camera_command(mut last_command: Local<u32>, mut camera: Query<&mut Transform, With<Camera3d>>) {
    let command = *CAMERA_COMMAND.read();
    if command == *last_command {
        return;
    }
    *last_command = command;

    if let Ok(mut transform) = camera.single_mut() {
        let angle = command as f32 * 0.85;
        let distance = 12.0;
        transform.translation = Vec3::new(
            angle.sin() * distance,
            4.8 + (command % 2) as f32 * 1.8,
            angle.cos() * distance,
        );
        transform.look_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y);
    }
}

fn gently_rotate_objects(mut objects: Query<&mut Transform, With<SceneObject>>, time: Res<Time>) {
    for mut transform in &mut objects {
        transform.rotate_y(time.delta_secs() * 0.28);
    }
}

#[derive(Component)]
struct SelectionRing;
