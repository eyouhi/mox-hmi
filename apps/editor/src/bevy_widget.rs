use anyrender::{AntiAliasing, PaintRef, PaintScene, RenderContext};
use bevy::{
    app::SubApps,
    input::{
        ButtonState,
        keyboard::{
            Key as BevyKey, KeyCode as BevyKeyCode, KeyboardFocusLost, KeyboardInput, NativeKey,
        },
        mouse::{MouseButton, MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel},
        touch::TouchPhase,
    },
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        renderer::{RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue},
        settings::RenderCreation,
        texture::{ManualTextureView, ManualTextureViews},
    },
    window::{CursorMoved, PrimaryWindow, Window},
};
use blitz_dom::{Widget, node::ComputedStyles};
use blitz_traits::events::{
    BlitzKeyEvent, BlitzPointerEvent, BlitzWheelDelta, KeyState, MouseEventButton, UiEvent,
};
use dioxus::prelude::*;
use dioxus_native::{CustomWidgetAttr, DeviceHandle};
use keyboard_types::{Code, Key, NamedKey};
use mox_hmi_studio::scene::{CanvasPinch, plugin::NATIVE_TEXTURE_VIEW};
use mox_hmi_ui::control::PreviewImage;
use peniko::{
    Fill, ImageBrush, ImageSampler,
    kurbo::{Affine, Rect},
};
use std::time::Instant;
use std::{
    cell::RefCell,
    sync::{Mutex, OnceLock},
};
use wgpu::{Texture, TextureView, TextureViewDescriptor};

static ASSET_PATH: OnceLock<String> = OnceLock::new();
static PINCH_DELTAS: Mutex<Vec<f32>> = Mutex::new(Vec::new());

struct PreviewCaptureContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: Texture,
    width: u32,
    height: u32,
}

thread_local! {
    static PREVIEW_CONTEXT: RefCell<Option<PreviewCaptureContext>> = const { RefCell::new(None) };
}

pub fn capture_preview() -> std::result::Result<PreviewImage, String> {
    PREVIEW_CONTEXT.with(|slot| {
        let context = slot.borrow();
        let Some(context) = context.as_ref() else {
            return Err("Bevy 画面尚未就绪".to_string());
        };
        context.capture()
    })
}

impl PreviewCaptureContext {
    fn capture(&self) -> std::result::Result<PreviewImage, String> {
        let width = self.width.max(1);
        let height = self.height.max(1);
        let unpadded_bytes_per_row = width * 4;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mox-bevy-preview-readback"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mox-bevy-preview-copy"),
            });
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|error| format!("等待 GPU 回读失败: {error:?}"))?;
        receiver
            .recv()
            .map_err(|_| "等待画面回读失败".to_string())?
            .map_err(|error| format!("画面回读失败: {error:?}"))?;
        let view = slice
            .get_mapped_range()
            .map_err(|error| format!("读取画面像素失败: {error:?}"))?;
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let start = (row * padded_bytes_per_row) as usize;
            pixels.extend_from_slice(&view[start..start + unpadded_bytes_per_row as usize]);
        }
        drop(view);
        buffer.unmap();

        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|error| format!("创建 PNG 编码器失败: {error}"))?;
            writer
                .write_image_data(&pixels)
                .map_err(|error| format!("编码 PNG 失败: {error}"))?;
            writer
                .finish()
                .map_err(|error| format!("完成 PNG 编码失败: {error}"))?;
        }
        Ok(PreviewImage { width, height, png })
    }
}

pub fn install_asset_path(path: String) {
    let _ = ASSET_PATH.set(path);
}

pub fn queue_pinch(delta: f32) {
    if delta.is_finite() && delta.abs() > f32::EPSILON {
        PINCH_DELTAS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(delta);
    }
}

pub fn viewport() -> Element {
    rsx! {
        NativeViewport {}
    }
}

#[component]
fn NativeViewport() -> Element {
    let widget = use_hook(|| CustomWidgetAttr::new(BevyWidget::new()));
    rsx! {
        object {
            class: "bevy-canvas",
            tabindex: "0",
            role: "img",
            aria_label: "HMI 组态画布",
            style: "display:block;width:100%;height:100%;",
            "data": widget,
        }
    }
}

