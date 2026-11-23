#[cfg(not(target_arch = "wasm32"))]
use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use hmi_core::Screen;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};
#[cfg(target_arch = "wasm32")]
use web_sys::{Gamepad, GamepadButton};

const STICK_DEADZONE: f32 = 0.18;
#[cfg(not(target_arch = "wasm32"))]
const HID_BUTTON_PAGE: u32 = 0x09;
#[cfg(not(target_arch = "wasm32"))]
const DUAL_SENSE_TOUCHPAD_USAGE: u32 = 14;
#[cfg(target_os = "macos")]
static GAME_CONTROLLER_REPORTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static GAME_CONTROLLER_INPUT_REPORTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum PadButton {
    Cross = 0,
    Circle = 1,
    Square = 2,
    Triangle = 3,
    L1 = 4,
    R1 = 5,
    L2 = 6,
    R2 = 7,
    Create = 8,
    Options = 9,
    L3 = 10,
    R3 = 11,
    DpadUp = 12,
    DpadDown = 13,
    DpadLeft = 14,
    DpadRight = 15,
    Touchpad = 17,
}

#[derive(Resource)]
pub struct GamepadState {
    pub connected: bool,
    pub id: String,
    axes: [f32; 4],
    triggers: [f32; 2],
    buttons: [bool; 18],
    previous_buttons: [bool; 18],
}

impl Default for GamepadState {
    fn default() -> Self {
        Self {
            connected: false,
            id: String::new(),
            axes: [0.0; 4],
            triggers: [0.0; 2],
            buttons: [false; 18],
            previous_buttons: [false; 18],
        }
    }
}

impl GamepadState {
    pub fn pressed(&self, button: PadButton) -> bool {
        self.buttons.get(button as usize).copied().unwrap_or(false)
    }

    pub fn just_pressed(&self, button: PadButton) -> bool {
        let index = button as usize;
        self.pressed(button) && !self.previous_buttons.get(index).copied().unwrap_or(false)
    }

    pub fn left_stick(&self) -> Vec2 {
        deadzone(Vec2::new(self.axes[0], self.axes[1]))
    }

    pub fn right_stick(&self) -> Vec2 {
        deadzone(Vec2::new(self.axes[2], self.axes[3]))
    }

    pub fn left_trigger(&self) -> f32 {
        self.triggers[0].clamp(0.0, 1.0)
    }

    pub fn right_trigger(&self) -> f32 {
        self.triggers[1].clamp(0.0, 1.0)
    }
}

fn deadzone(value: Vec2) -> Vec2 {
    let length = value.length();
    if length <= STICK_DEADZONE || !length.is_finite() {
        return Vec2::ZERO;
    }
    let normalized = ((length - STICK_DEADZONE) / (1.0 - STICK_DEADZONE)).clamp(0.0, 1.0);
    value / length * normalized
}

// The Web Gamepad API reports +Y as down, while Apple's GameController
// framework reports +Y as up. Keep GamepadState on the up-positive convention
// so every editor control interprets the sticks the same way on both backends.
#[cfg(any(target_arch = "wasm32", test))]
fn normalize_web_axis(index: usize, value: f32) -> f32 {
    if index % 2 == 1 { -value } else { value }
}

#[cfg(target_arch = "wasm32")]
pub fn poll_gamepad(mut state: ResMut<GamepadState>) {
    let previous_buttons = state.buttons;
    let Some(window) = web_sys::window() else {
        reset_disconnected(&mut state);
        return;
    };
    let Ok(gamepads) = window.navigator().get_gamepads() else {
        reset_disconnected(&mut state);
        return;
    };

    let mut found = None;
    for index in 0..gamepads.length() {
        let value = gamepads.get(index);
        if value.is_undefined() || value.is_null() {
            continue;
        }
        let Ok(gamepad) = value.dyn_into::<Gamepad>() else {
            continue;
        };
        if gamepad.connected() {
            found = Some(gamepad);
            break;
        }
    }

    let Some(gamepad) = found else {
        reset_disconnected(&mut state);
        return;
    };

    state.connected = true;
    state.id = gamepad.id();
    state.previous_buttons = previous_buttons;
    let axes = gamepad.axes();
    for index in 0..4 {
        let value = axes
            .get(index as u32)
            .as_f64()
            .map(|value| value as f32)
            .unwrap_or(0.0);
        state.axes[index] = normalize_web_axis(index, value);
    }
    let buttons = gamepad.buttons();
    state.buttons = [false; 18];
    state.triggers = [0.0; 2];
    for index in 0..18 {
        let value = buttons.get(index as u32);
        if value.is_undefined() || value.is_null() {
            continue;
        }
        let Ok(button) = value.dyn_into::<GamepadButton>() else {
            continue;
        };
        state.buttons[index as usize] = button.pressed();
        if index == PadButton::L2 as u32 {
            state.triggers[0] = button.value() as f32;
        } else if index == PadButton::R2 as u32 {
            state.triggers[1] = button.value() as f32;
        }
    }

    crate::bridge::set_gamepad_status(true, state.id.clone());
}

