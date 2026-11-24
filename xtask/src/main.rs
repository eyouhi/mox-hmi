mod control_e2e;
mod e2e;
mod server;

use std::{
    env, fs,
    io::ErrorKind,
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const DEFAULT_ADDR: &str = "127.0.0.1";
const RUNNER_PORT: &str = "8088";
const EDITOR_PORT: &str = "17880";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(180);
const APP_NAME: &str = "MOX HMI Studio";
const RUNTIME_MARKER: &str = "window.__MOX_HMI_RUNTIME_ONLY__ = true;";

fn main() {
    if let Err(error) = try_main() {
        eprintln!("xtask: {error}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("run") => run_runner(command_args(args)),
        Some("edit") => run_editor(command_args(args)),
        Some("bundle") => bundle(command_args(args)),
        Some("verify") => verify(command_args(args)),
        Some("check-wasm-size") => check_wasm_size(command_args(args)),
        Some("font-subset") | Some("font-sync") => font_sync(command_args(args)),
        Some("e2e") => e2e::run(command_args(args)),
        Some("control-e2e") => control_e2e::run(command_args(args)),
        Some("clean") => clean(command_args(args)),
        Some("help" | "-h" | "--help") | None => {
            print_help();
            Ok(())
        }
        Some(command) => {
            print_help();
            Err(format!("未知命令 `{command}`"))
        }
    }
}

fn command_args(args: impl Iterator<Item = String>) -> Vec<String> {
    let mut args: Vec<String> = args.collect();
    if args.first().is_some_and(|arg| arg == "--") {
        args.remove(0);
    }
    args
}

fn run_runner(mut forwarded: Vec<String>) -> Result<(), String> {
    ensure_dx()?;
    let release = take_flag(&mut forwarded, "--release") || take_flag(&mut forwarded, "-r");
    let open_browser = !take_flag(&mut forwarded, "--no-open");
    let addr = option_value(&forwarded, "--addr")
        .unwrap_or(DEFAULT_ADDR)
        .to_string();
    let port = option_value(&forwarded, "--port")
        .unwrap_or(RUNNER_PORT)
        .to_string();

    let mut command = Command::new("dx");
    command
        .current_dir(workspace_root().join("apps/studio"))
        .args(["serve", "--web", "--open", "false"]);
    if !has_option(&forwarded, "--addr") {
        command.args(["--addr", DEFAULT_ADDR]);
    }
    if !has_option(&forwarded, "--port") {
        command.args(["--port", RUNNER_PORT]);
    }
    if !has_option(&forwarded, "--debug-symbols") {
        command.args(["--debug-symbols", "false"]);
    }
    if release {
        command.arg("--release");
    }
    command.args(&forwarded);

    let host = display_host(&addr);
    let url = format!("http://{host}:{port}/?runtime=1");
    println!("启动 MOX HMI 运行器: {url}");
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动 dx serve: {error}"))?;
    wait_until_ready(&mut child, &host, &port)?;
    if open_browser {
        open_url(&url)?;
    }
    let status = child
        .wait()
        .map_err(|error| format!("等待 dx serve 失败: {error}"))?;
    exit_on_failure(status, "dx serve")
}

fn run_editor(mut forwarded: Vec<String>) -> Result<(), String> {
    if forwarded
        .iter()
        .any(|arg| matches!(arg.as_str(), "--web-dir" | "--no-open"))
    {
        return Err("`cargo x edit` 不接受 Web 参数；编辑器已改为原生窗口".to_string());
    }
    let explicit_debug = take_flag(&mut forwarded, "--debug");
    let release = take_flag(&mut forwarded, "--release") || take_flag(&mut forwarded, "-r");
    if explicit_debug && release {
        return Err("`cargo x edit` 不能同时使用 --debug 和 --release".to_string());
    }
    let debug = !release;

    let port = option_value(&forwarded, "--port")
        .unwrap_or(EDITOR_PORT)
        .to_string();
    let port_number = port
        .parse::<u16>()
        .map_err(|error| format!("无效编辑器端口 `{port}`: {error}"))?;
    if TcpStream::connect((DEFAULT_ADDR, port_number)).is_ok() {
        return Err(format!(
            "端口 {port} 已有进程在监听；编辑器可能已经启动，请先关闭旧实例"
        ));
    }
    if !has_option(&forwarded, "--port") {
        forwarded.push("--port".to_string());
        forwarded.push(EDITOR_PORT.to_string());
    }

    let editor = build_editor(debug)?;
    println!("启动 MOX HMI Studio 原生编辑器（Dioxus Native + Bevy）");
    println!("控制隧道: http://{DEFAULT_ADDR}:{port}/");
    println!("MCP:        默认关闭，在桌面应用“AI 操作隧道”中启用");
    println!("触觉反馈:   POST http://{DEFAULT_ADDR}:{port}/haptic");

    let status = Command::new(editor)
        .args(&forwarded)
        .status()
        .map_err(|error| format!("无法启动 mox-hmi-editor: {error}"))?;
    exit_on_failure(status, "mox-hmi-editor")
}

fn bundle(args: Vec<String>) -> Result<(), String> {
    if !args.is_empty() {
        return Err("cargo x bundle 不接受额外参数".to_string());
    }
    ensure_tool("hdiutil")?;

    let source_app = build_macos_app()?;

    let root = workspace_root();
    let source_web = web_dir(false);
    let icon = source_app.join("Contents/Resources/AppIcon.icns");
    if !source_app.is_dir() {
        return Err(format!("应用包不存在: {}", source_app.display()));
    }
    if !source_web.join("index.html").is_file() {
        return Err(format!("运行器 Web 产物不存在: {}", source_web.display()));
    }
    if !icon.is_file() {
        return Err(format!("应用图标未生成: {}", icon.display()));
    }

    let dist = root.join("dist");
    if dist.is_dir() {
        fs::remove_dir_all(&dist)
            .map_err(|error| format!("无法清理 dist 目录 {}: {error}", dist.display()))?;
    }
    fs::create_dir_all(&dist)
        .map_err(|error| format!("无法创建 dist 目录 {}: {error}", dist.display()))?;

    let runner_web = dist.join("runner-web");
    copy_tree(&source_web, &runner_web)?;
    configure_runner_index(&runner_web.join("index.html"))?;

    let staging = root.join("target/dmg-staging");
    if staging.is_dir() {
        fs::remove_dir_all(&staging)
            .map_err(|error| format!("无法清理 DMG 暂存目录 {}: {error}", staging.display()))?;
    }
    fs::create_dir_all(&staging)
        .map_err(|error| format!("无法创建 DMG 暂存目录 {}: {error}", staging.display()))?;
    copy_tree(&source_app, &staging.join(format!("{APP_NAME}.app")))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink("/Applications", staging.join("Applications"))
        .map_err(|error| format!("无法创建 Applications 快捷方式: {error}"))?;

    let dmg = dist.join(format!("{APP_NAME}.dmg"));
    println!("生成 DMG: {}", dmg.display());
    let status = Command::new("hdiutil")
        .args([
            "create",
            "-volname",
            APP_NAME,
            "-srcfolder",
            staging.to_string_lossy().as_ref(),
            "-ov",
            "-format",
            "UDZO",
            dmg.to_string_lossy().as_ref(),
        ])
        .status()
        .map_err(|error| format!("无法启动 hdiutil: {error}"))?;
    exit_on_failure(status, "hdiutil")?;

    println!();
    println!("Bundle 输出:");
    println!("  编辑器安装包: {}", dmg.display());
    println!("  运行器 Web:   {}", runner_web.display());
    println!("  WebGPU：运行器需通过 HTTPS（或 localhost）部署。");
    Ok(())
}

fn build_macos_app() -> Result<PathBuf, String> {
    ensure_tool("iconutil")?;
    ensure_tool("sips")?;
    ensure_tool("ditto")?;
    ensure_tool("plutil")?;

    prepare_web_dir(false)?;
    let binary = build_editor(false)?;
    let root = workspace_root();
    let web = web_dir(false);
    let icon_source = root.join("apps/editor/assets/AppIcon.png");
    let info_plist = root.join("apps/editor/Info.plist");
    for required in [&web.join("index.html"), &icon_source, &info_plist] {
        if !required.exists() {
            return Err(format!("打包所需文件不存在: {}", required.display()));
        }
    }

    let app = root.join(format!("target/{APP_NAME}.app"));
    if app.is_dir() {
        fs::remove_dir_all(&app)
            .map_err(|error| format!("无法清理旧应用包 {}: {error}", app.display()))?;
    }
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    fs::create_dir_all(&macos).map_err(|error| format!("无法创建应用目录: {error}"))?;
    fs::create_dir_all(&resources).map_err(|error| format!("无法创建资源目录: {error}"))?;

    let app_binary = macos.join("mox-hmi-editor");
    fs::copy(&binary, &app_binary).map_err(|error| format!("无法复制编辑器二进制: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&app_binary, fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("无法设置编辑器可执行权限: {error}"))?;
    }
    fs::copy(&info_plist, contents.join("Info.plist"))
        .map_err(|error| format!("无法复制 Info.plist: {error}"))?;
    fs::write(contents.join("PkgInfo"), b"APPL????")
        .map_err(|error| format!("无法写入 PkgInfo: {error}"))?;
    copy_tree(&web, &resources.join("web"))?;

    println!("生成应用图标 AppIcon.icns");
    let iconset = root.join("target/AppIcon.iconset");
    if iconset.is_dir() {
        fs::remove_dir_all(&iconset).map_err(|error| format!("无法清理旧 iconset: {error}"))?;
    }
    fs::create_dir_all(&iconset).map_err(|error| format!("无法创建 iconset: {error}"))?;
    for size in [16, 32, 128, 256, 512] {
        for (suffix, pixels) in [("", size), ("@2x", size * 2)] {
            let output = iconset.join(format!("icon_{size}x{size}{suffix}.png"));
            let status = Command::new("sips")
                .args([
                    "-z",
                    &pixels.to_string(),
                    &pixels.to_string(),
                    icon_source.to_string_lossy().as_ref(),
                    "--out",
                    output.to_string_lossy().as_ref(),
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|error| format!("无法启动 sips: {error}"))?;
            exit_on_failure(status, "sips")?;
        }
    }
    let status = Command::new("iconutil")
        .args([
            "-c",
            "icns",
            iconset.to_string_lossy().as_ref(),
            "-o",
            resources.join("AppIcon.icns").to_string_lossy().as_ref(),
        ])
        .status()
        .map_err(|error| format!("无法启动 iconutil: {error}"))?;
    exit_on_failure(status, "iconutil")?;

    let status = Command::new("plutil")
        .args([
            "-lint",
            contents.join("Info.plist").to_string_lossy().as_ref(),
        ])
        .stdout(Stdio::null())
        .status()
        .map_err(|error| format!("无法启动 plutil: {error}"))?;
    exit_on_failure(status, "plutil")?;

    match Command::new("codesign")
        .args([
            "--force",
            "--deep",
            "--sign",
            "-",
            app.to_string_lossy().as_ref(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(_) => eprintln!("warning: ad-hoc codesign 失败；应用仍可本机运行"),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            eprintln!("warning: 未找到 codesign；跳过 ad-hoc 签名")
        }
        Err(error) => eprintln!("warning: codesign 执行失败: {error}"),
    }

    println!("App bundle: {}", app.display());
    Ok(app)
}

fn verify(args: Vec<String>) -> Result<(), String> {
    let quick = args.iter().any(|arg| arg == "--quick");
    let unknown: Vec<&String> = args
        .iter()
        .filter(|arg| arg.as_str() != "--quick")
        .collect();
    if !unknown.is_empty() {
        return Err("cargo x verify 仅支持 --quick".to_string());
    }

    println!("==> cargo fmt --all -- --check");
    run_status(
        Command::new("cargo")
            .current_dir(workspace_root())
            .args(["fmt", "--all", "--", "--check"]),
        "cargo fmt",
    )?;

    println!("==> cargo test --workspace");
    run_status(
        Command::new("cargo")
            .current_dir(workspace_root())
            .args(["test", "--workspace"]),
        "cargo test",
    )?;

    println!("==> cargo check -p mox-hmi-editor");
    run_status(
        Command::new("cargo")
            .current_dir(workspace_root())
            .args(["check", "-p", "mox-hmi-editor"]),
        "native cargo check",
    )?;

    println!("==> cargo check -p mox-hmi-studio --target wasm32-unknown-unknown");
    run_status(
        Command::new("cargo").current_dir(workspace_root()).args([
            "check",
            "-p",
            "mox-hmi-studio",
            "--target",
            "wasm32-unknown-unknown",
        ]),
        "wasm32 cargo check",
    )?;

    if quick {
        println!("Verify 完成（--quick：已跳过浏览器验收）");
        return Ok(());
    }

    e2e::run(Vec::new())?;
    control_e2e::run(Vec::new())?;
    println!("Verify 完成：格式、测试、WASM 与浏览器验收均通过");
    Ok(())
}

fn check_wasm_size(args: Vec<String>) -> Result<(), String> {
    if args.len() > 1 {
        return Err("用法: cargo x check-wasm-size [WASM 路径]".to_string());
    }
    ensure_tool("wasm-opt")?;
    let wasm = match args.first() {
        Some(path) => PathBuf::from(path),
        None => find_release_wasm(web_dir(false).parent().unwrap_or(Path::new(".")))?,
    };
    if !wasm.is_file() {
        return Err(format!(
            "未找到 release WASM: {}；请先运行 cargo x bundle",
            wasm.display()
        ));
    }

    let optimized = wasm.with_extension("wasm.opt");
    println!("优化 WASM: {}", wasm.display());
    let status = Command::new("wasm-opt")
        .args([
            "-Oz",
            "--strip-debug",
            wasm.to_string_lossy().as_ref(),
            "-o",
            optimized.to_string_lossy().as_ref(),
        ])
        .status()
        .map_err(|error| format!("无法启动 wasm-opt: {error}"))?;
    exit_on_failure(status, "wasm-opt")?;
    fs::rename(&optimized, &wasm).map_err(|error| format!("无法替换优化后的 WASM: {error}"))?;

    let bytes = fs::metadata(&wasm)
        .map_err(|error| format!("无法读取 WASM 大小: {error}"))?
        .len();
    let limit = 30 * 1024 * 1024;
    println!(
        "WASM: {bytes} bytes ({:.2} MiB)",
        bytes as f64 / 1_048_576.0
    );
    if bytes > limit {
        return Err(format!(
            "WASM 超出体积预算: {} bytes > {} bytes (30 MiB)",
            bytes, limit
        ));
    }
    println!("WASM 体积检查通过（上限 30 MiB）");
    Ok(())
}

fn find_release_wasm(root: &Path) -> Result<PathBuf, String> {
    let mut matches = Vec::new();
    collect_files(root, &mut |path| {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.ends_with(".wasm") && name.contains("_bg") {
            matches.push(path.to_path_buf());
        }
    })?;
    matches.sort();
    matches
        .into_iter()
        .next()
        .ok_or_else(|| format!("目录中没有找到 *_bg*.wasm: {}", root.display()))
}

fn font_sync(args: Vec<String>) -> Result<(), String> {
    if !args.is_empty() {
        return Err("cargo x font-sync 不接受额外参数".to_string());
    }
    ensure_tool("curl")?;
    ensure_tool("python3")?;

    let root = workspace_root();
    let fonts = root.join("apps/studio/public/assets/fonts");
    let output = fonts.join("NotoSerifCJKsc-Regular.otf");
    let web_output = fonts.join("NotoSerifCJKsc-Regular.woff2");
    let license = fonts.join("OFL.txt");
    let font_url = "https://raw.githubusercontent.com/notofonts/noto-cjk/main/Serif/OTF/SimplifiedChinese/NotoSerifCJKsc-Regular.otf";
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建字体目录: {error}"))?;
    }
    download_if_missing(&output, font_url, 20_000_000)?;
    if !license.exists() {
        return Err(format!(
            "缺少字体许可文件 {}；请从 Noto CJK 仓库补充 OFL.txt",
            license.display()
        ));
    }

    let regenerate_woff2 = !web_output.exists()
        || fs::metadata(&output).ok().and_then(|m| m.modified().ok())
            > fs::metadata(&web_output)
                .ok()
                .and_then(|m| m.modified().ok());
    if regenerate_woff2 {
        println!("生成 Web 字体: {}", web_output.display());
        let status = Command::new("python3")
            .args([
                "-m",
                "fontTools.ttLib.woff2",
                "compress",
                "--quiet",
                "-o",
                web_output.to_string_lossy().as_ref(),
                output.to_string_lossy().as_ref(),
            ])
            .status()
            .map_err(|error| format!("无法启动 fontTools: {error}"))?;
        exit_on_failure(status, "fontTools")?;
    }

    let font_data =
        fs::read(&output).map_err(|error| format!("无法读取字体 {}: {error}", output.display()))?;
    let face = ttf_parser::Face::parse(&font_data, 0)
        .map_err(|error| format!("无法解析字体 {}: {error}", output.display()))?;
    let missing: String = "吃豆人车站湿度送风机频率新建设备"
        .chars()
        .filter(|character| face.glyph_index(*character).is_none())
        .collect();
    if !missing.is_empty() {
        return Err(format!("字体缺少字符: {missing}"));
    }

    let bytes = fs::metadata(&output)
        .map_err(|error| format!("无法读取字体文件大小: {error}"))?
        .len();
    let web_bytes = fs::metadata(&web_output)
        .map_err(|error| format!("无法读取 Web 字体文件大小: {error}"))?
        .len();
    println!(
        "Font: {} ({:.1} MiB); Web: {} ({:.1} MiB)",
        output.display(),
        bytes as f64 / (1024.0 * 1024.0),
        web_output.display(),
        web_bytes as f64 / (1024.0 * 1024.0),
    );
    println!("字体许可证: {}", license.display());
    Ok(())
}

fn download_if_missing(path: &Path, url: &str, minimum_bytes: u64) -> Result<(), String> {
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() >= minimum_bytes.max(1)) {
        return Ok(());
    }
    println!("下载: {url}");
    let status = Command::new("curl")
        .args([
            "-fL",
            "--retry",
            "3",
            "--connect-timeout",
            "20",
            "-o",
            path.to_string_lossy().as_ref(),
            url,
        ])
        .status()
        .map_err(|error| format!("无法启动 curl: {error}"))?;
    exit_on_failure(status, "curl")?;
    Ok(())
}

fn collect_files(root: &Path, visitor: &mut impl FnMut(&Path)) -> Result<(), String> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let entries = fs::read_dir(&path)
            .map_err(|error| format!("无法读取目录 {}: {error}", path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("无法读取目录项: {error}"))?;
            let child = entry.path();
            if child.is_dir() {
                pending.push(child);
            } else {
                visitor(&child);
            }
        }
    }
    Ok(())
}

fn run_status(command: &mut Command, action: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("无法启动 {action}: {error}"))?;
    exit_on_failure(status, action)
}

