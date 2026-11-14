//! Web 管理后台：`/<admin-path>/` 提供前端页面，`/<admin-path>/api/*` 提供管理接口。
//!
//! 安全措施：
//! - 仅 role=admin 可登录；按设置强制 TOTP 两步验证（首次登录引导绑定）。
//! - 会话令牌放 HttpOnly + Secure + SameSite=Strict Cookie，与客户端令牌互不通用。
//! - 修改类请求须带 `X-MyDesk: 1` 请求头（跨站页面无法在不触发预检的情况下伪造）。
//! - 可选来源 IP 白名单；登录失败与客户端共用锁定计数。

mod handlers;

use crate::{api::App, repo};
use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderMap, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use hbb_common::log;
use rust_embed::RustEmbed;
use serde_json::json;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
};

/// 前端构建产物（web/dist），编译进二进制。
#[derive(RustEmbed)]
#[folder = "web/dist"]
struct Assets;

pub const COOKIE: &str = "mdadmin";

/// 后台配置。
#[derive(Debug, Clone)]
pub struct AdminConfig {
    /// 访问路径（不含斜杠），默认 mydeskadmin
    pub path: String,
    /// 允许访问后台的来源网段；空 = 不限制
    pub allow: Vec<ipnetwork::IpNetwork>,
    /// 对外域名（仅用于显示）
    pub domain: String,
    /// 会话 Cookie 是否带 Secure（开启 HTTPS 时为真；纯 HTTP 内网测试时为假，否则浏览器不回传）
    pub secure_cookie: bool,
}

/// 登录第二步之前的待验证状态。
pub struct Pending {
    pub user_id: i64,
    pub expires: i64,
    /// 首次绑定时生成、尚未确认的 TOTP 密钥
    pub enroll_secret: Option<String>,
}

pub struct AdminState {
    pub app: Arc<App>,
    pub cfg: AdminConfig,
    pub pending: Mutex<HashMap<String, Pending>>,
}

/// 管理接口错误。
pub struct AdminError(StatusCode, String);

impl AdminError {
    pub fn new(s: StatusCode, m: impl Into<String>) -> Self {
        Self(s, m.into())
    }
    pub fn bad(m: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, m)
    }
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "对象不存在")
    }
}

impl IntoResponse for AdminError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<sqlx::Error> for AdminError {
    fn from(e: sqlx::Error) -> Self {
        log::error!("后台数据库错误: {e:?}");
        if let sqlx::Error::Database(d) = &e {
            if d.message().contains("UNIQUE") {
                return Self::bad("名称已存在");
            }
        }
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "服务器内部错误")
    }
}

impl From<hbb_common::anyhow::Error> for AdminError {
    fn from(e: hbb_common::anyhow::Error) -> Self {
        log::error!("后台内部错误: {e:?}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "服务器内部错误")
    }
}

pub type AdminResult<T> = Result<T, AdminError>;

/// 已登录的管理员。
pub struct Admin {
    pub user: repo::User,
    pub token: String,
    pub ip: String,
}

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_owned())
}

impl axum::extract::FromRequestParts<Arc<AdminState>> for Admin {
    type Rejection = AdminError;

    async fn from_request_parts(parts: &mut axum::http::request::Parts, st: &Arc<AdminState>) -> Result<Self, Self::Rejection> {
        let unauth = || AdminError::new(StatusCode::UNAUTHORIZED, "请先登录");
        let token = cookie_value(&parts.headers, COOKIE).ok_or_else(unauth)?;
        let user = repo::user_by_token(&st.app.db, &token, repo::TokenKind::Admin).await?.ok_or_else(unauth)?;
        if user.role != "admin" {
            return Err(AdminError::new(StatusCode::FORBIDDEN, "仅管理员可访问"));
        }
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip().to_string())
            .unwrap_or_default();
        Ok(Admin { user, token, ip })
    }
}

