#[cfg(target_arch = "wasm32")]
fn main() {
    use mox_hmi_studio::scene;

    console_error_panic_hook::set_once();
    mox_hmi_ui::install_engine_starter(scene::start_bevy);
    mox_hmi_ui::launch();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("mox-hmi-studio 是 Web runner；原生编辑器请运行 `cargo x edit`");
    std::process::exit(1);
}