fn clean(args: Vec<String>) -> Result<(), String> {
    if !args.is_empty() {
        return Err("cargo x clean 不接受额外参数".to_string());
    }
    clean_workspace(&workspace_root())
}

fn clean_workspace(root: &Path) -> Result<(), String> {
    println!("清理构建产物: {}", root.display());
    for name in ["dist", "target"] {
        let path = root.join(name);
        if !path.exists() {
            println!("  跳过 {name}/（不存在）");
            continue;
        }
        fs::remove_dir_all(&path)
            .map_err(|error| format!("无法删除 {}: {error}", path.display()))?;
        println!("  已删除 {}/", path.display());
    }
    println!("Clean 完成：target/ 与 dist/ 已清理");
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let status = Command::new("ditto")
        .arg(source)
        .arg(destination)
        .status()
        .map_err(|error| format!("无法启动 ditto: {error}"))?;
    exit_on_failure(
        status,
        &format!("复制 {} -> {}", source.display(), destination.display()),
    )
}

fn configure_runner_index(index: &Path) -> Result<(), String> {
    let html =
        fs::read_to_string(index).map_err(|error| format!("无法读取运行器 index.html: {error}"))?;
    let updated = inject_runtime_marker(&html);
    if updated == html {
        return Err("运行器 index.html 缺少可注入的 </head> 或启动脚本".to_string());
    }
    fs::write(index, updated).map_err(|error| format!("无法写入运行器 index.html: {error}"))
}

