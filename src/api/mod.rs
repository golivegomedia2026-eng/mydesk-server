//! 账号 API（兼容 RustDesk 客户端，契约见 docs/v1/CLIENT-API.md）。
//!
//! 分层：本目录各文件是 HTTP 适配层（解析请求、组装 RustDesk 期望的响应形状），
//! 数据读写集中在 `crate::repo`。

mod ab;
mod account;
mod audit;
mod device;
mod group;
mod help;

use crate::{repo, store::Db, vault::Vault};
use axum::{
    body::Bytes,
    extract::{FromRequestParts, State},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use hbb_common::{log, tokio, ResultType};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::{collections::HashMap, net::SocketAddr, sync::Arc, sync::Mutex};

/// 共享状态。
pub struct App {
    pub db: Db,
    pub vault: Vault,
    /// 登录失败计数：key = 用户名小写，value = (次数, 首次失败时间)
    pub login_failures: Mutex<HashMap<String, (u32, i64)>>,
}

pub type AppState = State<Arc<App>>;

/// API 错误：RustDesk 客户端统一识别 `{"error": "..."}`。
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    msg: String,
}

impl ApiError {
    pub fn new(status: StatusCode, msg: impl Into<String>) -> Self {
        Self { status, msg: msg.into() }
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, msg)
    }
    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "登录已失效，请重新登录")
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, msg)
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, msg)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.msg }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        log::error!("数据库错误: {e:?}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "服务器内部错误")
    }
}

impl From<hbb_common::anyhow::Error> for ApiError {
    fn from(e: hbb_common::anyhow::Error) -> Self {
        log::error!("内部错误: {e:?}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "服务器内部错误")
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// 宽松解析 JSON body：客户端部分请求不带 Content-Type，空 body 视为 `{}`。
pub fn parse<T: DeserializeOwned>(body: &Bytes) -> ApiResult<T> {
    let raw: &[u8] = if body.iter().all(u8::is_ascii_whitespace) { b"{}" } else { body };
    serde_json::from_slice(raw).map_err(|e| ApiError::bad_request(format!("请求格式错误: {e}")))
}

/// 写接口成功：HTTP 200 + 空 body（客户端把 `{}`/`null` 当错误）。
pub fn empty_ok() -> Response {
    (StatusCode::OK, "").into_response()
}

/// 已登录用户（由 Bearer 令牌解析）。
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user: repo::User,
    /// 本次请求所用的明文令牌（登出时删除）
    pub token: String,
}

impl CurrentUser {
    pub fn is_admin(&self) -> bool {
        self.user.role == "admin"
    }
}

impl FromRequestParts<Arc<App>> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &Arc<App>) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(ApiError::unauthorized)?;
        let user = repo::user_by_token(&app.db, token).await?.ok_or_else(ApiError::unauthorized)?;
        Ok(CurrentUser { user, token: token.to_owned() })
    }
}

/// 组装全部路由。
pub fn router(app: Arc<App>) -> Router {
    Router::new()
        // 账户
        .route("/api/login-options", get(account::login_options))
        .route("/api/login", post(account::login))
        .route("/api/currentUser", post(account::current_user))
        .route("/api/logout", post(account::logout))
        // 地址簿
        .route("/api/ab/personal", post(ab::personal))
        .route("/api/ab/settings", post(ab::settings))
        .route("/api/ab/shared/profiles", post(ab::shared_profiles))
        .route("/api/ab/peers", post(ab::peers))
        .route("/api/ab/tags/{guid}", post(ab::tags))
        .route("/api/ab/peer/add/{guid}", post(ab::peer_add))
        .route("/api/ab/peer/update/{guid}", put(ab::peer_update))
        .route("/api/ab/peer/{guid}", delete(ab::peer_delete))
        .route("/api/ab/tag/add/{guid}", post(ab::tag_add))
        .route("/api/ab/tag/rename/{guid}", put(ab::tag_rename))
        .route("/api/ab/tag/update/{guid}", put(ab::tag_update))
        .route("/api/ab/tag/{guid}", delete(ab::tag_delete))
        // 分组面板
        .route("/api/device-group/accessible", get(group::accessible_groups))
        .route("/api/users", get(group::users))
        .route("/api/peers", get(group::peers))
        // 被控端上报
        .route("/api/sysinfo", post(device::sysinfo))
        .route("/api/sysinfo_ver", post(device::sysinfo_ver))
        .route("/api/heartbeat", post(device::heartbeat))
        // 审计
        .route("/api/audit/conn", post(audit::conn))
        .route("/api/audit/file", post(audit::file))
        .route("/api/audit/alarm", post(audit::alarm))
        // MyDesk 扩展：求助工单
        .route("/api/mydesk/help", post(help::create).get(help::list))
        .route("/api/mydesk/help/{id}/done", post(help::done))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(app)
}

