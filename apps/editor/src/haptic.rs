#[cfg(target_os = "macos")]
fn play_sound() {
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSString;

    if let Some(sound) = NSSound::soundNamed(&NSString::from_str("Tink")) {
        sound.setVolume(0.32);
        let _ = sound.play();
    }
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
    use dispatch2::{DispatchQueue, DispatchTime};
    use std::time::Duration;

    let main = DispatchQueue::main();
    main.exec_async(pulse);
    main.exec_async(play_sound);
    let delay = DispatchTime::try_from(Duration::from_millis(22)).unwrap_or(DispatchTime::NOW);
    let _ = main.after(delay, pulse);
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
