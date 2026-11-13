//! MyDesk 单文件服务端。
//!
//! 一个进程内同时运行：
//! - 信令/ID 服务（官方 hbbs）：TCP 21115/21116/21118，UDP 21116
//! - 中继服务（官方 hbbr）：TCP 21117/21119
//! - 账号 API（兼容 RustDesk 客户端）：TCP 21114，数据在 mydesk.db
//!
//! 两个上游启动函数各自带 `#[tokio::main]`，因此各占一个 OS 线程、各自一个运行时；
//! 任一服务退出，整个进程以非零码退出，交给 systemd / Windows 服务管理器拉起。

// 让 relay_server.rs 中的 `crate::common::*` 解析到上游公共模块
use hbbs::common;

#[path = "../third_party/rustdesk-server/src/relay_server.rs"]
#[allow(dead_code, clippy::all)]
mod relay_server;

mod api;
mod auth;
mod bootstrap;
mod repo;
mod store;
mod vault;

use flexi_logger::{opt_format, Logger, WriteMode};
use hbb_common::{bail, log, ResultType};
use std::path::PathBuf;

const USAGE: &str = "\
MyDesk 单文件服务端

用法: mydesk-server [选项]

选项:
  --data-dir <DIR>    数据目录（密钥、数据库），默认 ./data
  --host <HOST>       对外域名或 IP，用于告知客户端中继地址，例如 rd.example.com
  --port <PORT>       信令基准端口，默认 21116（占用 21115/21116/21118）
  --relay-port <PORT> 中继端口，默认 21117（占用 21117/21119）
  --api-port <PORT>   账号 API 端口，默认 21114（客户端默认按 信令端口-2 推导）
  --reset-password <用户名>
                      重置该用户密码并打印新密码后退出
  --bind <IP>         监听地址，默认所有网卡
  --open-key          不强制校验客户端公钥（仅调试用，默认强制校验）
  -h, --help          显示帮助
  -V, --version       显示版本

每个选项也可用环境变量设置：MYDESK_DATA_DIR、MYDESK_HOST、MYDESK_PORT、
MYDESK_RELAY_PORT、MYDESK_API_PORT、MYDESK_BIND、MYDESK_OPEN_KEY=1。命令行优先。";

/// 账号数据库文件名（位于数据目录）。
const DB_FILE: &str = "mydesk.db";

/// 启动配置。
#[derive(Debug)]
struct Options {
    data_dir: PathBuf,
    host: String,
    port: i32,
    relay_port: i32,
    api_port: u16,
    bind: String,
    open_key: bool,
    reset_password: Option<String>,
}

impl Options {
    /// 从命令行与环境变量解析配置。
    fn parse() -> ResultType<Self> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let mut opts = Options {
            data_dir: env("MYDESK_DATA_DIR").unwrap_or_else(|| "data".into()).into(),
            host: env("MYDESK_HOST").unwrap_or_default(),
            port: env("MYDESK_PORT").map(|v| v.parse()).transpose()?.unwrap_or(21116),
            relay_port: env("MYDESK_RELAY_PORT").map(|v| v.parse()).transpose()?.unwrap_or(21117),
            api_port: env("MYDESK_API_PORT").map(|v| v.parse()).transpose()?.unwrap_or(21114),
            bind: env("MYDESK_BIND").unwrap_or_default(),
            open_key: env("MYDESK_OPEN_KEY").as_deref() == Some("1"),
            reset_password: None,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) => (n.to_owned(), Some(v.to_owned())),
                None => (arg.clone(), None),
            };
            let mut value = |n: &str| -> ResultType<String> {
                match inline.clone().or_else(|| args.next()) {
                    Some(v) => Ok(v),
                    None => bail!("选项 {} 缺少取值", n),
                }
            };
            match name.as_str() {
                "--data-dir" => opts.data_dir = value(&name)?.into(),
                "--host" => opts.host = value(&name)?,
                "--port" => opts.port = value(&name)?.parse()?,
                "--relay-port" => opts.relay_port = value(&name)?.parse()?,
                "--api-port" => opts.api_port = value(&name)?.parse()?,
                "--reset-password" => opts.reset_password = Some(value(&name)?),
                "--bind" => opts.bind = value(&name)?,
                "--open-key" => opts.open_key = true,
                "-h" | "--help" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                "-V" | "--version" => {
                    println!("mydesk-server {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                other => bail!("未知选项 {}，用 --help 查看用法", other),
            }
        }
        if opts.port < 3 || opts.relay_port < 3 {
            bail!("端口不合法");
        }
        Ok(opts)
    }
}

fn main() -> ResultType<()> {
    let opts = Options::parse()?;
    let _logger = Logger::try_with_env_or_str("info")?
        .log_to_stdout()
        .format(opt_format)
        .write_mode(WriteMode::Async)
        .start()?;

    // 上游代码以当前目录存放 id_ed25519 与 db_v2.sqlite3，切到数据目录
    std::fs::create_dir_all(&opts.data_dir)?;
    std::env::set_current_dir(&opts.data_dir)?;
    log::info!("数据目录: {}", std::env::current_dir()?.display());

    if let Some(user) = &opts.reset_password {
        let rt = hbb_common::tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let pw = rt.block_on(async {
            let db = store::open(DB_FILE).await?;
            bootstrap::reset_password(&db, user).await
        })?;
        println!("用户 {user} 的新密码: {pw}");
        return Ok(());
    }

    // 先生成/加载密钥，避免两个服务线程并发生成
    let (pk, _) = common::gen_sk(0)?;
    log::info!("服务器公钥（客户端打包时内置）: {}", pk);

    if !opts.host.is_empty() {
        common::set_arg("relay-servers", &format!("{}:{}", opts.host, opts.relay_port));
    }
    // "_" = 使用本地密钥并强制客户端公钥一致；"-" = 使用本地密钥但不强制
    let key = if opts.open_key { "-" } else { "_" }.to_owned();
    let bind_addr = common::parse_bind_address(&opts.bind)?;

    let (tx, rx) = std::sync::mpsc::channel::<(&'static str, ResultType<()>)>();

    let (k, t, port) = (key.clone(), tx.clone(), opts.port);
    std::thread::Builder::new().name("rendezvous".into()).spawn(move || {
        let r = hbbs::RendezvousServer::start_with_bind(bind_addr, port, 0, &k, 0);
        let _ = t.send(("信令服务", r));
    })?;

    let api_addr = std::net::SocketAddr::new(bind_addr.unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)), opts.api_port);
    let t = tx.clone();
    std::thread::Builder::new().name("api".into()).spawn(move || {
        let r = api::serve_blocking(api_addr, DB_FILE.to_owned());
        let _ = t.send(("账号 API", r));
    })?;

    let (k, t, relay_port) = (key, tx, opts.relay_port.to_string());
    std::thread::Builder::new().name("relay".into()).spawn(move || {
        let r = relay_server::start_with_bind(bind_addr, &relay_port, &k);
        let _ = t.send(("中继服务", r));
    })?;

    // 任一服务结束即整体退出
    let (name, result) = rx.recv()?;
    let code = match result {
        Ok(()) => {
            log::info!("{} 收到退出信号，服务端停止", name);
            0
        }
        Err(e) => {
            log::error!("{} 异常退出: {:?}", name, e);
            1
        }
    };
    std::thread::sleep(std::time::Duration::from_millis(200));
    std::process::exit(code);
}