/// API 的 TLS 模式。
#[derive(Debug, Clone)]
pub enum Tls {
    /// 仅 HTTP（内网测试或前置反向代理时用）
    Off,
    /// 自动向 Let's Encrypt 申请/续期证书（TLS-ALPN-01，需 443 可从公网访问）
    Acme { domain: String, email: String },
    /// 自备证书（PEM）
    Files { cert: String, key: String },
}

/// API 监听配置。
#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub http_addr: SocketAddr,
    pub https_addr: SocketAddr,
    pub tls: Tls,
    pub db_path: String,
}

/// 在当前线程新建 tokio 运行时并运行 API 服务（数据库连接池须与使用它的运行时同属一个）。
/// 开启 TLS 时，明文 HTTP 只监听本机回环地址，避免托管密码明文过网。
pub fn serve_blocking(cfg: ServeConfig) -> ResultType<()> {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build()?;
    rt.block_on(async move {
        let db = crate::store::open(&cfg.db_path).await?;
        crate::bootstrap::ensure_admin(&db).await?;
        let app = Arc::new(App { db, vault: Vault::load_or_create()?, login_failures: Mutex::default() });
        let make = || router(app.clone()).into_make_service_with_connect_info::<SocketAddr>();
        let (svc_http, svc_https) = (make(), make());

        let mut http_addr = cfg.http_addr;
        if !matches!(cfg.tls, Tls::Off) {
            http_addr.set_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        }
        let listener = tokio::net::TcpListener::bind(http_addr).await?;
        log::info!("账号 API 监听 http://{}", http_addr);
        let http = async move { axum::serve(listener, svc_http).await.map_err(hbb_common::anyhow::Error::from) };

        let https = async move {
            match &cfg.tls {
                Tls::Off => std::future::pending::<ResultType<()>>().await,
                Tls::Files { cert, key } => {
                    let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;
                    log::info!("账号 API 监听 https://{}（自备证书）", cfg.https_addr);
                    axum_server::bind_rustls(cfg.https_addr, tls).serve(svc_https).await?;
                    Ok(())
                }
                Tls::Acme { domain, email } => {
                    use hbb_common::futures::StreamExt;
                    let mut state = rustls_acme::AcmeConfig::new([domain.as_str()])
                        .contact(Some(email).filter(|e| !e.is_empty()).map(|e| format!("mailto:{e}")))
                        .cache(rustls_acme::caches::DirCache::new("acme"))
                        .directory_lets_encrypt(true)
                        .state();
                    let acceptor = state.axum_acceptor(state.default_rustls_config());
                    tokio::spawn(async move {
                        while let Some(ev) = state.next().await {
                            match ev {
                                Ok(ok) => log::info!("证书: {:?}", ok),
                                Err(err) => log::error!("证书申请/续期失败: {:?}", err),
                            }
                        }
                    });
                    log::info!("账号 API 监听 https://{}:{}（Let's Encrypt 自动证书）", domain, cfg.https_addr.port());
                    axum_server::bind(cfg.https_addr).acceptor(acceptor).serve(svc_https).await?;
                    Ok(())
                }
            }
        };
        tokio::try_join!(http, https)?;
        Ok(())
    })
}