fn asset_path() -> String {
    ASSET_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| "apps/studio/public/assets".to_string())
}

struct BevyWidget {
    runtime: Option<BevyRuntime>,
    texture: Option<Texture>,
    resource: Option<anyrender::ResourceId>,
    error: Option<String>,
    scale: f32,
}

impl BevyWidget {
    fn new() -> Self {
        Self {
            runtime: None,
            texture: None,
            resource: None,
            error: None,
            scale: 1.0,
        }
    }

    fn fail(&mut self, error: impl Into<String>) {
        let error = error.into();
        mox_hmi_ui::bridge::mark_engine_failed(error.clone());
        mox_hmi_ui::bridge::set_notice(mox_hmi_ui::bridge::NoticeLevel::Error, error.clone());
        self.error = Some(error);
    }

    fn pointer_move(&mut self, x: f32, y: f32) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        runtime.pointer_move(to_physical(x, y, self.scale), self.scale);
        mox_hmi_ui::bridge::set_pointer_on_native_canvas(true);
        mox_hmi_ui::bridge::set_text_entry_focused(false);
    }

    fn pointer_button(&mut self, event: &BlitzPointerEvent, pressed: bool) {
        let Some(button) = native_mouse_button(event.button) else {
            return;
        };
        let position = to_physical(event.element_x(), event.element_y(), self.scale);
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        runtime.pointer_move(position, self.scale);
        runtime.mouse_button(button, pressed);
        mox_hmi_ui::bridge::set_pointer_on_native_canvas(true);
        mox_hmi_ui::bridge::set_text_entry_focused(false);
    }

    fn pointer_leave(&mut self) {
        mox_hmi_ui::bridge::set_pointer_on_native_canvas(false);
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.pointer_leave();
        }
    }

    fn wheel(&mut self, delta: &BlitzWheelDelta, x: f32, y: f32) {
        let (unit, delta_x, delta_y) = match delta {
            BlitzWheelDelta::Lines(x, y) => (MouseScrollUnit::Line, *x as f32, *y as f32),
            BlitzWheelDelta::Pixels(x, y) => (MouseScrollUnit::Pixel, *x as f32, *y as f32),
        };
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        runtime.pointer_move(to_physical(x, y, self.scale), self.scale);
        runtime.mouse_wheel(unit, delta_x, delta_y);
    }

    fn key(&mut self, event: &BlitzKeyEvent) {
        let pressed = event.state == KeyState::Pressed;
        if pressed && !event.is_auto_repeating && !is_text_composition(event) {
            self.handle_shortcut(event);
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        runtime.keyboard(event, pressed);
    }

    fn handle_shortcut(&mut self, event: &BlitzKeyEvent) {
        let command = event.modifiers.meta() || event.modifiers.ctrl();
        if command
            && matches!(
                key_character(&event.key).as_deref(),
                Some("a" | "c" | "v" | "x" | "z" | "y")
            )
        {
            return;
        }
        let key = editor_key(&event.key);
        mox_hmi_ui::shortcuts::handle_editor_shortcut(key, command, event.modifiers.shift());
    }

    fn apple_keybinding(&mut self, action: &str) {
        let action = action.trim_end_matches(':');
        let (character, shift) = match action {
            "copy" => ("c", false),
            "paste" => ("v", false),
            "cut" => ("x", false),
            "selectAll" | "selectall" => ("a", false),
            "undo" => ("z", false),
            "redo" => ("z", true),
            _ => return,
        };
        mox_hmi_ui::shortcuts::handle_editor_shortcut(
            mox_hmi_ui::shortcuts::EditorKey::Character(character.to_string()),
            true,
            shift,
        );
    }

    fn drain_pinch(&mut self) {
        let deltas = {
            let mut pending = PINCH_DELTAS
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            std::mem::take(&mut *pending)
        };
        if deltas.is_empty() {
            return;
        }
        let delta = deltas.into_iter().sum::<f32>();
        if let Some(runtime) = self.runtime.as_mut() {
            runtime
                .sub_apps
                .main
                .world_mut()
                .write_message(CanvasPinch { delta });
        }
    }

    fn register_texture(&mut self, render_ctx: &mut dyn RenderContext) -> Option<()> {
        let texture = self.texture.clone()?;
        match render_ctx.try_register_custom_resource(Box::new(texture)) {
            Ok(resource) => {
                self.resource = Some(resource);
                Some(())
            }
            Err(error) => {
                self.fail(format!("注册 Bevy 纹理失败：{error:?}"));
                None
            }
        }
    }
}

