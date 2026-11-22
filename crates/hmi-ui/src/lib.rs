mod app;
pub mod bridge;
pub mod control;
pub mod shortcuts;
mod timer;
pub mod ui;

pub use app::App;

use std::sync::OnceLock;

static ENGINE_STARTER: OnceLock<fn()> = OnceLock::new();

#[cfg(not(target_arch = "wasm32"))]
static NATIVE_VIEWPORT: OnceLock<fn() -> dioxus::prelude::Element> = OnceLock::new();

pub fn install_engine_starter(starter: fn()) {
    let _ = ENGINE_STARTER.set(starter);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn install_native_viewport(provider: fn() -> dioxus::prelude::Element) {
    let _ = NATIVE_VIEWPORT.set(provider);
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn native_viewport() -> Option<dioxus::prelude::Element> {
    NATIVE_VIEWPORT.get().copied().map(|provider| provider())
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn start_engine() -> Result<(), &'static str> {
    ENGINE_STARTER
        .get()
        .copied()
        .ok_or("Bevy 渲染器启动器未安装")
        .map(|starter| starter())
}

pub fn launch() {
    dioxus::launch(App);
}
