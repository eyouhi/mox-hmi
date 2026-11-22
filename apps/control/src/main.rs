use mox_hmi_control::{ControlHub, http, mcp};
use std::{env, net::IpAddr};
use tokio::net::TcpListener;

#[derive(Debug)]
struct Config {
    addr: IpAddr,
    port: u16,
    token: Option<String>,
}

impl Config {
    fn parse() -> Result<Self, String> {
        let mut addr = "127.0.0.1".to_string();
        let mut port = hmi_control_protocol::DEFAULT_CONTROL_PORT;
        let mut token = None;
        let mut args = env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--addr" => addr = next_value(&mut args, "--addr")?,
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

        Ok(Self {
            addr: addr
                .parse()
                .map_err(|error| format!("无效监听地址: {error}"))?,
            port,
            token,
        })
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{option} 缺少参数值"))
}

fn print_help() {
    println!(
        "MOX HMI control tunnel\n\n\
         用法:\n  cargo run -p mox-hmi-control -- [OPTIONS]\n\n\
         选项:\n  --addr <ADDR>     WebSocket 监听地址，默认 127.0.0.1\n  \
         --port <PORT>     WebSocket 端口，默认 17880\n  \
         --token <TOKEN>   浏览器连接令牌；生产环境必须设置\n\n\
         MCP 使用 stdio，日志只写入 stderr。"
    );
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("mox-hmi-control: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let config = Config::parse()?;
    let hub = ControlHub::new(config.token.clone());
    let bind_addr = std::net::SocketAddr::new(config.addr, config.port);
    let listener = TcpListener::bind(bind_addr)
        .await
        .map_err(|error| format!("无法监听 {bind_addr}: {error}"))?;

    eprintln!(
        "MOX HMI control tunnel: ws://{}:{}{}, MCP http://{}:{}/mcp",
        config.addr,
        config.port,
        hmi_control_protocol::DEFAULT_CONTROL_PATH,
        config.addr,
        config.port,
    );
    if config.token.is_none() {
        eprintln!(
            "warning: no --token configured; only use this development mode on a trusted machine"
        );
    }

    let http_config = http::HttpConfig {
        token: config.token.clone(),
        web_dir: None,
        haptic: None,
    };
    let http_hub = hub.clone();
    tokio::spawn(async move {
        if let Err(error) = http::serve(listener, http_hub, http_config).await {
            eprintln!("mox-hmi-control: HTTP/WebSocket 服务退出: {error}");
        }
    });
    mcp::serve(hub).await
}