impl Widget for BevyWidget {
    fn can_create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        let Some(context) = render_ctx.renderer_specific_context() else {
            self.fail("Dioxus Native 没有提供 wgpu 渲染上下文");
            return;
        };
        let Ok(handle) = context.downcast::<DeviceHandle>() else {
            self.fail("Dioxus Native 当前不是 wgpu 后端");
            return;
        };
        match BevyRuntime::new(&handle, asset_path()) {
            Ok(runtime) => {
                self.texture = Some(runtime.texture.clone());
                self.scale = runtime.scale;
                self.runtime = Some(runtime);
                self.error = None;
            }
            Err(error) => self.fail(error),
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

    fn handle_event(&mut self, event: &UiEvent) {
        match event {
            UiEvent::PointerMove(pointer) => {
                self.pointer_move(pointer.element_x(), pointer.element_y());
            }
            UiEvent::PointerDown(pointer) => self.pointer_button(pointer, true),
            UiEvent::PointerUp(pointer) => self.pointer_button(pointer, false),
            UiEvent::PointerCancel(_) | UiEvent::PointerLeave(_) => self.pointer_leave(),
            UiEvent::Wheel(wheel) => self.wheel(&wheel.delta, wheel.element_x(), wheel.element_y()),
            UiEvent::KeyDown(key) => self.key(key),
            UiEvent::KeyUp(key) => self.key(key),
            UiEvent::AppleStandardKeybinding(action) => self.apple_keybinding(action),
            UiEvent::Ime(_)
            | UiEvent::Paste(_)
            | UiEvent::DragOver(_)
            | UiEvent::DragLeave(_)
            | UiEvent::Drop(_) => {}
        }
    }

    fn paint(
        &mut self,
        render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        scale: f64,
    ) -> anyrender::Scene {
        let width = width.max(1);
        let height = height.max(1);
        self.scale = scale.max(0.01) as f32;
        let mut scene = anyrender::Scene::new();
        self.drain_pinch();

        let Some(runtime) = self.runtime.as_mut() else {
            return scene;
        };
        if runtime.size != (width, height) {
            if let Some(resource) = self.resource.take() {
                render_ctx.unregister_resource(resource);
            }
            if let Err(error) = runtime.resize(width, height) {
                self.fail(error);
                return scene;
            }
            self.texture = Some(runtime.texture.clone());
        }
        PREVIEW_CONTEXT.with(|slot| *slot.borrow_mut() = Some(runtime.capture_context()));
        let frame_result = runtime.frame();
        PREVIEW_CONTEXT.with(|slot| *slot.borrow_mut() = None);
        if let Err(error) = frame_result {
            self.fail(error);
            return scene;
        }
        if self.resource.is_none() && self.register_texture(render_ctx).is_none() {
            return scene;
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
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: Texture,
    texture_view: TextureView,
    window_entity: Entity,
    size: (u32, u32),
    scale: f32,
    cursor: Option<Vec2>,
    pressed_buttons: u8,
    last_state_sync: Instant,
    ready_reported: bool,
}

impl BevyRuntime {
    fn new(handle: &DeviceHandle, asset_path: String) -> std::result::Result<Self, String> {
        const INITIAL_WIDTH: u32 = 960;
        const INITIAL_HEIGHT: u32 = 600;
        const SCALE: f32 = 1.0;

        let texture = create_texture(&handle.device, INITIAL_WIDTH, INITIAL_HEIGHT);
        let texture_view = texture.create_view(&TextureViewDescriptor {
            format: Some(TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        });
        let adapter_info = handle.adapter.get_info();
        let render_creation = RenderCreation::manual(
            RenderDevice::new(handle.device.clone()),
            RenderQueue::new(handle.queue.clone()),
            RenderAdapterInfo::new(adapter_info),
            RenderAdapter::new(handle.adapter.clone()),
            RenderInstance::new(handle.instance.clone()),
        );

        let mut app = mox_hmi_studio::scene::build_native_scene_app(
            render_creation,
            asset_path,
            INITIAL_WIDTH,
            INITIAL_HEIGHT,
            SCALE,
        );
        app.finish();
        app.cleanup();
        let mut sub_apps = std::mem::take(app.sub_apps_mut());
        let window_entity = {
            let world = sub_apps.main.world_mut();
            let mut query = world.query_filtered::<Entity, With<PrimaryWindow>>();
            query
                .single(world)
                .map_err(|error| format!("Bevy 原生编辑器没有 PrimaryWindow 实体: {error}"))?
        };
        let mut runtime = Self {
            sub_apps,
            device: handle.device.clone(),
            queue: handle.queue.clone(),
            texture,
            texture_view,
            window_entity,
            size: (INITIAL_WIDTH, INITIAL_HEIGHT),
            scale: SCALE,
            cursor: None,
            pressed_buttons: 0,
            last_state_sync: Instant::now(),
            ready_reported: false,
        };
        runtime.resize(INITIAL_WIDTH, INITIAL_HEIGHT)?;
        Ok(runtime)
    }

    fn resize(&mut self, width: u32, height: u32) -> std::result::Result<(), String> {
        let width = width.max(1);
        let height = height.max(1);
        let device = self
            .sub_apps
            .main
            .world()
            .resource::<RenderDevice>()
            .wgpu_device();
        self.texture = create_texture(device, width, height);
        self.texture_view = self.texture.create_view(&TextureViewDescriptor {
            format: Some(TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        });

        let world = self.sub_apps.main.world_mut();
        if let Some(mut views) = world.get_resource_mut::<ManualTextureViews>() {
            views.insert(
                NATIVE_TEXTURE_VIEW,
                ManualTextureView {
                    texture_view: self.texture_view.clone().into(),
                    size: UVec2::new(width, height),
                    view_format: TextureFormat::Rgba8UnormSrgb,
                },
            );
        } else {
            return Err("Bevy 未初始化 ManualTextureViews".to_string());
        }
        if let Some(mut window) = world.get_mut::<Window>(self.window_entity) {
            window.resolution.set_scale_factor_override(Some(1.0));
            window.resolution.set_physical_resolution(width, height);
            if let Some(cursor) = self.cursor {
                window.set_physical_cursor_position(Some(cursor.as_dvec2()));
            }
        }
        self.size = (width, height);
        Ok(())
    }

    fn frame(&mut self) -> std::result::Result<(), String> {
        self.sub_apps.update();
        self.drain_control();
        if !self.ready_reported {
            self.ready_reported = true;
            mox_hmi_ui::bridge::mark_engine_ready();
        }
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
        if self.last_state_sync.elapsed() >= std::time::Duration::from_millis(250) {
            self.sync_control_state();
            self.last_state_sync = Instant::now();
        }
        Ok(())
    }

    fn capture_context(&self) -> PreviewCaptureContext {
        PreviewCaptureContext {
            device: self.device.clone(),
            queue: self.queue.clone(),
            texture: self.texture.clone(),
            width: self.size.0,
            height: self.size.1,
        }
    }

    fn drain_control(&self) {
        crate::control_native::with_installed(|control| control.process());
    }

    fn sync_control_state(&self) {
        crate::control_native::with_installed(|control| control.sync_state());
    }

    fn pointer_move(&mut self, position: Vec2, scale: f32) {
        self.scale = scale.max(0.01);
        let previous = self.cursor.replace(position);
        let delta = previous.map_or(Vec2::ZERO, |previous| position - previous);
        let world = self.sub_apps.main.world_mut();
        if let Some(mut window) = world.get_mut::<Window>(self.window_entity) {
            window.set_physical_cursor_position(Some(position.as_dvec2()));
        }
        world.write_message(CursorMoved {
            window: self.window_entity,
            position,
            delta: previous.map(|_| delta),
        });
        world.write_message(MouseMotion { delta });
    }

    fn mouse_button(&mut self, button: MouseButton, pressed: bool) {
        let mask = button_mask(button);
        if pressed {
            self.pressed_buttons |= mask;
        } else {
            self.pressed_buttons &= !mask;
        }
        self.sub_apps
            .main
            .world_mut()
            .write_message(MouseButtonInput {
                button,
                state: if pressed {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                },
                window: self.window_entity,
            });
    }

    fn mouse_wheel(&mut self, unit: MouseScrollUnit, x: f32, y: f32) {
        self.sub_apps.main.world_mut().write_message(MouseWheel {
            unit,
            x,
            y,
            window: self.window_entity,
            phase: TouchPhase::Moved,
        });
    }

    fn pointer_leave(&mut self) {
        self.cursor = None;
        for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
            if self.pressed_buttons & button_mask(button) != 0 {
                self.mouse_button(button, false);
            }
        }
        let world = self.sub_apps.main.world_mut();
        if let Some(mut window) = world.get_mut::<Window>(self.window_entity) {
            window.set_physical_cursor_position(None);
        }
        world.write_message(KeyboardFocusLost);
    }

    fn keyboard(&mut self, event: &BlitzKeyEvent, pressed: bool) {
        let Some(key_code) = native_key_code(event.code) else {
            return;
        };
        self.sub_apps.main.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: BevyKey::Unidentified(NativeKey::Unidentified),
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: event.text.as_ref().map(|text| text.to_string().into()),
            repeat: event.is_auto_repeating,
            window: self.window_entity,
        });
    }
}

fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mox-bevy-native-widget"),
        size: Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC,
        view_formats: &[TextureFormat::Rgba8UnormSrgb],
    })
}