#[cfg(target_arch = "wasm32")]
fn reset_disconnected(state: &mut GamepadState) {
    if state.connected {
        state.connected = false;
        state.id.clear();
        state.axes = [0.0; 4];
        state.triggers = [0.0; 2];
        state.buttons = [false; 18];
        state.previous_buttons = [false; 18];
        crate::bridge::set_gamepad_status(false, "");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn poll_gamepad(
    mut state: ResMut<GamepadState>,
    gamepads: Query<(&bevy::input::gamepad::Gamepad, &Name)>,
) {
    let previous_buttons = state.buttons;
    state.previous_buttons = previous_buttons;
    #[cfg(target_os = "macos")]
    if poll_game_controller(&mut state) {
        crate::bridge::set_gamepad_status(true, state.id.clone());
        return;
    }
    let Some((gamepad, name)) = gamepads.iter().next() else {
        if state.connected {
            state.connected = false;
            state.id.clear();
            state.axes = [0.0; 4];
            state.triggers = [0.0; 2];
            state.buttons = [false; 18];
            state.previous_buttons = [false; 18];
            crate::bridge::set_gamepad_status(false, "");
        }
        return;
    };
    use bevy::input::gamepad::GamepadAxis;

    state.connected = true;
    state.id = if name.as_str().is_empty() {
        match (gamepad.vendor_id(), gamepad.product_id()) {
            (Some(vendor), Some(product)) => format!("{vendor:04x}:{product:04x}"),
            _ => "DualSense".to_string(),
        }
    } else {
        name.as_str().to_string()
    };
    state.axes = [
        gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0),
        gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0),
        gamepad.get(GamepadAxis::RightStickX).unwrap_or(0.0),
        gamepad.get(GamepadAxis::RightStickY).unwrap_or(0.0),
    ];
    state.triggers = [
        gamepad
            .get(GamepadAxis::LeftZ)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0),
        gamepad
            .get(GamepadAxis::RightZ)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0),
    ];
    state.buttons = [false; 18];
    for button in gamepad.get_pressed() {
        if let Some(target) = native_pad_button(*button) {
            state.buttons[target as usize] = true;
        }
    }
    crate::bridge::set_gamepad_status(true, state.id.clone());
}

// macOS 26+ switches DualSense Bluetooth reports to a combined format that
// gilrs does not decode. Apple's GameController framework reads the same
// controller correctly, so prefer it when a controller is available.
#[cfg(target_os = "macos")]
fn poll_game_controller(state: &mut GamepadState) -> bool {
    use objc2_game_controller::GCController;
    use std::sync::Once;

    static SETUP: Once = Once::new();
    SETUP.call_once(|| unsafe {
        GCController::setShouldMonitorBackgroundEvents(true);
    });

    let controllers = unsafe { GCController::controllers() };
    for controller in controllers.iter() {
        let Some(pad) = (unsafe { controller.extendedGamepad() }) else {
            continue;
        };
        unsafe {
            fill_game_controller_state(&controller, &pad, state);
        }
        return true;
    }
    false
}