fn inject_runtime_marker(html: &str) -> String {
    if html.contains(RUNTIME_MARKER) {
        return html.to_string();
    }
    let tag = format!("<script>{RUNTIME_MARKER}</script>");
    let boot = "<script src=\"webgpu-compat.js\"></script>";
    if html.contains(boot) {
        return html.replacen(boot, &format!("{tag}\n    {boot}"), 1);
    }
    html.replacen("</head>", &format!("    {tag}\n  </head>"), 1)
}

fn ensure_tool(tool: &str) -> Result<(), String> {
    match Command::new(tool).arg("--help").output() {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Err(format!("缺少 macOS 打包工具 `{tool}`"))
        }
        Err(error) => Err(format!("无法执行 `{tool}`: {error}")),
    }
}

fn prepare_web_dir(debug: bool) -> Result<(), String> {
    ensure_dx()?;
    let output = web_dir(debug);
    if output.is_dir() {
        std::fs::remove_dir_all(&output)
            .map_err(|error| format!("无法清理旧 Web 构建 {}: {error}", output.display()))?;
    }

    println!(
        "构建 Web 资源 ({})",
        if debug { "debug" } else { "release" }
    );
    let mut command = Command::new("dx");
    command
        .current_dir(workspace_root().join("apps/studio"))
        .args(["build", "--web"]);
    if !debug {
        command.args(["--release", "--debug-symbols", "false"]);
    }
    let status = command
        .status()
        .map_err(|error| format!("无法启动 dx build: {error}"))?;
    exit_on_failure(status, "dx build")?;
    if !output.join("index.html").is_file() {
        return Err(format!("Web 构建缺少 index.html: {}", output.display()));
    }
    Ok(())
}