fn to_physical(x: f32, y: f32, scale: f32) -> Vec2 {
    Vec2::new(x.max(0.0), y.max(0.0)) * scale.max(0.01)
}

fn native_mouse_button(button: MouseEventButton) -> Option<MouseButton> {
    match button {
        MouseEventButton::Main => Some(MouseButton::Left),
        MouseEventButton::Secondary => Some(MouseButton::Right),
        MouseEventButton::Auxiliary => Some(MouseButton::Middle),
        MouseEventButton::Fourth | MouseEventButton::Fifth => None,
    }
}

fn button_mask(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
        _ => 0,
    }
}

fn key_character(key: &Key) -> Option<String> {
    match key {
        Key::Character(value) => Some(value.to_ascii_lowercase()),
        _ => None,
    }
}

fn editor_key(key: &Key) -> mox_hmi_ui::shortcuts::EditorKey {
    use mox_hmi_ui::shortcuts::EditorKey;
    match key {
        Key::Character(value) => EditorKey::Character(value.to_string()),
        Key::Named(NamedKey::F1) => EditorKey::F1,
        Key::Named(NamedKey::Escape) => EditorKey::Escape,
        Key::Named(NamedKey::Delete) => EditorKey::Delete,
        Key::Named(NamedKey::Backspace) => EditorKey::Backspace,
        _ => EditorKey::Other,
    }
}

