use dioxus::prelude::*;
use hmi_core::{ProjectDocument, TagValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(feature = "server")]
use hmi_core::{TagSimulation, TagSnapshot, bas_demo, from_json};
#[cfg(feature = "server")]
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[cfg(feature = "server")]
use tokio::{sync::broadcast, task::JoinHandle, time::MissedTickBehavior};

#[cfg(feature = "server")]
const RUNNER_REFRESH_MS: u64 = 250;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunnerBootstrap {
    pub project: ProjectDocument,
    pub tags: RunnerTagFrame,
    pub project_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunnerTagFrame {
    pub revision: u64,
    pub generated_at_ms: u64,
    pub values: BTreeMap<String, TagValue>,
}

impl RunnerTagFrame {
    #[cfg(feature = "server")]
    fn from_snapshot(snapshot: &TagSnapshot) -> Self {
        Self {
            revision: snapshot.revision(),
            generated_at_ms: current_unix_ms(),
            values: snapshot.values().clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
enum BootstrapState {
    Loading,
    Ready,
    Failed(String),
}

#[component]
pub fn RunnerRoot() -> Element {
    #[allow(unused_mut)]
    let mut bootstrap = use_signal(|| BootstrapState::Loading);

    #[cfg(target_arch = "wasm32")]
    use_future(move || async move {
        if !mox_hmi_ui::bridge::runtime_only() {
            match mox_hmi_ui::bridge::initialize() {
                Ok(()) => bootstrap.set(BootstrapState::Ready),
                Err(error) => bootstrap.set(BootstrapState::Failed(error)),
            }
            return;
        }
        match runner_bootstrap().await {
            Ok(data) => {
                let project_notice = data.project_notice.clone();
                if let Err(error) = mox_hmi_ui::bridge::initialize_remote(
                    data.project,
                    data.tags.values,
                    data.tags.revision,
                ) {
                    bootstrap.set(BootstrapState::Failed(error));
                    return;
                }
                mox_hmi_ui::bridge::install_remote_tag_writer(enqueue_runner_tag_write);
                spawn(async move {
                    run_tag_stream().await;
                });
                if let Some(message) = project_notice {
                    mox_hmi_ui::bridge::set_notice(
                        mox_hmi_ui::bridge::NoticeLevel::Warning,
                        message,
                    );
                }
                bootstrap.set(BootstrapState::Ready);
            }
            Err(error) => {
                bootstrap.set(BootstrapState::Failed(format!(
                    "无法连接运行器服务端：{error}"
                )));
            }
        }
    });

    match bootstrap() {
        BootstrapState::Ready => rsx! {
            mox_hmi_ui::App {}
        },
        BootstrapState::Loading => rsx! {
            div { class: "boot-card", role: "status", aria_live: "polite",
                span { class: "boot-mark", aria_hidden: "true", "M" }
                span { class: "boot-spinner", aria_hidden: "true" }
                div {
                    div { class: "boot-title", "MOX HMI Runtime" }
                    div { class: "boot-copy", "正在加载运行环境…" }
                }
            }
        },
        BootstrapState::Failed(message) => rsx! {
            div { class: "boot-card", role: "alert",
                span { class: "boot-mark", aria_hidden: "true", "!" }
                div {
                    div { class: "boot-title", "运行环境不可用" }
                    div { class: "boot-copy", "{message}" }
                }
            }
        },
    }
}

#[get("/api/runner/bootstrap")]
async fn runner_bootstrap() -> Result<RunnerBootstrap, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let hub = runner_hub().map_err(ServerFnError::new)?;
        return hub.bootstrap().map_err(ServerFnError::new);
    }
    #[cfg(not(feature = "server"))]
    Err(ServerFnError::new(
        "运行器服务端 API 仅在 server 构建中可用",
    ))
}

#[get("/api/runner/tags")]
async fn runner_tag_stream()
-> Result<dioxus::fullstack::ServerEvents<RunnerTagFrame>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let hub = runner_hub().map_err(ServerFnError::new)?;
        let mut receiver = hub.subscribe();
        let events = dioxus::fullstack::ServerEvents::new(move |mut sender| async move {
            loop {
                match receiver.recv().await {
                    Ok(frame) => {
                        if sender.send(frame).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("运行器实时流落后，跳过 {skipped} 个 Tag 帧");
                        if let Ok(frame) = hub.frame()
                            && sender.send(frame).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        return Ok(events);
    }
    #[cfg(not(feature = "server"))]
    Err(ServerFnError::new("运行器实时流仅在 server 构建中可用"))
}

#[post("/api/runner/tags/write")]
async fn write_runner_tag(tag_id: String, value: TagValue) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        let hub = runner_hub().map_err(ServerFnError::new)?;
        return hub.write(tag_id, value).map_err(ServerFnError::new);
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (tag_id, value);
        Err(ServerFnError::new("Tag 写入 API 仅在 server 构建中可用"))
    }
}

#[cfg(target_arch = "wasm32")]
fn enqueue_runner_tag_write(tag_id: String, value: TagValue) {
    spawn(async move {
        if let Err(error) = write_runner_tag(tag_id, value).await {
            mox_hmi_ui::bridge::set_notice(
                mox_hmi_ui::bridge::NoticeLevel::Error,
                format!("服务端拒绝写入 Tag：{error}"),
            );
        }
    });
}

#[cfg(target_arch = "wasm32")]
async fn run_tag_stream() {
    loop {
        match runner_tag_stream().await {
            Ok(mut events) => loop {
                match events.recv().await {
                    Some(Ok(frame)) => {
                        if let Err(error) = mox_hmi_ui::bridge::apply_remote_tag_snapshot(
                            frame.values,
                            frame.revision,
                        ) {
                            mox_hmi_ui::bridge::set_notice(
                                mox_hmi_ui::bridge::NoticeLevel::Error,
                                error,
                            );
                        }
                    }
                    Some(Err(error)) => {
                        mox_hmi_ui::bridge::set_notice(
                            mox_hmi_ui::bridge::NoticeLevel::Warning,
                            format!("实时数据流中断：{error}"),
                        );
                        break;
                    }
                    None => break,
                }
            },
            Err(error) => {
                mox_hmi_ui::bridge::set_notice(
                    mox_hmi_ui::bridge::NoticeLevel::Warning,
                    format!("无法建立实时数据流：{error}"),
                );
            }
        }
        mox_hmi_ui::bridge::set_remote_runner_connected(false);
        mox_hmi_ui::timer::sleep_ms(1_000).await;
    }
}

#[cfg(feature = "server")]
fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

#[cfg(feature = "server")]
static RUNNER_HUB: OnceLock<Result<RunnerHub, String>> = OnceLock::new();

#[cfg(feature = "server")]
#[derive(Clone)]
struct RunnerHub {
    shared: Arc<RunnerShared>,
}

#[cfg(feature = "server")]
struct RunnerShared {
    project: ProjectDocument,
    project_notice: Option<String>,
    simulation: Mutex<TagSimulation>,
    events: broadcast::Sender<RunnerTagFrame>,
}

#[cfg(feature = "server")]
impl RunnerHub {
    fn start() -> Result<Self, String> {
        let loaded = load_runner_project()?;
        let simulation = TagSimulation::new(&loaded.project);
        let (events, _) = broadcast::channel(64);
        let hub = Self {
            shared: Arc::new(RunnerShared {
                project: loaded.project,
                project_notice: loaded.notice,
                simulation: Mutex::new(simulation),
                events,
            }),
        };
        hub.clone().spawn_ticker()?;
        Ok(hub)
    }

    fn spawn_ticker(self) -> Result<JoinHandle<()>, String> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|error| format!("无法启动运行器仿真任务：{error}"))?;
        Ok(runtime.spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(RUNNER_REFRESH_MS));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(error) = self.tick(RUNNER_REFRESH_MS) {
                    tracing::error!("运行器仿真推进失败：{error}");
                }
            }
        }))
    }

    fn bootstrap(&self) -> Result<RunnerBootstrap, String> {
        Ok(RunnerBootstrap {
            project: self.shared.project.clone(),
            tags: self.frame()?,
            project_notice: self.shared.project_notice.clone(),
        })
    }

    fn frame(&self) -> Result<RunnerTagFrame, String> {
        let simulation = self
            .shared
            .simulation
            .lock()
            .map_err(|_| "Tag 仿真锁已损坏".to_string())?;
        Ok(RunnerTagFrame::from_snapshot(simulation.snapshot()))
    }

    fn tick(&self, dt_ms: u64) -> Result<(), String> {
        let mut simulation = self
            .shared
            .simulation
            .lock()
            .map_err(|_| "Tag 仿真锁已损坏".to_string())?;
        simulation.tick(dt_ms);
        let _ = self
            .shared
            .events
            .send(RunnerTagFrame::from_snapshot(simulation.snapshot()));
        Ok(())
    }

    fn write(&self, tag_id: String, value: TagValue) -> Result<(), String> {
        let mut simulation = self
            .shared
            .simulation
            .lock()
            .map_err(|_| "Tag 仿真锁已损坏".to_string())?;
        simulation
            .write(&tag_id, value)
            .map_err(|error| error.to_string())?;
        let _ = self
            .shared
            .events
            .send(RunnerTagFrame::from_snapshot(simulation.snapshot()));
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<RunnerTagFrame> {
        self.shared.events.subscribe()
    }
}

