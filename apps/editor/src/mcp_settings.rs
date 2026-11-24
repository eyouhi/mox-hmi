use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

const SETTINGS_ENV: &str = "MOX_HMI_MCP_SETTINGS_PATH";
const SETTINGS_FILE: &str = "control-tunnel.toml";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ControlTunnelConfig {
    #[serde(default)]
    enabled: bool,
}

pub struct McpSettings {
    path: PathBuf,
    enabled: AtomicBool,
    write_lock: Mutex<()>,
}

impl McpSettings {
    pub fn load() -> Self {
        let path = settings_path();
        let config = read_config(&path);
        Self {
            path,
            enabled: AtomicBool::new(config.enabled),
            write_lock: Mutex::new(()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| "MCP 配置锁已损坏".to_string())?;
        write_config(&self.path, enabled)?;
        self.enabled.store(enabled, Ordering::Release);
        Ok(())
    }
}

pub fn mcp_endpoint(address: SocketAddr, token: Option<&str>) -> String {
    let mut endpoint = format!("http://{address}/mcp");
    if let Some(token) = token {
        endpoint.push_str("?token=");
        endpoint.push_str(&encode_query_component(token));
    }
    endpoint
}

fn settings_path() -> PathBuf {
    if let Some(path) = env::var_os(SETTINGS_ENV) {
        return PathBuf::from(path);
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library/Application Support/MOX HMI Studio")
            .join(SETTINGS_FILE);
    }
    env::temp_dir().join(SETTINGS_FILE)
}

fn read_config(path: &Path) -> ControlTunnelConfig {
    match fs::read_to_string(path) {
        Ok(contents) => match toml::from_str(&contents) {
            Ok(config) => config,
            Err(error) => {
                eprintln!(
                    "mox-hmi-editor: MCP 配置无效，按关闭处理（{}）: {error}",
                    path.display()
                );
                ControlTunnelConfig::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ControlTunnelConfig::default()
        }
        Err(error) => {
            eprintln!(
                "mox-hmi-editor: 读取 MCP 配置失败，按关闭处理（{}）: {error}",
                path.display()
            );
            ControlTunnelConfig::default()
        }
    }
}

fn write_config(path: &Path, enabled: bool) -> Result<(), String> {
    let contents = toml::to_string_pretty(&ControlTunnelConfig { enabled })
        .map_err(|error| format!("序列化 MCP 配置失败: {error}"))?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建 MCP 配置目录 {} 失败: {error}", parent.display()))?;
    }
    let staging = path.with_extension("toml.tmp");
    fs::write(&staging, contents)
        .map_err(|error| format!("写入 MCP 临时配置 {} 失败: {error}", staging.display()))?;
    fs::rename(&staging, path)
        .map_err(|error| format!("更新 MCP 配置 {} 失败: {error}", path.display()))
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address() -> SocketAddr {
        "127.0.0.1:17880".parse().unwrap()
    }

    #[test]
    fn missing_and_empty_configs_are_disabled() {
        let directory = tempfile::tempdir().unwrap();
        assert!(!read_config(&directory.path().join("missing.toml")).enabled);

        let empty = directory.path().join("empty.toml");
        fs::write(&empty, "").unwrap();
        assert!(!read_config(&empty).enabled);
    }

    #[test]
    fn enabled_setting_round_trips() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("control-tunnel.toml");

        write_config(&path, true).unwrap();
        assert!(read_config(&path).enabled);
        write_config(&path, false).unwrap();
        assert!(!read_config(&path).enabled);
    }

    #[test]
    fn endpoint_includes_an_encoded_optional_token() {
        assert_eq!(mcp_endpoint(address(), None), "http://127.0.0.1:17880/mcp");
        assert_eq!(
            mcp_endpoint(address(), Some("token 1/+")),
            "http://127.0.0.1:17880/mcp?token=token%201%2F%2B"
        );
    }
}