#[cfg(target_os = "macos")]
unsafe fn fill_game_controller_state(
    controller: &objc2_game_controller::GCController,
    pad: &objc2_game_controller::GCExtendedGamepad,
    state: &mut GamepadState,
) {
    use objc2_game_controller::GCDevice;

    unsafe {
        state.connected = true;
        state.id = controller
            .vendorName()
            .map(|name| name.to_string())
            .unwrap_or_else(|| "DualSense (GameController)".to_string());

        let left_stick = pad.leftThumbstick();
        let right_stick = pad.rightThumbstick();
        state.axes = [
            left_stick.xAxis().value(),
            left_stick.yAxis().value(),
            right_stick.xAxis().value(),
            right_stick.yAxis().value(),
        ];

        let left_trigger = pad.leftTrigger();
        let right_trigger = pad.rightTrigger();
        state.triggers = [
            left_trigger.value().clamp(0.0, 1.0),
            right_trigger.value().clamp(0.0, 1.0),
        ];

        state.buttons = [false; 18];
        state.buttons[PadButton::Cross as usize] = pad.buttonA().isPressed();
        state.buttons[PadButton::Circle as usize] = pad.buttonB().isPressed();
        state.buttons[PadButton::Square as usize] = pad.buttonX().isPressed();
        state.buttons[PadButton::Triangle as usize] = pad.buttonY().isPressed();
        state.buttons[PadButton::L1 as usize] = pad.leftShoulder().isPressed();
        state.buttons[PadButton::R1 as usize] = pad.rightShoulder().isPressed();
        state.buttons[PadButton::L2 as usize] = left_trigger.isPressed();
        state.buttons[PadButton::R2 as usize] = right_trigger.isPressed();
        state.buttons[PadButton::Options as usize] = pad.buttonMenu().isPressed();
        state.buttons[PadButton::Create as usize] =
            pad.buttonOptions().is_some_and(|button| button.isPressed());
        state.buttons[PadButton::L3 as usize] = pad
            .leftThumbstickButton()
            .is_some_and(|button| button.isPressed());
        state.buttons[PadButton::R3 as usize] = pad
            .rightThumbstickButton()
            .is_some_and(|button| button.isPressed());

        let dpad = pad.dpad();
        state.buttons[PadButton::DpadUp as usize] = dpad.up().isPressed();
        state.buttons[PadButton::DpadDown as usize] = dpad.down().isPressed();
        state.buttons[PadButton::DpadLeft as usize] = dpad.left().isPressed();
        state.buttons[PadButton::DpadRight as usize] = dpad.right().isPressed();

        let profile = controller.physicalInputProfile();
        if let Some(name) = objc2_game_controller::GCInputDualShockTouchpadButton
            && let Some(button) = profile.buttons().objectForKey(name)
        {
            state.buttons[PadButton::Touchpad as usize] = button.isPressed();
        }

        if !GAME_CONTROLLER_REPORTED.swap(true, Ordering::Relaxed) {
            eprintln!("MOX HMI Studio: GameController 已连接: {}", state.id);
        }
        let has_input = state.buttons.iter().any(|pressed| *pressed)
            || state.axes.iter().any(|axis| axis.abs() > 0.05)
            || state.triggers.iter().any(|trigger| *trigger > 0.05);
        if has_input && !GAME_CONTROLLER_INPUT_REPORTED.swap(true, Ordering::Relaxed) {
            eprintln!("MOX HMI Studio: GameController 首次收到手柄输入");
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn native_pad_button(button: GamepadButton) -> Option<PadButton> {
    match button {
        GamepadButton::South => Some(PadButton::Cross),
        GamepadButton::East => Some(PadButton::Circle),
        GamepadButton::West => Some(PadButton::Square),
        GamepadButton::North => Some(PadButton::Triangle),
        GamepadButton::LeftTrigger => Some(PadButton::L1),
        GamepadButton::RightTrigger => Some(PadButton::R1),
        GamepadButton::LeftTrigger2 => Some(PadButton::L2),
        GamepadButton::RightTrigger2 => Some(PadButton::R2),
        GamepadButton::Select => Some(PadButton::Create),
        GamepadButton::Start => Some(PadButton::Options),
        GamepadButton::LeftThumb => Some(PadButton::L3),
        GamepadButton::RightThumb => Some(PadButton::R3),
        GamepadButton::DPadUp => Some(PadButton::DpadUp),
        GamepadButton::DPadDown => Some(PadButton::DpadDown),
        GamepadButton::DPadLeft => Some(PadButton::DpadLeft),
        GamepadButton::DPadRight => Some(PadButton::DpadRight),
        GamepadButton::Other(code) => native_hid_pad_button(code),
        GamepadButton::C | GamepadButton::Z | GamepadButton::Mode => None,
    }
}

// macOS exposes DualSense as 14 HID buttons. Button 14 is the touchpad click,
// but Gilrs maps it through SDL's `touchpad` binding to `Button::Unknown`.
#[cfg(not(target_arch = "wasm32"))]
fn native_hid_pad_button(code: u32) -> Option<PadButton> {
    if code >> 16 != HID_BUTTON_PAGE {
        return None;
    }
    match code & 0xffff {
        1 => Some(PadButton::Square),
        2 => Some(PadButton::Cross),
        3 => Some(PadButton::Circle),
        4 => Some(PadButton::Triangle),
        5 => Some(PadButton::L1),
        6 => Some(PadButton::R1),
        7 => Some(PadButton::L2),
        8 => Some(PadButton::R2),
        9 => Some(PadButton::Create),
        10 => Some(PadButton::Options),
        11 => Some(PadButton::L3),
        12 => Some(PadButton::R3),
        DUAL_SENSE_TOUCHPAD_USAGE => Some(PadButton::Touchpad),
        _ => None,
    }
}

pub fn pulse_gamepad_rumble(
    mut requests: MessageWriter<bevy::input::gamepad::GamepadRumbleRequest>,
    gamepads: Query<Entity, With<bevy::input::gamepad::Gamepad>>,
) {
    if !super::snap_feedback::take_gamepad_rumble() {
        return;
    }
    if super::gamepad_haptics::pulse() {
        return;
    }
    for gamepad in gamepads.iter() {
        requests.write(bevy::input::gamepad::GamepadRumbleRequest::Add {
            gamepad,
            intensity: bevy::input::gamepad::GamepadRumbleIntensity {
                strong_motor: 0.28,
                weak_motor: 0.72,
            },
            duration: std::time::Duration::from_millis(55),
        });
    }
}

pub fn gamepad_guide_controls(pad: Res<GamepadState>, time: Res<Time>, mut repeat: Local<f32>) {
    if !pad.connected {
        *repeat = 0.0;
        return;
    }
    if crate::bridge::gamepad_menu_open() {
        *repeat = 0.0;
        return;
    }
    if pad.just_pressed(PadButton::Create) {
        if pad.pressed(PadButton::L3) {
            return;
        }
        if crate::bridge::guide_open() {
            crate::bridge::set_guide_open(false);
        } else {
            crate::bridge::toggle_guide();
        }
        *repeat = 0.0;
        return;
    }
    if !crate::bridge::guide_open() {
        *repeat = 0.0;
        return;
    }

    if pad.just_pressed(PadButton::Circle) {
        crate::bridge::set_guide_open(false);
        *repeat = 0.0;
        return;
    }
    apply_guide_tab_controls(&pad);
    apply_guide_scroll(&pad, time.delta_secs().min(0.05), &mut repeat);
}

const GUIDE_DPAD_STEP: f64 = 92.0;
const GUIDE_DPAD_REPEAT_SECS: f32 = 0.11;
const GUIDE_STICK_SPEED: f64 = 760.0;

fn apply_guide_tab_controls(pad: &GamepadState) {
    if pad.just_pressed(PadButton::L1) || pad.just_pressed(PadButton::DpadLeft) {
        crate::bridge::cycle_guide_section(-1);
    }
    if pad.just_pressed(PadButton::R1) || pad.just_pressed(PadButton::DpadRight) {
        crate::bridge::cycle_guide_section(1);
    }
}

fn apply_guide_scroll(pad: &GamepadState, dt: f32, repeat: &mut f32) {
    let stick_delta = guide_stick_scroll(pad.left_stick().y, dt);
    if stick_delta.abs() > 0.01 {
        crate::bridge::request_guide_scroll(stick_delta);
    }
    let dpad_delta = guide_repeat_scroll(repeat, dt, guide_dpad_direction(pad));
    if dpad_delta.abs() > f64::EPSILON {
        crate::bridge::request_guide_scroll(dpad_delta);
    }
}

fn guide_stick_scroll(stick_y: f32, dt: f32) -> f64 {
    -(stick_y as f64) * GUIDE_STICK_SPEED * dt as f64
}

fn guide_dpad_direction(pad: &GamepadState) -> f64 {
    if pad.pressed(PadButton::DpadUp) {
        -1.0
    } else if pad.pressed(PadButton::DpadDown) {
        1.0
    } else {
        0.0
    }
}

fn guide_repeat_scroll(repeat: &mut f32, dt: f32, direction: f64) -> f64 {
    if direction == 0.0 {
        *repeat = 0.0;
        0.0
    } else if *repeat <= 0.0 {
        *repeat = GUIDE_DPAD_REPEAT_SECS;
        direction * GUIDE_DPAD_STEP
    } else {
        *repeat = (*repeat - dt).max(0.0);
        0.0
    }
}

pub fn scene_input_enabled() -> bool {
    !crate::bridge::guide_open()
        && crate::bridge::WORKSPACE_MODE() == crate::bridge::WorkspaceMode::Edit
        && !text_entry_focused()
}

pub fn gamepad_input_enabled() -> bool {
    scene_input_enabled() && !crate::bridge::gamepad_menu_open()
}

pub fn keyboard_input_enabled() -> bool {
    scene_input_enabled() && canvas_has_focus()
}

pub fn canvas_has_focus() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        true
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element())
            .is_some_and(|element| element.id() == "bevy-canvas")
    }
}

