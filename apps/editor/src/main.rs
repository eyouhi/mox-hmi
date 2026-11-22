mod bevy_widget;
mod control_native;
mod haptic;

use control_native::NativeControl;
use dioxus::prelude::*;
use dioxus_native::{Config as NativeConfig, LogicalSize, WindowAttributes, use_window_event};
use mox_hmi_control::{
    ControlHub,
    http::{self, HttpConfig},
};
use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Duration,
};

const DEFAULT_PORT: u16 = 17_880;

#[derive(Debug)]
struct Options {
    port: u16,
    token: Option<String>,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut port = DEFAULT_PORT;
        let mut token = None;
        let mut args = env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--port" => {
                    port = next_value(&mut args, "--port")?
                        .parse()
                        .map_err(|error| format!("无效端口: {error}"))?;
                }
                "--token" => token = Some(next_value(&mut args, "--token")?),
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("未知参数 `{other}`")),
            }
        }

        Ok(Self { port, token })
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{option} 缺少参数值"))
}

fn print_help() {
    println!(
        "MOX HMI Studio · native Dioxus + Bevy editor\n\n\
         用法:\n  mox-hmi-editor [OPTIONS]\n\n\
         选项:\n  --port <PORT>       本地控制与 MCP 端口，默认 17880\n  \
         --token <TOKEN>     可选的控制令牌\n\n\
         MCP 端点: http://127.0.0.1:<PORT>/mcp"
    );
}

fn main() {
    if let Err(error) = run() {
        eprintln!("mox-hmi-editor: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let options = Options::parse()?;
    mox_hmi_ui::bridge::initialize()?;
    mox_hmi_ui::bridge::install_native_haptic(haptic::snap);
    haptic::activate_app();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("创建 Tokio 运行时失败: {error}"))?;
    let hub = ControlHub::new(options.token.clone());
    let native_control = NativeControl::start(hub.clone(), runtime.handle())?;
    let (address_tx, address_rx) = mpsc::channel::<Result<SocketAddr, String>>();
    let server_port = options.port;
    let server_token = options.token.clone();

    std::thread::spawn(move || {
        runtime.block_on(async move {
            let listener = match tokio::net::TcpListener::bind(("127.0.0.1", server_port)).await {
                Ok(listener) => listener,
                Err(error) => {
                    let _ =
                        address_tx.send(Err(format!("无法监听 127.0.0.1:{server_port}: {error}")));
                    return;
                }
            };
            let address = match listener.local_addr() {
                Ok(address) => address,
                Err(error) => {
                    let _ = address_tx.send(Err(format!("读取监听地址失败: {error}")));
                    return;
                }
            };
            let _ = address_tx.send(Ok(address));
            let config = HttpConfig {
                token: server_token,
                web_dir: None,
                haptic: Some(Arc::new(haptic::snap)),
            };
            if let Err(error) = http::serve(listener, hub, config).await {
                eprintln!("mox-hmi-editor: 本地服务退出: {error}");
            }
        });
    });

    let address = address_rx
        .recv_timeout(Duration::from_secs(10))
        .map_err(|_| "启动本地服务超时".to_string())??;
    eprintln!("MOX HMI Studio (native): http://{address}/");
    eprintln!("MCP endpoint: http://{address}/mcp");

    bevy_widget::install_asset_path(asset_path()?);
    mox_hmi_ui::install_native_viewport(bevy_widget::viewport);
    control_native::install(native_control);

    let config = NativeConfig::new().with_window_attributes(
        WindowAttributes::default()
            .with_title("MOX HMI Studio")
            .with_surface_size(LogicalSize::new(1440.0, 900.0))
            .with_min_surface_size(LogicalSize::new(1080.0, 680.0)),
    );
    dioxus_native::launch_cfg(EditorRoot, Vec::new(), vec![Box::new(config)]);
    Ok(())
}

#[component]
fn EditorRoot() -> Element {
    use_window_event(|event, _| {
        if let dioxus_native::winit::event::WindowEvent::PinchGesture { delta, phase, .. } = event
            && *phase != dioxus_native::winit::event::TouchPhase::Started
        {
            bevy_widget::queue_pinch(*delta as f32);
        }
    });
    rsx! {
        mox_hmi_ui::App {}
    }
}

fn asset_path() -> Result<String, String> {
    if let Some(path) = env::var_os("MOX_HMI_ASSET_DIR") {
        return Ok(PathBuf::from(path).to_string_lossy().into_owned());
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "无法定位 workspace".to_string())?;
    let mut candidates = vec![workspace.join("apps/studio/public/assets")];
    if let Ok(executable) = env::current_exe()
        && let Some(contents) = executable.parent().and_then(Path::parent)
    {
        candidates.push(contents.join("Resources/assets"));
        candidates.push(contents.join("Resources/web/assets"));
    }
    if let Ok(current) = env::current_dir() {
        candidates.push(current.join("apps/studio/public/assets"));
    }
    candidates
        .into_iter()
        .find(|path| path.is_dir())
        .map(|path| path.to_string_lossy().into_owned())
        .ok_or_else(|| "找不到 Bevy 资源目录".to_string())
}
