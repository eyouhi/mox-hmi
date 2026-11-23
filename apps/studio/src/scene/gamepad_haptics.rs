#[cfg(target_os = "macos")]
use std::cell::RefCell;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
use objc2::AnyThread;
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2_core_haptics::{
    CHHapticDynamicParameter, CHHapticEngine, CHHapticEvent, CHHapticEventParameter,
    CHHapticEventParameterIDHapticIntensity, CHHapticEventParameterIDHapticSharpness,
    CHHapticEventTypeHapticTransient, CHHapticPattern, CHHapticPatternPlayer,
    CHHapticTimeImmediate,
};
#[cfg(target_os = "macos")]
use objc2_foundation::NSArray;
#[cfg(target_os = "macos")]
use objc2_game_controller::{GCController, GCDeviceHaptics, GCHapticsLocalityDefault};

#[cfg(target_os = "macos")]
static HAPTIC_ERROR_REPORTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static HAPTIC_READY_REPORTED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
struct HapticsSession {
    controller: usize,
    engine: Retained<CHHapticEngine>,
    pattern: Retained<CHHapticPattern>,
}

#[cfg(target_os = "macos")]
thread_local! {
    static HAPTICS_SESSION: RefCell<Option<HapticsSession>> = const { RefCell::new(None) };
}

#[cfg(target_os = "macos")]
pub fn pulse() -> bool {
    let controllers = unsafe { GCController::controllers() };
    for controller in controllers.iter() {
        let Some(haptics) = (unsafe { controller.haptics() }) else {
            continue;
        };
        let key = Retained::as_ptr(&controller) as usize;
        let played = HAPTICS_SESSION.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot
                .as_ref()
                .is_none_or(|session| session.controller != key)
            {
                *slot = HapticsSession::new(key, &haptics);
            }
            let played = slot.as_ref().is_some_and(|session| session.play());
            if !played {
                *slot = None;
            }
            played
        });
        if played {
            return true;
        }
    }
    false
}

#[cfg(not(target_os = "macos"))]
pub fn pulse() -> bool {
    false
}

#[cfg(target_os = "macos")]
impl HapticsSession {
    fn new(controller: usize, haptics: &GCDeviceHaptics) -> Option<Self> {
        unsafe {
            let responds: bool = objc2::msg_send![
                haptics,
                respondsToSelector: objc2::sel!(createEngineWithLocality:)
            ];
            if !responds {
                return None;
            }
            let engine: Option<Retained<CHHapticEngine>> = objc2::msg_send![
                haptics,
                createEngineWithLocality: GCHapticsLocalityDefault
            ];
            let engine = engine?;
            engine.setPlaysHapticsOnly(true);
            if engine.startAndReturnError().is_err() {
                report_failure("启动 CoreHaptics 引擎失败");
                return None;
            }
            let pattern = build_pattern()?;
            if !HAPTIC_READY_REPORTED.swap(true, Ordering::Relaxed) {
                eprintln!("MOX HMI Studio: DualSense CoreHaptics 震动已就绪");
            }
            Some(Self {
                controller,
                engine,
                pattern,
            })
        }
    }

    fn play(&self) -> bool {
        unsafe {
            let Ok(player) = self.engine.createPlayerWithPattern_error(&self.pattern) else {
                report_failure("创建震动 Pattern Player 失败");
                return false;
            };
            if player.startAtTime_error(CHHapticTimeImmediate).is_err() {
                report_failure("播放震动 Pattern 失败");
                return false;
            }
        }
        true
    }
}

#[cfg(target_os = "macos")]
unsafe fn build_pattern() -> Option<Retained<CHHapticPattern>> {
    unsafe {
        let intensity = CHHapticEventParameter::initWithParameterID_value(
            CHHapticEventParameter::alloc(),
            CHHapticEventParameterIDHapticIntensity,
            0.72,
        );
        let sharpness = CHHapticEventParameter::initWithParameterID_value(
            CHHapticEventParameter::alloc(),
            CHHapticEventParameterIDHapticSharpness,
            0.64,
        );
        let parameters = NSArray::from_retained_slice(&[intensity, sharpness]);
        let event = CHHapticEvent::initWithEventType_parameters_relativeTime(
            CHHapticEvent::alloc(),
            CHHapticEventTypeHapticTransient,
            &parameters,
            0.0,
        );
        let events = NSArray::from_retained_slice(&[event]);
        let dynamic_parameters = NSArray::<CHHapticDynamicParameter>::from_slice(&[]);
        match CHHapticPattern::initWithEvents_parameters_error(
            CHHapticPattern::alloc(),
            &events,
            &dynamic_parameters,
        ) {
            Ok(pattern) => Some(pattern),
            Err(_) => {
                report_failure("创建震动 Pattern 失败");
                None
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn report_failure(message: &str) {
    if !HAPTIC_ERROR_REPORTED.swap(true, Ordering::Relaxed) {
        eprintln!("MOX HMI Studio: {message}，将回退到手柄 rumble");
    }
}