pub fn pointer_over_canvas(position: Vec2) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = position;
        crate::bridge::pointer_on_native_canvas()
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("bevy-canvas"))
            .is_some_and(|element| {
                let rect = element.get_bounding_client_rect();
                position.x >= 0.0
                    && position.y >= 0.0
                    && position.x <= rect.width() as f32
                    && position.y <= rect.height() as f32
            })
    }
}

pub fn text_entry_focused() -> bool {
    crate::bridge::text_entry_focused()
}

pub fn shift_pressed(keyboard: &ButtonInput<KeyCode>) -> bool {
    keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight)
}

pub fn zoom_modifier_pressed(keyboard: &ButtonInput<KeyCode>) -> bool {
    command_modifier_pressed(keyboard) || wheel_zoom_modifier_active()
}

pub fn command_modifier_pressed(keyboard: &ButtonInput<KeyCode>) -> bool {
    crate::bridge::scene_shortcuts_suppressed()
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight)
        || keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
}

fn wheel_zoom_modifier_active() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|window| {
                js_sys::Reflect::get(&window, &JsValue::from_str("__moxHmiWheelZoomActive")).ok()
            })
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }
}

pub fn cycle_selection(direction: isize) {
    let Some(screen) = crate::bridge::active_screen_snapshot() else {
        return;
    };
    let ids: Vec<String> = match screen {
        Screen::TwoD { nodes, .. } => nodes
            .into_iter()
            .filter(|node| node.visible && !node.locked)
            .map(|node| node.id)
            .collect(),
        Screen::ThreeD { nodes, .. } => nodes
            .into_iter()
            .filter(|node| node.visible && !node.locked)
            .map(|node| node.id)
            .collect(),
    };
    if ids.is_empty() {
        crate::bridge::select_node(None);
        return;
    }
    let current = crate::bridge::selected_node();
    let current_index = current
        .as_ref()
        .and_then(|selected| ids.iter().position(|id| id == selected));
    let next_index = match current_index {
        Some(index) => {
            let length = ids.len() as isize;
            (index as isize + direction).rem_euclid(length) as usize
        }
        None if direction < 0 => ids.len() - 1,
        None => 0,
    };
    crate::bridge::select_node(Some(ids[next_index].clone()));
}

