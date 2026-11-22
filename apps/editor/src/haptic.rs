#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2_app_kit::NSSound;
#[cfg(target_os = "macos")]
use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
thread_local! {
    static SNAP_SOUND: RefCell<Option<Retained<NSSound>>> = const { RefCell::new(None) };
    static LAST_SNAP_SOUND: Cell<Option<Instant>> = const { Cell::new(None) };
}

#[cfg(target_os = "macos")]
fn play_sound() {
    use objc2::AnyThread;
    use objc2_foundation::NSData;

    let now = Instant::now();
    let should_play = LAST_SNAP_SOUND.with(|last| {
        let play = last
            .get()
            .is_none_or(|previous| now.duration_since(previous) >= Duration::from_millis(60));
        if play {
            last.set(Some(now));
        }
        play
    });
    if !should_play {
        return;
    }

    SNAP_SOUND.with(|slot| {
        if slot.borrow().is_none() {
            let data = NSData::with_bytes(&snap_wav());
            let Some(sound) = NSSound::initWithData(NSSound::alloc(), &data) else {
                return;
            };
            sound.setVolume(0.5);
            *slot.borrow_mut() = Some(sound);
        }
        if let Some(sound) = slot.borrow().as_ref() {
            let _ = sound.play();
        }
    });
}

#[cfg(target_os = "macos")]
fn snap_wav() -> Vec<u8> {
    const SAMPLE_RATE: u32 = 44_100;
    const DURATION_SECS: f32 = 0.022;
    const FREQUENCY: f32 = 3_100.0;

    let sample_count = (SAMPLE_RATE as f32 * DURATION_SECS).round() as u32;
    let data_len = sample_count * 2;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());

    for index in 0..sample_count {
        let time = index as f32 / SAMPLE_RATE as f32;
        let attack = (time / 0.0007).min(1.0);
        let decay = (-time / 0.0045).exp();
        let tone = (std::f32::consts::TAU * FREQUENCY * time).sin()
            + 0.22 * (std::f32::consts::TAU * FREQUENCY * 1.7 * time).sin();
        let sample = (tone * attack * decay * 0.34 * i16::MAX as f32)
            .clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
    }

    wav
}

#[cfg(target_os = "macos")]
fn pulse() {
    use objc2_app_kit::{
        NSHapticFeedbackManager, NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime,
        NSHapticFeedbackPerformer,
    };

    let performer = NSHapticFeedbackManager::defaultPerformer();
    performer.performFeedbackPattern_performanceTime(
        NSHapticFeedbackPattern::Alignment,
        NSHapticFeedbackPerformanceTime::Now,
    );
}

#[cfg(target_os = "macos")]
pub fn snap() {
    use dispatch2::DispatchQueue;

    let main = DispatchQueue::main();
    main.exec_async(pulse);
    main.exec_async(play_sound);
}

#[cfg(not(target_os = "macos"))]
pub fn snap() {}

#[cfg(target_os = "macos")]
pub fn activate_app() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}

#[cfg(not(target_os = "macos"))]
pub fn activate_app() {}