#[cfg(feature = "server")]
fn runner_hub() -> Result<RunnerHub, String> {
    let hub = RUNNER_HUB
        .get_or_init(RunnerHub::start)
        .as_ref()
        .map_err(Clone::clone)?;
    Ok(hub.clone())
}

#[cfg(feature = "server")]
#[cfg(feature = "server")]
struct LoadedRunnerProject {
    project: ProjectDocument,
    notice: Option<String>,
}

#[cfg(feature = "server")]
fn load_runner_project() -> Result<LoadedRunnerProject, String> {
    load_runner_project_from_path(runner_project_path())
}

#[cfg(feature = "server")]
fn load_runner_project_from_path(path: PathBuf) -> Result<LoadedRunnerProject, String> {
    if !path.is_file() {
        let message = format!(
            "未找到运行器工程 {}，当前使用内置 BAS 示例。可设置 MOX_HMI_RUNNER_PROJECT 指定工程。",
            path.display()
        );
        tracing::warn!("{message}");
        return Ok(LoadedRunnerProject {
            project: bas_demo(),
            notice: Some(message),
        });
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|error| format!("读取运行器工程 {} 失败：{error}", path.display()))?;
    let project = from_json(&raw)
        .map_err(|error| format!("解析运行器工程 {} 失败：{error}", path.display()))?;
    tracing::info!(
        "运行器加载工程 {:?}（{} 张画面，{} 个 Tag）：{}",
        project.name,
        project.screens.len(),
        project.tags.len(),
        path.display()
    );
    Ok(LoadedRunnerProject {
        project,
        notice: None,
    })
}

