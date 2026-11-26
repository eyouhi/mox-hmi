#[cfg(all(target_arch = "wasm32", feature = "web"))]
fn main() {
    use dioxus::prelude::LaunchBuilder;
    use mox_hmi_studio::{runner::RunnerRoot, scene};

    console_error_panic_hook::set_once();
    mox_hmi_ui::install_engine_starter(scene::start_bevy);
    LaunchBuilder::web().launch(RunnerRoot);
}

#[cfg(all(not(target_arch = "wasm32"), feature = "server"))]
fn main() {
    use dioxus::prelude::LaunchBuilder;
    use mox_hmi_studio::runner::RunnerRoot;

    LaunchBuilder::server().launch(RunnerRoot);
}

#[cfg(not(any(
    all(target_arch = "wasm32", feature = "web"),
    all(not(target_arch = "wasm32"), feature = "server")
)))]
fn main() {
    eprintln!(
        "mox-hmi-studio 需要 web 或 server feature；开发运行器请使用 `cargo x run`，原生编辑器请使用 `cargo x edit`"
    );
    std::process::exit(1);
}
