use std::{
    fs::{self, File},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime},
};

pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(1_200);
pub const BUILD_TIMEOUT: Duration = Duration::from_secs(1_200);

pub struct DevServer {
    child: Child,
    log_path: PathBuf,
    web_output: PathBuf,
    stale_index_stamp: Option<SystemTime>,
    pub url: String,
}

impl DevServer {
    pub fn start(root: &Path, port: u16, release: bool) -> Result<Self, String> {
        let log_path = std::env::temp_dir().join("mox-hmi-dx-serve.log");
        let log = File::create(&log_path)
            .map_err(|error| format!("无法创建 dx 日志 {}: {error}", log_path.display()))?;
        let stderr = log
            .try_clone()
            .map_err(|error| format!("无法复制 dx 日志句柄: {error}"))?;

        let mut command = Command::new("dx");
        command.current_dir(root.join("apps/studio")).args([
            "serve",
            "--web",
            "--addr",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--open",
            "false",
            "--watch",
            "false",
            "--interactive",
            "false",
            "--debug-symbols",
            "false",
        ]);
        if release {
            command.arg("--release");
        }
        command.stdout(Stdio::from(log)).stderr(Stdio::from(stderr));

        let web_output = root
            .join("target/dx/mox-hmi-studio")
            .join(if release { "release" } else { "debug" })
            .join("web/public");
        let stale_index_stamp = fs::metadata(web_output.join("index.html"))
            .and_then(|metadata| metadata.modified())
            .ok();

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let child = command
            .spawn()
            .map_err(|error| format!("无法启动 dx serve: {error}"))?;
        let url = format!("http://127.0.0.1:{port}/");
        Ok(Self {
            child,
            log_path,
            web_output,
            stale_index_stamp,
            url,
        })
    }

    pub fn wait_ready(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        while Instant::now() < deadline {
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|error| format!("检查 dx serve 状态失败: {error}"))?
            {
                return Err(format!(
                    "dx serve 提前退出（{status}）:\n{}",
                    self.log_tail()
                ));
            }
            if app_http_ready(&self.url) {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
        if !app_http_ready(&self.url) {
            return Err(format!("等待 dx serve 启动超时:\n{}", self.log_tail()));
        }

        let deadline = Instant::now() + BUILD_TIMEOUT;
        while Instant::now() < deadline {
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|error| format!("检查 dx serve 状态失败: {error}"))?
            {
                return Err(format!(
                    "dx serve 构建期间退出（{status}）:\n{}",
                    self.log_tail()
                ));
            }
            let log = self.log_contents();
            if log.contains("Build completed successfully")
                || build_output_ready(&self.web_output, self.stale_index_stamp)
            {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(500));
        }
        Err(format!("dx serve 构建超时:\n{}", self.log_tail()))
    }

    fn log_contents(&self) -> String {
        fs::read_to_string(&self.log_path).unwrap_or_default()
    }

    fn log_tail(&self) -> String {
        let log = self.log_contents();
        if log.len() <= 8_000 {
            log
        } else {
            let start = log
                .char_indices()
                .rev()
                .nth(8_000)
                .map(|(index, _)| index)
                .unwrap_or(0);
            log[start..].to_string()
        }
    }
}

fn build_output_ready(public: &Path, stale_index_stamp: Option<SystemTime>) -> bool {
    let index = public.join("index.html");
    if !index.is_file() {
        return false;
    }
    if let Some(stale) = stale_index_stamp {
        let current = fs::metadata(&index)
            .and_then(|metadata| metadata.modified())
            .ok();
        if current.is_none_or(|current| current <= stale) {
            return false;
        }
    }
    let mut wasm = false;
    let _ = collect_files(public, &mut |path| {
        if path.extension().and_then(|value| value.to_str()) == Some("wasm")
            && fs::metadata(path).is_ok_and(|metadata| metadata.len() > 0)
        {
            wasm = true;
        }
    });
    wasm
}

fn collect_files(root: &Path, visitor: &mut impl FnMut(&Path)) -> std::io::Result<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path)? {
            let child = entry?.path();
            if child.is_dir() {
                pending.push(child);
            } else {
                visitor(&child);
            }
        }
    }
    Ok(())
}

impl Drop for DevServer {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_some() {
            return;
        }
        #[cfg(unix)]
        {
            let group = format!("-{}", self.child.id());
            let _ = Command::new("kill")
                .args(["-TERM", &group])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            thread::sleep(Duration::from_millis(250));
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = Command::new("kill")
                    .args(["-KILL", &group])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn choose_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("无法分配本地端口: {error}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("无法读取本地端口: {error}"))
}

fn app_http_ready(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let Some((host, port)) = authority.rsplit_once(':') else {
        return false;
    };
    let Ok(port) = port.parse::<u16>() else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect((host, port)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request = format!("GET / HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = Vec::new();
    if stream.read_to_end(&mut response).is_err() {
        return false;
    }
    let response = String::from_utf8_lossy(&response);
    response.starts_with("HTTP/")
        && response.contains(" 200 ")
        && response.contains("id=\"boot-shell\"")
}