fn build_editor(debug: bool) -> Result<PathBuf, String> {
    println!(
        "构建 macOS 原生编辑器 ({})",
        if debug { "debug" } else { "release" }
    );
    let mut command = Command::new("cargo");
    command
        .current_dir(workspace_root())
        .args(["build", "-p", "mox-hmi-editor"]);
    if !debug {
        command.arg("--release");
    }
    let status = command
        .status()
        .map_err(|error| format!("无法启动 cargo build: {error}"))?;
    exit_on_failure(status, "cargo build")?;

    let binary = workspace_root()
        .join("target")
        .join(if debug { "debug" } else { "release" })
        .join("mox-hmi-editor");
    if !binary.is_file() {
        return Err(format!("编辑器二进制不存在: {}", binary.display()));
    }
    Ok(binary)
}

fn web_dir(debug: bool) -> PathBuf {
    workspace_root()
        .join("target/dx/mox-hmi-studio")
        .join(if debug { "debug" } else { "release" })
        .join("web/public")
}

fn wait_until_ready(child: &mut Child, host: &str, port: &str) -> Result<(), String> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if TcpStream::connect((host, port.parse::<u16>().unwrap_or(0))).is_ok() {
            return Ok(());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("检查 dx serve 状态失败: {error}"))?
        {
            return Err(format!("dx serve 提前退出: {status}"));
        }
        thread::sleep(Duration::from_millis(250));
    }
    let _ = child.kill();
    Err(format!(
        "等待 dx serve 启动超时（{} 秒）",
        STARTUP_TIMEOUT.as_secs()
    ))
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let opener = "xdg-open";

    Command::new(opener)
        .arg(url)
        .status()
        .map_err(|error| format!("无法打开浏览器: {error}"))?;
    Ok(())
}

