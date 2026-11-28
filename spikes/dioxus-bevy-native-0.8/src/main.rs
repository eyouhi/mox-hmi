use anyrender::{AntiAliasing, PaintRef, PaintScene, RenderContext};
use bevy::{
    app::SubApps,
    asset::RenderAssetUsages,
    camera::RenderTarget,
    prelude::*,
    render::{
        RenderPlugin,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        renderer::{RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue},
        settings::RenderCreation,
        texture::ManualTextureView,
    },
    window::ExitCondition,
    winit::WinitPlugin,
};
use blitz_dom::{Widget, node::ComputedStyles};
use dioxus::prelude::*;
use dioxus_native::{CustomWidgetAttr, DeviceHandle};
use peniko::{
    Fill, ImageBrush, ImageSampler,
    kurbo::{Affine, Rect},
};
use std::{sync::mpsc, time::Instant};
use wgpu::{Texture, TextureView, TextureViewDescriptor};

const STYLE: dioxus::prelude::Asset = asset!("/assets/main.css");

fn main() {
    dioxus_native::launch(app);
}

fn app() -> Element {
    let widget = use_hook(|| CustomWidgetAttr::new(BevyWidget::new()));
    rsx! {
        document::Stylesheet { href: STYLE }
        main {
            header {
                h1 { "Dioxus Native 0.8 + Bevy 0.20" }
                p { "共享 wgpu 30 设备 · Bevy 渲染到纹理 · Dioxus blended widget" }
            }
            div { id: "canvas-container",
                object {
                    style: "display: block; width: 800px; height: 600px;",
                    "data": widget,
                }
            }
        }
    }
}

struct BevyWidget {
    runtime: Option<BevyRuntime>,
    error: Option<String>,
    resource: Option<anyrender::ResourceId>,
    texture: Option<Texture>,
}

impl BevyWidget {
    fn new() -> Self {
        Self {
            runtime: None,
            error: None,
            resource: None,
            texture: None,
        }
    }
}

