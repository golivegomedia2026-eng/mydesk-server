//! 登录 / 当前用户 / 登出（契约 §1）。

use super::{parse, ApiError, ApiResult, App, CurrentUser};
use crate::{auth, repo, store::now};
use axum::{body::Bytes, extract::State, Json};
use hbb_common::log;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// 连续失败多少次后锁定、锁定多久。
const MAX_FAILURES: u32 = 5;
const LOCK_SECS: i64 = 15 * 60;

/// RustDesk 客户端期望的 user 对象；`info` 必须存在（Rust 侧 OIDC 路径要求）。
pub fn user_payload(u: &repo::User) -> Value {
    json!({
        "name": u.username,
        "display_name": u.display_name,
        "email": u.email,
        "note": u.note,
        "status": u.status,
        "is_admin": u.role == "admin",
        "avatar": "",
        "info": {},
    })
}

pub async fn login_options() -> Json<Vec<String>> {
    Json(vec![])
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LoginReq {
    username: String,
    password: String,
    id: String,
    uuid: String,
    #[serde(rename = "type")]
    kind: String,
}

pub async fn login(State(app): State<Arc<App>>, body: Bytes) -> ApiResult<Json<Value>> {
    let req: LoginReq = parse(&body)?;
    if !req.kind.is_empty() && req.kind != "account" {
        return Err(ApiError::bad_request("不支持的登录方式"));
    }
    let key = req.username.trim().to_lowercase();
    if key.is_empty() || req.password.is_empty() {
        return Err(ApiError::bad_request("请输入用户名和密码"));
    }
    if let Some(wait) = locked_for(&app, &key) {
        return Err(ApiError::new(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            format!("密码错误次数过多，请 {} 分钟后再试", (wait + 59) / 60),
        ));
    }
    let user = repo::user_by_name(&app.db, req.username.trim()).await?;
    // 用户不存在时也跑一次哈希，避免通过耗时差异枚举用户名
    let ok = match &user {
        Some(u) => auth::verify_password(&req.password, &u.password_hash),
        None => {
            auth::verify_password(&req.password, DUMMY_HASH);
            false
        }
    };
    let Some(user) = user.filter(|_| ok) else {
        record_failure(&app, &key);
        log::warn!("登录失败: {}", key);
        return Err(ApiError::new(axum::http::StatusCode::UNAUTHORIZED, "用户名或密码错误"));
    };
    if user.status != 1 {
        return Err(ApiError::forbidden("账号已停用"));
    }
    app.login_failures.lock().unwrap().remove(&key);
    let token = repo::create_token(&app.db, user.id, &req.id, &req.uuid, repo::TokenKind::Client).await?;
    repo::admin_audit(&app.db, Some(user.id), "login", &req.id, "", "").await?;
    Ok(Json(json!({ "type": "access_token", "access_token": token, "user": user_payload(&user) })))
}

pub async fn current_user(me: CurrentUser) -> Json<Value> {
    Json(user_payload(&me.user))
}

pub async fn logout(State(app): State<Arc<App>>, me: CurrentUser) -> ApiResult<&'static str> {
    repo::delete_token(&app.db, &me.token).await?;
    Ok("")
}

/// 一个合法但永远不会匹配的 argon2 哈希，用于恒定耗时。
pub(crate) const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$J5OaD0P8C2xjxXGJ+uYzR3lJ7r1Qm6a3cQ4bhuD9hF0";

/// 仍在锁定期则返回剩余秒数。
pub(crate) fn locked_for(app: &App, key: &str) -> Option<i64> {
    let map = app.login_failures.lock().unwrap();
    let (n, first) = map.get(key)?;
    let left = first + LOCK_SECS - now();
    (*n >= MAX_FAILURES && left > 0).then_some(left)
}

pub(crate) fn record_failure(app: &App, key: &str) {
    let mut map = app.login_failures.lock().unwrap();
    let t = now();
    let e = map.entry(key.to_owned()).or_insert((0, t));
    if t - e.1 > LOCK_SECS {
        *e = (0, t);
    }
    e.0 += 1;
}