#[cfg(feature = "server")]
fn runner_project_path() -> PathBuf {
    if let Some(path) = std::env::var_os("MOX_HMI_RUNNER_PROJECT") {
        return PathBuf::from(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return Path::new(&home).join("Library/Application Support/MOX HMI Studio/project.json");
    }
    PathBuf::from("project.json")
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::{RunnerHub, RunnerTagFrame, load_runner_project_from_path};
    use hmi_core::{TagSimulation, bas_demo};
    use std::path::PathBuf;

    #[test]
    fn tag_frame_round_trips_all_values() {
        let project = bas_demo();
        let simulation = TagSimulation::new(&project);
        let frame = RunnerTagFrame::from_snapshot(simulation.snapshot());
        let json = serde_json::to_string(&frame).unwrap();
        let decoded: RunnerTagFrame = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.values, frame.values);
        assert_eq!(decoded.revision, frame.revision);
    }

    #[test]
    fn hub_rejects_unknown_tags() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let _guard = runtime.enter();
        let hub = RunnerHub::start().unwrap();
        assert!(
            hub.write("missing.tag".into(), hmi_core::TagValue::Bool(true))
                .is_err()
        );
    }

    #[test]
    fn missing_project_uses_demo_and_returns_notice() {
        let path = PathBuf::from(format!(
            "/tmp/mox-hmi-missing-runner-project-{}.json",
            std::process::id()
        ));
        let loaded = load_runner_project_from_path(path.clone()).unwrap();
        assert_eq!(loaded.project.id, bas_demo().id);
        assert!(
            loaded
                .notice
                .as_deref()
                .is_some_and(|notice| notice.contains(&path.display().to_string()))
        );
    }
}