fn display_host(addr: &str) -> String {
    match addr {
        "0.0.0.0" | "::" => DEFAULT_ADDR.to_string(),
        other => other.to_string(),
    }
}

fn ensure_dx() -> Result<(), String> {
    let output = match Command::new("dx").arg("--version").output() {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            let details = String::from_utf8_lossy(&output.stderr);
            return Err(format!("dx 不可用: {}", details.trim()));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err("未找到 dx。请安装与本项目匹配的 Dioxus CLI 0.8：\n  cargo install dioxus-cli --version 0.8.0-alpha.3 --locked --force".to_string());
        }
        Err(error) => return Err(format!("无法执行 dx: {error}")),
    };
    let version = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if version
        .split_whitespace()
        .any(|part| part.starts_with("0.8."))
    {
        return Ok(());
    }
    Err(format!(
        "dx 版本不匹配：{}。Runner 使用 Dioxus 0.8，请运行：\n  cargo install dioxus-cli --version 0.8.0-alpha.3 --locked --force",
        version.trim()
    ))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask 必须位于 workspace 根目录")
        .to_path_buf()
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let Some(index) = args.iter().position(|arg| arg == flag) else {
        return false;
    };
    args.remove(index);
    true
}

fn has_option(args: &[String], name: &str) -> bool {
    let prefix = format!("{name}=");
    args.iter()
        .any(|arg| arg == name || arg.starts_with(&prefix))
}

