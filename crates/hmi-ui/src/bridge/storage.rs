#[cfg(target_arch = "wasm32")]
pub fn load_raw() -> Result<Option<String>, String> {
    let window = web_sys::window().ok_or_else(|| "window 不可用".to_string())?;
    let storage = window
        .local_storage()
        .map_err(|error| format!("localStorage 不可用：{error:?}"))?
        .ok_or_else(|| "localStorage 不可用".to_string())?;
    storage
        .get_item(hmi_core::STORAGE_KEY)
        .map_err(|error| format!("读取 localStorage 失败：{error:?}"))
}

#[cfg(target_arch = "wasm32")]
pub fn save_raw(raw: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| "window 不可用".to_string())?;
    let storage = window
        .local_storage()
        .map_err(|error| format!("localStorage 不可用：{error:?}"))?
        .ok_or_else(|| "localStorage 不可用".to_string())?;
    storage
        .set_item(hmi_core::STORAGE_KEY, raw)
        .map_err(|error| format!("写入 localStorage 失败：{error:?}"))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_raw() -> Result<Option<String>, String> {
    let path = native_storage_path();
    match std::fs::read_to_string(&path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("读取 {} 失败：{error}", path.display())),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_raw(raw: &str) -> Result<(), String> {
    let path = native_storage_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("创建 {} 失败：{error}", parent.display()))?;
    }
    let staging = path.with_extension("json.tmp");
    std::fs::write(&staging, raw)
        .map_err(|error| format!("写入 {} 失败：{error}", staging.display()))?;
    std::fs::rename(&staging, &path)
        .map_err(|error| format!("替换 {} 失败：{error}", path.display()))
}

#[cfg(not(target_arch = "wasm32"))]
fn native_storage_path() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("MOX_HMI_STORAGE_PATH") {
        return std::path::PathBuf::from(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return std::path::PathBuf::from(home)
            .join("Library/Application Support/MOX HMI Studio/project.json");
    }
    std::env::temp_dir().join("mox-hmi-project.json")
}
