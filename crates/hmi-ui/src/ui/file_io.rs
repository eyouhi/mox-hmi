use dioxus::html::FileData;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};
#[cfg(target_arch = "wasm32")]
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement};

#[cfg(target_arch = "wasm32")]
pub fn download_json(filename: &str, body: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| "window 不可用".to_string())?;
    let document = window
        .document()
        .ok_or_else(|| "document 不可用".to_string())?;
    let parts = js_sys::Array::of1(&JsValue::from_str(body));
    let options = BlobPropertyBag::new();
    options.set_type("application/json;charset=utf-8");
    let blob = Blob::new_with_str_sequence_and_options(&parts, &options)
        .map_err(|error| format!("创建文件失败：{error:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|error| format!("创建下载地址失败：{error:?}"))?;
    let anchor = document
        .create_element("a")
        .map_err(|error| format!("创建下载节点失败：{error:?}"))?
        .dyn_into::<HtmlAnchorElement>()
        .map_err(|_| "下载节点类型错误".to_string())?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    web_sys::Url::revoke_object_url(&url)
        .map_err(|error| format!("释放下载地址失败：{error:?}"))?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn download_json(filename: &str, body: &str) -> Result<(), String> {
    let path = macos_choose_save_path(filename)?;
    std::fs::write(&path, body).map_err(|error| format!("写入 {path} 失败：{error}"))
}

#[cfg(target_arch = "wasm32")]
pub async fn read_uploaded_text(file: FileData) -> Result<String, String> {
    let size = file.size();
    if size > hmi_core::MAX_IMPORT_BYTES as u64 {
        return Err("工程文件超过 5 MB 限制".to_string());
    }
    let bytes = file
        .read_bytes()
        .await
        .map_err(|error| format!("读取文件失败：{error}"))?;
    String::from_utf8(bytes.to_vec()).map_err(|_| "工程文件必须是 UTF-8 JSON".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn read_uploaded_text(_file: FileData) -> Result<String, String> {
    Err("原生端请使用文件选择对话框导入工程".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn pick_json_text() -> Result<Option<String>, String> {
    let Some(path) = macos_choose_open_path()? else {
        return Ok(None);
    };
    let metadata =
        std::fs::metadata(&path).map_err(|error| format!("读取 {path} 失败：{error}"))?;
    if metadata.len() > hmi_core::MAX_IMPORT_BYTES as u64 {
        return Err("工程文件超过 5 MB 限制".to_string());
    }
    std::fs::read_to_string(&path)
        .map(Some)
        .map_err(|error| format!("读取 {path} 失败：{error}"))
}

#[cfg(target_os = "macos")]
fn macos_choose_open_path() -> Result<Option<String>, String> {
    let script = r#"try
POSIX path of (choose file with prompt "导入 MOX HMI 工程" of type {"json"})
on error number -128
return ""
end try"#;
    run_osascript(script)
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "macos")))]
fn macos_choose_open_path() -> Result<Option<String>, String> {
    Err("当前平台暂不支持原生文件选择".to_string())
}

#[cfg(target_os = "macos")]
fn macos_choose_save_path(filename: &str) -> Result<String, String> {
    let filename = filename.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"try
POSIX path of (choose file name with prompt "导出 MOX HMI 工程" default name "{filename}")
on error number -128
return ""
end try"#
    );
    run_osascript(&script)?.ok_or_else(|| "已取消导出".to_string())
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "macos")))]
fn macos_choose_save_path(_filename: &str) -> Result<String, String> {
    Err("当前平台暂不支持原生文件选择".to_string())
}

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Result<Option<String>, String> {
    let output = std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|error| format!("无法打开文件对话框：{error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!path.is_empty()).then_some(path))
}