fn option_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().enumerate().find_map(|(index, arg)| {
        if arg == name {
            return args.get(index + 1).map(String::as_str);
        }
        arg.strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
    })
}

fn exit_on_failure(status: ExitStatus, action: &str) -> Result<(), String> {
    if status.success() {
        return Ok(());
    }
    if let Some(code) = status.code() {
        std::process::exit(code);
    }
    Err(format!("{action} 被信号终止"))
}

fn print_help() {
    println!(
        "MOX HMI xtask\n\n\
         用法:\n  cargo x edit                   以快速 debug 构建启动 macOS 原生编辑器\n  \
         cargo x edit --port 17900      覆盖编辑器端口\n  \
         cargo x edit --release         以 release 构建启动编辑器（接近打包效果）\n\n  \
         cargo x run                    启动浏览器运行器，不显示编辑器\n  \
         cargo x run --port 9000        覆盖运行器端口\n  \
         cargo x run --release          以 release 模式启动运行器\n  \
         cargo x run --no-open          不自动打开浏览器\n\n  \
         cargo x bundle                 生成 dist/ 下的 DMG 与运行器 Web 目录\n\n\
         cargo x verify                 格式、测试、WASM 和两套浏览器验收\n  \
         cargo x verify --quick         跳过浏览器验收的快速检查\n\n  \
         cargo x e2e                    用快速 debug 构建运行 21 条浏览器交互验收\n  \
         cargo x e2e --release          用 release 构建运行浏览器交互验收\n  \
         cargo x control-e2e            用快速 debug 构建运行 MCP 控制隧道验收\n  \
         cargo x control-e2e --release  用 release 构建运行控制隧道验收\n\n  \
         cargo x check-wasm-size        用 wasm-opt 优化并检查 30 MiB 预算\n  \
         cargo x font-sync              重新下载 Noto Serif CJK 字体并生成 WOFF2\n\n\
         cargo x clean                  删除 target/ 和 dist/ 构建产物\n\n\
         开发命令默认使用 debug 构建；cargo x bundle 始终使用 release 构建。\n\
         触觉反馈仅在 cargo x edit 中可用。"
    );
}