fn is_text_composition(event: &BlitzKeyEvent) -> bool {
    event.is_composing
}

fn native_key_code(code: Code) -> Option<BevyKeyCode> {
    Some(match code {
        Code::Backquote => BevyKeyCode::Backquote,
        Code::Backslash => BevyKeyCode::Backslash,
        Code::BracketLeft => BevyKeyCode::BracketLeft,
        Code::BracketRight => BevyKeyCode::BracketRight,
        Code::Comma => BevyKeyCode::Comma,
        Code::Digit0 => BevyKeyCode::Digit0,
        Code::Digit1 => BevyKeyCode::Digit1,
        Code::Digit2 => BevyKeyCode::Digit2,
        Code::Digit3 => BevyKeyCode::Digit3,
        Code::Digit4 => BevyKeyCode::Digit4,
        Code::Digit5 => BevyKeyCode::Digit5,
        Code::Digit6 => BevyKeyCode::Digit6,
        Code::Digit7 => BevyKeyCode::Digit7,
        Code::Digit8 => BevyKeyCode::Digit8,
        Code::Digit9 => BevyKeyCode::Digit9,
        Code::Equal => BevyKeyCode::Equal,
        Code::KeyA => BevyKeyCode::KeyA,
        Code::KeyB => BevyKeyCode::KeyB,
        Code::KeyC => BevyKeyCode::KeyC,
        Code::KeyD => BevyKeyCode::KeyD,
        Code::KeyE => BevyKeyCode::KeyE,
        Code::KeyF => BevyKeyCode::KeyF,
        Code::KeyG => BevyKeyCode::KeyG,
        Code::KeyH => BevyKeyCode::KeyH,
        Code::KeyI => BevyKeyCode::KeyI,
        Code::KeyJ => BevyKeyCode::KeyJ,
        Code::KeyK => BevyKeyCode::KeyK,
        Code::KeyL => BevyKeyCode::KeyL,
        Code::KeyM => BevyKeyCode::KeyM,
        Code::KeyN => BevyKeyCode::KeyN,
        Code::KeyO => BevyKeyCode::KeyO,
        Code::KeyP => BevyKeyCode::KeyP,
        Code::KeyQ => BevyKeyCode::KeyQ,
        Code::KeyR => BevyKeyCode::KeyR,
        Code::KeyS => BevyKeyCode::KeyS,
        Code::KeyT => BevyKeyCode::KeyT,
        Code::KeyU => BevyKeyCode::KeyU,
        Code::KeyV => BevyKeyCode::KeyV,
        Code::KeyW => BevyKeyCode::KeyW,
        Code::KeyX => BevyKeyCode::KeyX,
        Code::KeyY => BevyKeyCode::KeyY,
        Code::KeyZ => BevyKeyCode::KeyZ,
        Code::Minus => BevyKeyCode::Minus,
        Code::Period => BevyKeyCode::Period,
        Code::Quote => BevyKeyCode::Quote,
        Code::Semicolon => BevyKeyCode::Semicolon,
        Code::Slash => BevyKeyCode::Slash,
        Code::AltLeft => BevyKeyCode::AltLeft,
        Code::AltRight => BevyKeyCode::AltRight,
        Code::Backspace => BevyKeyCode::Backspace,
        Code::CapsLock => BevyKeyCode::CapsLock,
        Code::ControlLeft => BevyKeyCode::ControlLeft,
        Code::ControlRight => BevyKeyCode::ControlRight,
        Code::Enter => BevyKeyCode::Enter,
        Code::MetaLeft => BevyKeyCode::SuperLeft,
        Code::MetaRight => BevyKeyCode::SuperRight,
        Code::ShiftLeft => BevyKeyCode::ShiftLeft,
        Code::ShiftRight => BevyKeyCode::ShiftRight,
        Code::Space => BevyKeyCode::Space,
        Code::Tab => BevyKeyCode::Tab,
        Code::Delete => BevyKeyCode::Delete,
        Code::End => BevyKeyCode::End,
        Code::Home => BevyKeyCode::Home,
        Code::Insert => BevyKeyCode::Insert,
        Code::PageDown => BevyKeyCode::PageDown,
        Code::PageUp => BevyKeyCode::PageUp,
        Code::ArrowDown => BevyKeyCode::ArrowDown,
        Code::ArrowLeft => BevyKeyCode::ArrowLeft,
        Code::ArrowRight => BevyKeyCode::ArrowRight,
        Code::ArrowUp => BevyKeyCode::ArrowUp,
        Code::NumLock => BevyKeyCode::NumLock,
        Code::Numpad0 => BevyKeyCode::Numpad0,
        Code::Numpad1 => BevyKeyCode::Numpad1,
        Code::Numpad2 => BevyKeyCode::Numpad2,
        Code::Numpad3 => BevyKeyCode::Numpad3,
        Code::Numpad4 => BevyKeyCode::Numpad4,
        Code::Numpad5 => BevyKeyCode::Numpad5,
        Code::Numpad6 => BevyKeyCode::Numpad6,
        Code::Numpad7 => BevyKeyCode::Numpad7,
        Code::Numpad8 => BevyKeyCode::Numpad8,
        Code::Numpad9 => BevyKeyCode::Numpad9,
        Code::NumpadAdd => BevyKeyCode::NumpadAdd,
        Code::NumpadDecimal => BevyKeyCode::NumpadDecimal,
        Code::NumpadDivide => BevyKeyCode::NumpadDivide,
        Code::NumpadEnter => BevyKeyCode::NumpadEnter,
        Code::NumpadMultiply => BevyKeyCode::NumpadMultiply,
        Code::NumpadSubtract => BevyKeyCode::NumpadSubtract,
        _ => return None,
    })
}