impl Widget for BevyWidget {
    fn can_create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        let Some(context) = render_ctx.renderer_specific_context() else {
            self.error = Some("renderer returned no context".to_string());
            return;
        };
        let Ok(handle) = context.downcast::<DeviceHandle>() else {
            self.error = Some("renderer is not the wgpu backend".to_string());
            return;
        };
        match BevyRuntime::new(&handle) {
            Ok(runtime) => {
                self.texture = Some(runtime.texture.clone());
                self.runtime = Some(runtime);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn destroy_surfaces(&mut self) {
        self.runtime = None;
        self.texture = None;
        self.resource = None;
    }

    fn requires_redraw(&self) -> bool {
        true
    }

    fn paint(
        &mut self,
        render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        _scale: f64,
    ) -> anyrender::Scene {
        let width = width.max(1);
        let height = height.max(1);
        let mut scene = anyrender::Scene::new();

        let Some(runtime) = &mut self.runtime else {
            return scene;
        };
        if runtime.size != (width, height) {
            if let Some(resource) = self.resource.take() {
                render_ctx.unregister_resource(resource);
            }
            if let Err(error) = runtime.resize(width, height) {
                self.error = Some(error);
                return scene;
            }
            self.texture = Some(runtime.texture.clone());
        }
        if let Err(error) = runtime.frame(width, height) {
            self.error = Some(error);
            return scene;
        }

        if self.resource.is_none() {
            let Some(texture) = self.texture.clone() else {
                return scene;
            };
            match render_ctx.try_register_custom_resource(Box::new(texture)) {
                Ok(resource) => self.resource = Some(resource),
                Err(error) => {
                    self.error = Some(format!("register texture failed: {error:?}"));
                    return scene;
                }
            }
        }

        if let Some(resource) = self.resource {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                PaintRef::Resource(ImageBrush {
                    image: resource,
                    sampler: ImageSampler::default(),
                }),
                None,
                &Rect::from_origin_size((0.0, 0.0), (width as f64, height as f64)),
                AntiAliasing::Enabled,
            );
        }
        scene
    }

    fn intrinsic_sizes(&self) -> blitz_dom::IntrinsicSizes {
        blitz_dom::IntrinsicSizes {
            width: Some(960.0),
            height: Some(600.0),
            ratio: Some(1.6),
        }
    }
}

struct BevyRuntime {
    sub_apps: SubApps,
    texture: Texture,
    texture_view: TextureView,
    size: (u32, u32),
    started: Instant,
    input_tx: mpsc::Sender<InputCommand>,
}

#[derive(Debug)]
enum InputCommand {
    PointerMove { x: f64, y: f64 },
    PointerButton { button: MouseButton, pressed: bool },
    Wheel { x: f32, y: f32 },
}

impl BevyRuntime {
    fn new(handle: &DeviceHandle) -> std::result::Result<Self, String> {
        const INITIAL_WIDTH: u32 = 960;
        const INITIAL_HEIGHT: u32 = 600;

        let texture = create_texture(&handle.device, INITIAL_WIDTH, INITIAL_HEIGHT);
        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        let adapter_info = handle.adapter.get_info();
        let render_creation = RenderCreation::manual(
            RenderDevice::new(handle.device.clone()),
            RenderQueue::new(handle.queue.clone()),
            RenderAdapterInfo::new(adapter_info),
            RenderAdapter::new(handle.adapter.clone()),
            RenderInstance::new(handle.instance.clone()),
        );

        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation,
                    synchronous_pipeline_compilation: true,
                    ..default()
                })
                .disable::<WinitPlugin>(),
        );
        app.world_mut()
            .resource_mut::<bevy::render::texture::ManualTextureViews>()
            .insert(
                bevy::camera::ManualTextureViewHandle(0x4d4f58),
                ManualTextureView {
                    texture_view: texture_view.clone().into(),
                    size: UVec2::new(INITIAL_WIDTH, INITIAL_HEIGHT),
                    view_format: TextureFormat::Rgba8Unorm,
                },
            );
        app.insert_resource(ClearColor(Color::srgb(0.025, 0.045, 0.06)))
            .add_systems(Startup, setup_scene)
            .add_systems(Update, spin_cube);
        app.finish();
        app.cleanup();
        let sub_apps = std::mem::take(app.sub_apps_mut());
        let (input_tx, input_rx) = mpsc::channel();
        std::mem::forget(input_rx);

        Ok(Self {
            sub_apps,
            texture,
            texture_view,
            size: (INITIAL_WIDTH, INITIAL_HEIGHT),
            started: Instant::now(),
            input_tx,
        })
    }

    fn resize(&mut self, width: u32, height: u32) -> std::result::Result<(), String> {
        self.texture = create_texture(
            self.sub_apps
                .main
                .world()
                .resource::<RenderDevice>()
                .wgpu_device(),
            width,
            height,
        );
        self.texture_view = self.texture.create_view(&TextureViewDescriptor::default());
        if let Some(mut views) = self
            .sub_apps
            .main
            .world_mut()
            .get_resource_mut::<bevy::render::texture::ManualTextureViews>()
        {
            views.insert(
                bevy::camera::ManualTextureViewHandle(0x4d4f58),
                ManualTextureView {
                    texture_view: self.texture_view.clone().into(),
                    size: UVec2::new(width, height),
                    view_format: TextureFormat::Rgba8Unorm,
                },
            );
        }
        self.size = (width, height);
        Ok(())
    }

    fn frame(&mut self, width: u32, height: u32) -> std::result::Result<(), String> {
        if self.size != (width, height) {
            self.resize(width, height)?;
        }
        self.sub_apps.update();
        let device = self
            .sub_apps
            .main
            .world()
            .resource::<RenderDevice>()
            .wgpu_device();
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        let _ = self.started.elapsed();
        let _ = &self.input_tx;
        Ok(())
    }
}

fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mox-bevy-spike"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

#[derive(Component)]
struct Spinning;

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        RenderTarget::TextureView(bevy::camera::ManualTextureViewHandle(0x4d4f58)),
        Transform::from_xyz(4.0, 3.2, 5.0).looking_at(Vec3::new(0.0, 0.8, 0.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 4_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.8, 1.8, 1.8))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.72, 0.67),
            metallic: 0.15,
            perceptual_roughness: 0.3,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.9, 0.0),
        Spinning,
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.035, 0.07, 0.085),
            perceptual_roughness: 0.92,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}

fn spin_cube(time: Res<Time>, mut query: Query<&mut Transform, With<Spinning>>) {
    for mut transform in &mut query {
        transform.rotate_y(time.delta_secs() * 0.8);
        transform.rotate_x(time.delta_secs() * 0.25);
    }
}

const _: () = {
    fn assert_render_asset_usages(_: RenderAssetUsages) {}
};