#[cfg(test)]
mod tests {
    use super::{clean_workspace, inject_runtime_marker};
    use std::{env, fs, process};

    #[test]
    fn runtime_marker_is_injected_before_compat_script() {
        let html = "<head><script src=\"webgpu-compat.js\"></script></head>";
        let updated = inject_runtime_marker(html);
        assert!(updated.contains("window.__MOX_HMI_RUNTIME_ONLY__ = true;"));
        assert!(
            updated.find("__MOX_HMI_RUNTIME_ONLY__").unwrap()
                < updated.find("webgpu-compat.js").unwrap()
        );
    }

    #[test]
    fn runtime_marker_injection_is_idempotent() {
        let html = "<head><script src=\"webgpu-compat.js\"></script></head>";
        let once = inject_runtime_marker(html);
        assert_eq!(inject_runtime_marker(&once), once);
    }

    #[test]
    fn clean_removes_target_and_dist_only() {
        let root = env::temp_dir().join(format!("mox-hmi-clean-test-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("target/nested")).unwrap();
        fs::create_dir_all(root.join("dist/nested")).unwrap();
        fs::create_dir_all(root.join("keep")).unwrap();
        fs::write(root.join("target/nested/artifact"), b"artifact").unwrap();
        fs::write(root.join("dist/nested/package"), b"package").unwrap();
        fs::write(root.join("Cargo.toml"), b"[workspace]").unwrap();

        clean_workspace(&root).unwrap();

        assert!(!root.join("target").exists());
        assert!(!root.join("dist").exists());
        assert!(root.join("Cargo.toml").is_file());
        assert!(root.join("keep").is_dir());
        fs::remove_dir_all(root).unwrap();
    }
}