pub fn cycle_screen(direction: isize) {
    let project = crate::bridge::project();
    if project.screens.is_empty() {
        return;
    }
    let current = crate::bridge::active_screen().unwrap_or_else(|| project.startup_screen.clone());
    let current_index = project
        .screens
        .iter()
        .position(|screen| screen.id() == current);
    let next_index = match current_index {
        Some(index) => {
            let length = project.screens.len() as isize;
            (index as isize + direction).rem_euclid(length) as usize
        }
        None if direction < 0 => project.screens.len() - 1,
        None => 0,
    };
    crate::bridge::select_screen(project.screens[next_index].id());
}

pub fn gamepad_shortcuts(
    pad: Res<GamepadState>,
    tuning: Res<super::controller_tuning::ControllerTuningState>,
    mut guide_was_open: Local<bool>,
) {
    let guide_open = crate::bridge::guide_open();
    let guide_closed_this_frame = *guide_was_open && !guide_open;
    *guide_was_open = guide_open;
    if !gamepad_input_enabled()
        || !pad.connected
        || guide_open
        || guide_closed_this_frame
        || crate::bridge::controller_tuning_active()
        || tuning.shortcuts_suppressed()
    {
        return;
    }
    if pad.pressed(PadButton::L3) && pad.just_pressed(PadButton::Options) {
        crate::bridge::redo();
        return;
    }
    if pad.pressed(PadButton::L3) && pad.just_pressed(PadButton::L1) {
        cycle_screen(-1);
        return;
    }
    if pad.pressed(PadButton::L3) && pad.just_pressed(PadButton::R1) {
        cycle_screen(1);
        return;
    }
    if pad.just_pressed(PadButton::Options) {
        crate::bridge::undo();
        return;
    }
    if pad.pressed(PadButton::L3) && pad.just_pressed(PadButton::Square) {
        crate::bridge::select_all_nodes();
        return;
    }
    if pad.pressed(PadButton::L3) && pad.just_pressed(PadButton::Cross) {
        crate::bridge::copy_selection();
        return;
    }
    if pad.pressed(PadButton::L3) && pad.just_pressed(PadButton::Circle) {
        if let Err(error) = crate::bridge::cut_selection() {
            crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error);
        }
        return;
    }
    if pad.pressed(PadButton::L3) && pad.just_pressed(PadButton::Triangle) {
        if let Err(error) = crate::bridge::paste_clipboard() {
            crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error);
        }
        return;
    }
    if pad.just_pressed(PadButton::L1) {
        cycle_selection(-1);
        return;
    }
    if pad.just_pressed(PadButton::R1) {
        cycle_selection(1);
        return;
    }
    if pad.just_pressed(PadButton::Circle) {
        crate::bridge::select_node(None);
        return;
    }
    if pad.just_pressed(PadButton::R3)
        && crate::bridge::active_screen_snapshot()
            .is_some_and(|screen| matches!(screen, Screen::ThreeD { .. }))
    {
        crate::bridge::reset_camera_3d();
        return;
    }
    if pad.just_pressed(PadButton::Square) {
        if let Err(error) = crate::bridge::duplicate_selection() {
            crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error);
        }
        return;
    }
    if pad.just_pressed(PadButton::Triangle)
        && let Err(error) = crate::bridge::delete_selection()
    {
        crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn web_gamepad_y_axis_is_normalized_to_up_positive() {
        assert_eq!(normalize_web_axis(0, 0.5), 0.5);
        assert_eq!(normalize_web_axis(1, 0.5), -0.5);
        assert_eq!(normalize_web_axis(2, -0.5), -0.5);
        assert_eq!(normalize_web_axis(3, -0.5), 0.5);
    }

    #[test]
    fn native_mapping_distinguishes_shoulders_and_triggers() {
        assert_eq!(
            native_pad_button(GamepadButton::LeftTrigger),
            Some(PadButton::L1)
        );
        assert_eq!(
            native_pad_button(GamepadButton::LeftTrigger2),
            Some(PadButton::L2)
        );
        assert_eq!(
            native_pad_button(GamepadButton::RightTrigger),
            Some(PadButton::R1)
        );
        assert_eq!(
            native_pad_button(GamepadButton::RightTrigger2),
            Some(PadButton::R2)
        );
    }

    #[test]
    fn native_mapping_recognizes_dualsense_touchpad() {
        let raw_code = (HID_BUTTON_PAGE << 16) | DUAL_SENSE_TOUCHPAD_USAGE;
        assert_eq!(
            native_pad_button(GamepadButton::Other(raw_code)),
            Some(PadButton::Touchpad)
        );
    }

    #[test]
    fn native_mapping_ignores_guide_and_unrelated_hid_buttons() {
        assert_eq!(native_pad_button(GamepadButton::Mode), None);
        assert_eq!(
            native_pad_button(GamepadButton::Other((HID_BUTTON_PAGE << 16) | 13)),
            None
        );
    }

    #[test]
    fn guide_scroll_uses_up_positive_stick_and_repeats_dpad() {
        assert!(guide_stick_scroll(1.0, 0.5) < 0.0);
        assert!(guide_stick_scroll(-1.0, 0.5) > 0.0);

        let mut repeat = 0.0;
        assert_eq!(
            guide_repeat_scroll(&mut repeat, 0.016, -1.0),
            -GUIDE_DPAD_STEP
        );
        assert_eq!(guide_repeat_scroll(&mut repeat, 0.016, -1.0), 0.0);
        assert_eq!(guide_repeat_scroll(&mut repeat, 1.0, -1.0), 0.0);
        assert_eq!(
            guide_repeat_scroll(&mut repeat, 0.016, -1.0),
            -GUIDE_DPAD_STEP
        );
        assert_eq!(guide_repeat_scroll(&mut repeat, 0.016, 0.0), 0.0);
        assert_eq!(repeat, 0.0);
    }
}
