#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};
#[cfg(target_arch = "wasm32")]
use web_sys::{AudioContext, AudioContextState, AudioNode, OscillatorType};

#[cfg(target_arch = "wasm32")]
thread_local! {
    static AUDIO_CONTEXT: RefCell<Option<AudioContext>> = const { RefCell::new(None) };
}

pub fn prime() {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = audio_context();
    }
}

#[cfg(not(target_arch = "wasm32"))]
static GAMEPAD_RUMBLE_PENDING: AtomicBool = AtomicBool::new(false);

pub fn trigger() {
    #[cfg(target_arch = "wasm32")]
    {
        play_snap_sound();
        vibrate();
        gamepad_haptic();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::bridge::native_haptic();
        GAMEPAD_RUMBLE_PENDING.store(true, Ordering::Relaxed);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn take_gamepad_rumble() -> bool {
    GAMEPAD_RUMBLE_PENDING.swap(false, Ordering::Relaxed)
}

#[cfg(target_arch = "wasm32")]
pub fn take_gamepad_rumble() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn audio_context() -> Option<AudioContext> {
    AUDIO_CONTEXT.with(|slot| {
        let cached = slot.borrow().clone();
        let context = match cached {
            Some(context) => context,
            None => {
                let context = AudioContext::new().ok()?;
                *slot.borrow_mut() = Some(context.clone());
                context
            }
        };
        if context.state() == AudioContextState::Suspended {
            let _ = context.resume();
        }
        Some(context)
    })
}

#[cfg(target_arch = "wasm32")]
fn play_snap_sound() {
    let Some(context) = audio_context() else {
        return;
    };
    let Ok(oscillator) = context.create_oscillator() else {
        return;
    };
    let Ok(gain) = context.create_gain() else {
        return;
    };

    let now = context.current_time();
    oscillator.set_type(OscillatorType::Sine);
    let frequency = oscillator.frequency();
    let _ = frequency.set_value_at_time(2600.0, now);
    let _ = frequency.exponential_ramp_to_value_at_time(2200.0, now + 0.024);

    let envelope = gain.gain();
    let _ = envelope.set_value_at_time(0.0001, now);
    let _ = envelope.exponential_ramp_to_value_at_time(0.035, now + 0.002);
    let _ = envelope.exponential_ramp_to_value_at_time(0.0001, now + 0.028);

    let destination = context.destination();
    let oscillator_node: &AudioNode = oscillator.unchecked_ref();
    let gain_node: &AudioNode = gain.unchecked_ref();
    let destination_node: &AudioNode = destination.unchecked_ref();
    let _ = oscillator_node.connect_with_audio_node(gain_node);
    let _ = gain_node.connect_with_audio_node(destination_node);
    let _ = oscillator.start_with_when(now);
    let _ = oscillator.stop_with_when(now + 0.04);
}

#[cfg(target_arch = "wasm32")]
fn vibrate() {
    let Some(window) = web_sys::window() else {
        return;
    };
    if is_native_editor(&window) {
        native_haptic(&window);
        return;
    }
    let navigator: JsValue = window.navigator().into();
    let Ok(vibrate) = js_sys::Reflect::get(&navigator, &JsValue::from_str("vibrate")) else {
        return;
    };
    let Some(vibrate) = vibrate.dyn_ref::<js_sys::Function>() else {
        return;
    };
    let _ = vibrate.call1(&navigator, &JsValue::from_f64(10.0));
}

#[cfg(target_arch = "wasm32")]
fn is_native_editor(window: &web_sys::Window) -> bool {
    let native_marker = js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("__MOX_NATIVE__"))
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if native_marker {
        return true;
    }

    window.location().search().is_ok_and(|search| {
        search
            .trim_start_matches('?')
            .split('&')
            .any(|part| matches!(part, "native=1" | "native=true"))
    })
}

#[cfg(target_arch = "wasm32")]
fn native_haptic(window: &web_sys::Window) {
    let window: &JsValue = window.as_ref();
    if let Ok(ipc) = js_sys::Reflect::get(window, &JsValue::from_str("ipc")) {
        if let Ok(post_message) = js_sys::Reflect::get(&ipc, &JsValue::from_str("postMessage")) {
            if let Some(post_message) = post_message.dyn_ref::<js_sys::Function>() {
                let message = JsValue::from_str(r#"{"method":"haptic"}"#);
                if post_message.call1(&ipc, &message).is_ok() {
                    return;
                }
            }
        }
    }

    let Ok(fetch) = js_sys::Reflect::get(window, &JsValue::from_str("fetch")) else {
        return;
    };
    let Some(fetch) = fetch.dyn_ref::<js_sys::Function>() else {
        return;
    };
    let init = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &init,
        &JsValue::from_str("method"),
        &JsValue::from_str("POST"),
    );
    let _ = fetch.call2(window, &JsValue::from_str("/haptic"), &init);
}

#[cfg(target_arch = "wasm32")]
fn gamepad_haptic() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(gamepads) = window.navigator().get_gamepads() else {
        return;
    };
    for index in 0..gamepads.length() {
        let gamepad = gamepads.get(index);
        if gamepad.is_undefined() || gamepad.is_null() {
            continue;
        }
        let Ok(connected) = js_sys::Reflect::get(&gamepad, &JsValue::from_str("connected")) else {
            continue;
        };
        if !connected.as_bool().unwrap_or(false) {
            continue;
        }
        let Ok(actuator) = js_sys::Reflect::get(&gamepad, &JsValue::from_str("vibrationActuator"))
        else {
            continue;
        };
        if actuator.is_undefined() || actuator.is_null() {
            continue;
        }
        let Ok(play_effect) = js_sys::Reflect::get(&actuator, &JsValue::from_str("playEffect"))
        else {
            continue;
        };
        let Some(play_effect) = play_effect.dyn_ref::<js_sys::Function>() else {
            continue;
        };
        let options = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &options,
            &JsValue::from_str("duration"),
            &JsValue::from_f64(55.0),
        );
        let _ = js_sys::Reflect::set(
            &options,
            &JsValue::from_str("startDelay"),
            &JsValue::from_f64(0.0),
        );
        let _ = js_sys::Reflect::set(
            &options,
            &JsValue::from_str("strongMagnitude"),
            &JsValue::from_f64(0.42),
        );
        let _ = js_sys::Reflect::set(
            &options,
            &JsValue::from_str("weakMagnitude"),
            &JsValue::from_f64(0.9),
        );
        let _ = play_effect.call2(
            &actuator,
            &JsValue::from_str("dual-rumble"),
            &options.into(),
        );
        return;
    }
}