/// 来源 IP 白名单 + 修改类请求的防伪请求头。
async fn guard(State(st): State<Arc<AdminState>>, ConnectInfo(addr): ConnectInfo<SocketAddr>, req: Request, next: Next) -> Response {
    if !ip_allowed(&st.cfg.allow, addr.ip()) {
        log::warn!("拒绝来自 {} 的后台访问（不在白名单）", addr.ip());
        return StatusCode::NOT_FOUND.into_response();
    }
    let is_api = req.uri().path().contains("/api/");
    let mutating = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if is_api && mutating && req.headers().get("x-mydesk").and_then(|v| v.to_str().ok()) != Some("1") {
        return AdminError::new(StatusCode::FORBIDDEN, "缺少请求头").into_response();
    }
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    h.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    h.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'".parse().unwrap(),
    );
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    resp
}

pub fn ip_allowed(allow: &[ipnetwork::IpNetwork], ip: IpAddr) -> bool {
    if allow.is_empty() {
        return true;
    }
    // IPv4 映射的 IPv6 地址按 IPv4 判断
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    };
    ip.is_loopback() || allow.iter().any(|n| n.contains(ip))
}

/// 前端静态文件；未知路径回落到 index.html（单页应用）。
async fn asset(req: Request) -> Response {
    let path = req.uri().path();
    let rel = path.splitn(3, '/').nth(2).unwrap_or("");
    let file = if rel.is_empty() || !rel.contains('.') { "index.html" } else { rel };
    match Assets::get(file) {
        Some(f) => {
            let mime = mime_guess::from_path(file).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(f.data.into_owned()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// 后台路由，挂在 `/<path>` 下。
pub fn router(app: Arc<App>, cfg: AdminConfig) -> Router {
    let base = format!("/{}", cfg.path);
    let st = Arc::new(AdminState { app, cfg, pending: Mutex::default() });
    let api = Router::new()
        .route("/login", post(handlers::login))
        .route("/login/tfa", post(handlers::login_tfa))
        .route("/logout", post(handlers::logout))
        .route("/me", get(handlers::me))
        .route("/overview", get(handlers::overview))
        .route("/devices", get(handlers::devices))
        .route("/devices/{id}", patch(handlers::update_device).delete(handlers::delete_device))
        .route("/devices/{id}/rotate", post(handlers::rotate_device))
        .route("/devices/{id}/disconnect", post(handlers::disconnect_device))
        .route("/devices/{id}/reveal", post(handlers::reveal_secret))
        .route("/groups", get(handlers::groups).post(handlers::create_group))
        .route("/groups/{id}", patch(handlers::update_group).delete(handlers::delete_group))
        .route("/users", get(handlers::users).post(handlers::create_user))
        .route("/users/{id}", patch(handlers::update_user))
        .route("/users/{id}/reset-password", post(handlers::reset_password))
        .route("/users/{id}/reset-tfa", post(handlers::reset_tfa))
        .route("/help", get(handlers::help))
        .route("/help/{id}/done", post(handlers::help_done))
        .route("/audit", get(handlers::audit))
        .route("/audit.csv", get(handlers::audit_csv))
        .route("/settings", get(handlers::settings).put(handlers::save_settings));
    Router::new()
        .nest(&format!("{base}/api"), api)
        .route(&base, {
            let to = format!("{base}/");
            get(move || {
                let to = to.clone();
                async move { axum::response::Redirect::permanent(&to) }
            })
        })
        .route(&format!("{base}/"), get(asset))
        .route(&format!("{base}/{{*file}}"), get(asset))
        .layer(middleware::from_fn_with_state(st.clone(), guard))
        .with_state(st)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist() {
        let allow: Vec<ipnetwork::IpNetwork> = vec!["10.0.0.0/8".parse().unwrap(), "203.0.113.7/32".parse().unwrap()];
        assert!(ip_allowed(&allow, "10.1.2.3".parse().unwrap()));
        assert!(ip_allowed(&allow, "203.0.113.7".parse().unwrap()));
        assert!(ip_allowed(&allow, "::ffff:10.9.9.9".parse().unwrap()));
        assert!(ip_allowed(&allow, "127.0.0.1".parse().unwrap()));
        assert!(!ip_allowed(&allow, "8.8.8.8".parse().unwrap()));
        assert!(ip_allowed(&[], "8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn cookie_parse() {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, "a=1; mdadmin=abc; b=2".parse().unwrap());
        assert_eq!(cookie_value(&h, COOKIE).as_deref(), Some("abc"));
        assert_eq!(cookie_value(&h, "zz"), None);
    }
}
