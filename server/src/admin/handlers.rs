//! 管理接口实现。所有修改类操作都写入后台审计（audit_admin）。

use super::{Admin, AdminError, AdminResult, AdminState, Pending, COOKIE};
use crate::{
    api::account::{locked_for, record_failure, DUMMY_HASH},
    auth, repo, repo_admin as ra,
    store::now,
    totp,
};
use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::Arc};

type St = State<Arc<AdminState>>;

const PENDING_TTL: i64 = 300;
const ISSUER: &str = "MyDesk";

fn ok() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn log_op(st: &AdminState, who: &Admin, action: &str, target: &str, detail: &str) -> AdminResult<()> {
    repo::admin_audit(&st.app.db, Some(who.user.id), action, target, detail, &who.ip).await?;
    Ok(())
}

async fn require_tfa(st: &AdminState) -> AdminResult<bool> {
    Ok(ra::setting(&st.app.db, "require_admin_tfa").await? != "0")
}

fn session_cookie(st: &AdminState, token: &str, max_age: i64) -> String {
    let secure = if st.cfg.secure_cookie { " Secure;" } else { "" };
    format!("{COOKIE}={token}; Path=/{}/; HttpOnly;{secure} SameSite=Strict; Max-Age={max_age}", st.cfg.path)
}

/// 发放后台会话并写 Cookie。
async fn start_session(st: &AdminState, user_id: i64, ip: &str) -> AdminResult<Response> {
    let token = repo::create_token(&st.app.db, user_id, "", "", repo::TokenKind::Admin).await?;
    repo::admin_audit(&st.app.db, Some(user_id), "admin.login", "", "", ip).await?;
    Ok(([(header::SET_COOKIE, session_cookie(st, &token, repo::TokenKind::Admin.ttl()))], ok()).into_response())
}

// ───────────────────────────── 登录 ─────────────────────────────

#[derive(Deserialize)]
pub struct LoginReq {
    username: String,
    password: String,
}

/// 第一步：账号密码。返回 `{step: "tfa"|"enroll"|"done"}`。
pub async fn login(State(st): St, ConnectInfo(addr): ConnectInfo<SocketAddr>, Json(r): Json<LoginReq>) -> AdminResult<Response> {
    let key = format!("admin:{}", r.username.trim().to_lowercase());
    if let Some(wait) = locked_for(&st.app, &key) {
        return Err(AdminError::new(StatusCode::TOO_MANY_REQUESTS, format!("密码错误次数过多，请 {} 分钟后再试", (wait + 59) / 60)));
    }
    let user = repo::user_by_name(&st.app.db, r.username.trim()).await?;
    let ok_pw = match &user {
        Some(u) => auth::verify_password(&r.password, &u.password_hash),
        None => {
            auth::verify_password(&r.password, DUMMY_HASH);
            false
        }
    };
    let Some(user) = user.filter(|u| ok_pw && u.role == "admin" && u.status == 1) else {
        record_failure(&st.app, &key);
        repo::admin_audit(&st.app.db, None, "admin.login_failed", r.username.trim(), "", &addr.ip().to_string()).await?;
        return Err(AdminError::new(StatusCode::UNAUTHORIZED, "用户名或密码错误，或该账号不是管理员"));
    };
    st.app.login_failures.lock().unwrap().remove(&key);
    let secret = ra::totp_secret(&st.app.db, user.id).await?;
    let ip = addr.ip().to_string();
    if secret.is_empty() && !require_tfa(&st).await? {
        return start_session(&st, user.id, &ip).await;
    }
    let pending = auth::new_token();
    let enroll = secret.is_empty().then(totp::new_secret);
    let mut body = json!({ "step": if enroll.is_some() { "enroll" } else { "tfa" }, "pending": pending });
    if let Some(s) = &enroll {
        let uri = totp::otpauth_uri(ISSUER, &user.username, s);
        body["secret"] = json!(s);
        body["qr_svg"] = json!(totp::qr_svg(&uri));
    }
    let mut map = st.pending.lock().unwrap();
    let t = now();
    map.retain(|_, p| p.expires > t);
    map.insert(pending, Pending { user_id: user.id, expires: t + PENDING_TTL, enroll_secret: enroll });
    Ok(Json(body).into_response())
}

#[derive(Deserialize)]
pub struct TfaReq {
    pending: String,
    code: String,
}

/// 第二步：动态码（首次登录时同时完成绑定）。
pub async fn login_tfa(State(st): St, ConnectInfo(addr): ConnectInfo<SocketAddr>, Json(r): Json<TfaReq>) -> AdminResult<Response> {
    let p = {
        let map = st.pending.lock().unwrap();
        map.get(&r.pending).filter(|p| p.expires > now()).map(|p| (p.user_id, p.enroll_secret.clone()))
    };
    let Some((user_id, enroll)) = p else {
        return Err(AdminError::new(StatusCode::UNAUTHORIZED, "登录已过期，请重新输入账号密码"));
    };
    let key = format!("admin-tfa:{user_id}");
    if locked_for(&st.app, &key).is_some() {
        st.pending.lock().unwrap().remove(&r.pending);
        return Err(AdminError::new(StatusCode::TOO_MANY_REQUESTS, "动态码错误次数过多，请稍后重新登录"));
    }
    let secret = match &enroll {
        Some(s) => s.clone(),
        None => ra::totp_secret(&st.app.db, user_id).await?,
    };
    if !totp::verify(&secret, &r.code, now() as u64) {
        record_failure(&st.app, &key);
        return Err(AdminError::new(StatusCode::UNAUTHORIZED, "动态码不正确"));
    }
    st.pending.lock().unwrap().remove(&r.pending);
    st.app.login_failures.lock().unwrap().remove(&key);
    if let Some(s) = enroll {
        ra::set_totp_secret(&st.app.db, user_id, &s).await?;
        repo::admin_audit(&st.app.db, Some(user_id), "admin.tfa_enrolled", "", "", &addr.ip().to_string()).await?;
    }
    start_session(&st, user_id, &addr.ip().to_string()).await
}

pub async fn logout(State(st): St, me: Admin) -> AdminResult<Response> {
    repo::delete_token(&st.app.db, &me.token).await?;
    Ok(([(header::SET_COOKIE, session_cookie(&st, "", 0))], ok()).into_response())
}

pub async fn me(State(st): St, me: Admin) -> AdminResult<Json<Value>> {
    let tfa = !ra::totp_secret(&st.app.db, me.user.id).await?.is_empty();
    Ok(Json(json!({
        "username": me.user.username,
        "display_name": me.user.display_name,
        "tfa": tfa,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

// ───────────────────────────── 概览 ─────────────────────────────

pub async fn overview(State(st): St, _me: Admin) -> AdminResult<Json<Value>> {
    let t = now();
    // 按北京时间（UTC+8）计“今天”的起点
    let day_start = (t + 8 * 3600).div_euclid(86400) * 86400 - 8 * 3600;
    let o = ra::overview(&st.app.db, day_start).await?;
    let (_, recent) = ra::audit(&st.app.db, ra::AuditKind::Conn, 1, 8).await?;
    Ok(Json(json!({ "stats": o, "recent": recent })))
}

// ───────────────────────────── 设备 ─────────────────────────────

pub async fn devices(State(st): St, _me: Admin) -> AdminResult<Json<Value>> {
    Ok(Json(json!(ra::devices(&st.app.db).await?)))
}

#[derive(Deserialize)]
pub struct DeviceEdit {
    #[serde(default)]
    alias: String,
    #[serde(default)]
    owner_name: String,
    #[serde(default)]
    note: String,
    group_id: Option<i64>,
}

fn clip(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

pub async fn update_device(State(st): St, me: Admin, Path(id): Path<String>, Json(r): Json<DeviceEdit>) -> AdminResult<Json<Value>> {
    if !ra::update_device(&st.app.db, &id, &clip(&r.alias, 64), &clip(&r.owner_name, 64), &clip(&r.note, 200), r.group_id).await? {
        return Err(AdminError::not_found());
    }
    log_op(&st, &me, "device.update", &id, &format!("分组={:?}", r.group_id)).await?;
    Ok(ok())
}

pub async fn rotate_device(State(st): St, me: Admin, Path(id): Path<String>) -> AdminResult<Json<Value>> {
    if !ra::set_device_flag(&st.app.db, &id, ra::DeviceFlag::Rotate).await? {
        return Err(AdminError::not_found());
    }
    log_op(&st, &me, "device.rotate", &id, "").await?;
    Ok(ok())
}

pub async fn disconnect_device(State(st): St, me: Admin, Path(id): Path<String>) -> AdminResult<Json<Value>> {
    if !ra::set_device_flag(&st.app.db, &id, ra::DeviceFlag::Disconnect).await? {
        return Err(AdminError::not_found());
    }
    log_op(&st, &me, "device.disconnect", &id, "").await?;
    Ok(ok())
}

#[derive(Deserialize)]
pub struct CodeReq {
    code: String,
}

/// 查看托管密码：必须当场输入本人的两步验证码，每次查看都记审计。
pub async fn reveal_secret(State(st): St, me: Admin, Path(id): Path<String>, Json(r): Json<CodeReq>) -> AdminResult<Json<Value>> {
    let secret = ra::totp_secret(&st.app.db, me.user.id).await?;
    if secret.is_empty() {
        return Err(AdminError::bad("请先为本账号开启两步验证"));
    }
    if !totp::verify(&secret, &r.code, now() as u64) {
        log_op(&st, &me, "credential.reveal_denied", &id, "动态码错误").await?;
        return Err(AdminError::new(StatusCode::UNAUTHORIZED, "动态码不正确"));
    }
    let cipher = ra::device_secret(&st.app.db, &id).await?.ok_or_else(|| AdminError::bad("该设备未托管密码"))?;
    let password = st.app.vault.open(&cipher)?;
    log_op(&st, &me, "credential.reveal", &id, "").await?;
    Ok(Json(json!({ "password": password })))
}

pub async fn delete_device(State(st): St, me: Admin, Path(id): Path<String>) -> AdminResult<Json<Value>> {
    if !ra::delete_device(&st.app.db, &id).await? {
        return Err(AdminError::not_found());
    }
    log_op(&st, &me, "device.delete", &id, "").await?;
    Ok(ok())
}

// ───────────────────────────── 分组 ─────────────────────────────

pub async fn groups(State(st): St, _me: Admin) -> AdminResult<Json<Value>> {
    Ok(Json(json!(ra::groups(&st.app.db).await?)))
}

#[derive(Deserialize)]
pub struct GroupReq {
    name: String,
    #[serde(default)]
    note: String,
}

fn group_name(n: &str) -> AdminResult<String> {
    let n = clip(n, 32);
    if n.is_empty() {
        return Err(AdminError::bad("分组名称不能为空"));
    }
    Ok(n)
}

pub async fn create_group(State(st): St, me: Admin, Json(r): Json<GroupReq>) -> AdminResult<Json<Value>> {
    let name = group_name(&r.name)?;
    let id = ra::create_group(&st.app.db, &name, &clip(&r.note, 200)).await?;
    log_op(&st, &me, "group.create", &name, "").await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn update_group(State(st): St, me: Admin, Path(id): Path<i64>, Json(r): Json<GroupReq>) -> AdminResult<Json<Value>> {
    let name = group_name(&r.name)?;
    if !ra::update_group(&st.app.db, id, &name, &clip(&r.note, 200)).await? {
        return Err(AdminError::not_found());
    }
    log_op(&st, &me, "group.update", &name, "").await?;
    Ok(ok())
}

pub async fn delete_group(State(st): St, me: Admin, Path(id): Path<i64>) -> AdminResult<Json<Value>> {
    if !ra::delete_group(&st.app.db, id).await? {
        return Err(AdminError::not_found());
    }
    log_op(&st, &me, "group.delete", &id.to_string(), "组内设备转为未分组").await?;
    Ok(ok())
}

// ───────────────────────────── 用户 ─────────────────────────────

pub async fn users(State(st): St, _me: Admin) -> AdminResult<Json<Value>> {
    Ok(Json(json!(ra::users(&st.app.db).await?)))
}

#[derive(Deserialize)]
pub struct NewUser {
    username: String,
    #[serde(default)]
    display_name: String,
    #[serde(default = "operator")]
    role: String,
    #[serde(default)]
    group_ids: Vec<i64>,
}
fn operator() -> String {
    "operator".into()
}

fn valid_role(r: &str) -> AdminResult<()> {
    if matches!(r, "admin" | "operator") {
        Ok(())
    } else {
        Err(AdminError::bad("角色只能是管理员或运维"))
    }
}

/// 新建账号，返回一次性显示的初始密码。
pub async fn create_user(State(st): St, me: Admin, Json(r): Json<NewUser>) -> AdminResult<Json<Value>> {
    let username = r.username.trim();
    if username.is_empty() || username.len() > 32 || !username.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        return Err(AdminError::bad("用户名只能包含字母、数字和 . _ -，最长 32 位"));
    }
    valid_role(&r.role)?;
    let password = auth::random_password(16);
    let id = repo::create_user(&st.app.db, username, &auth::hash_password(&password)?, &r.role, &clip(&r.display_name, 32)).await?;
    ra::update_user(&st.app.db, id, &clip(&r.display_name, 32), &r.role, 1, &r.group_ids).await?;
    log_op(&st, &me, "user.create", username, &format!("角色={}", r.role)).await?;
    Ok(Json(json!({ "id": id, "password": password })))
}

#[derive(Deserialize)]
pub struct EditUser {
    #[serde(default)]
    display_name: String,
    role: String,
    status: i64,
    #[serde(default)]
    group_ids: Vec<i64>,
}

pub async fn update_user(State(st): St, me: Admin, Path(id): Path<i64>, Json(r): Json<EditUser>) -> AdminResult<Json<Value>> {
    valid_role(&r.role)?;
    let target = repo::user_by_id(&st.app.db, id).await?.ok_or_else(AdminError::not_found)?;
    let demoting = target.role == "admin" && target.status == 1 && (r.role != "admin" || r.status != 1);
    if demoting && ra::active_admins(&st.app.db).await? <= 1 {
        return Err(AdminError::bad("至少需要保留一个启用的管理员"));
    }
    ra::update_user(&st.app.db, id, &clip(&r.display_name, 32), &r.role, (r.status == 1) as i64, &r.group_ids).await?;
    if r.status != 1 {
        repo::revoke_user_tokens(&st.app.db, id).await?;
    }
    log_op(&st, &me, "user.update", &target.username, &format!("角色={} 状态={} 分组={:?}", r.role, r.status, r.group_ids)).await?;
    Ok(ok())
}

pub async fn reset_password(State(st): St, me: Admin, Path(id): Path<i64>) -> AdminResult<Json<Value>> {
    let target = repo::user_by_id(&st.app.db, id).await?.ok_or_else(AdminError::not_found)?;
    let password = auth::random_password(16);
    ra::set_user_password_by_id(&st.app.db, id, &auth::hash_password(&password)?).await?;
    repo::revoke_user_tokens(&st.app.db, id).await?;
    log_op(&st, &me, "user.reset_password", &target.username, "").await?;
    Ok(Json(json!({ "password": password })))
}

pub async fn reset_tfa(State(st): St, me: Admin, Path(id): Path<i64>) -> AdminResult<Json<Value>> {
    let target = repo::user_by_id(&st.app.db, id).await?.ok_or_else(AdminError::not_found)?;
    ra::set_totp_secret(&st.app.db, id, "").await?;
    log_op(&st, &me, "user.reset_tfa", &target.username, "").await?;
    Ok(ok())
}

// ───────────────────────────── 求助 ─────────────────────────────

pub async fn help(State(st): St, me: Admin) -> AdminResult<Json<Value>> {
    let list = repo::open_help(&st.app.db, me.user.id, true).await?;
    Ok(Json(json!(list
        .into_iter()
        .map(|h| json!({ "id": h.id, "device_id": h.device_id, "hostname": h.hostname, "message": h.message, "created_at": h.created_at }))
        .collect::<Vec<_>>())))
}

pub async fn help_done(State(st): St, me: Admin, Path(id): Path<i64>) -> AdminResult<Json<Value>> {
    if !repo::close_help(&st.app.db, id, me.user.id, true).await? {
        return Err(AdminError::not_found());
    }
    Ok(ok())
}

// ───────────────────────────── 审计 ─────────────────────────────

#[derive(Deserialize)]
pub struct AuditQuery {
    kind: String,
    #[serde(default = "one")]
    page: i64,
    #[serde(default = "fifty")]
    size: i64,
}
fn one() -> i64 {
    1
}
fn fifty() -> i64 {
    50
}

pub async fn audit_list(st: &AdminState, q: &AuditQuery) -> AdminResult<(i64, Vec<ra::AuditRow>)> {
    let kind = ra::AuditKind::parse(&q.kind).ok_or_else(|| AdminError::bad("未知审计类别"))?;
    Ok(ra::audit(&st.app.db, kind, q.page, q.size).await?)
}

pub async fn audit(State(st): St, _me: Admin, Query(q): Query<AuditQuery>) -> AdminResult<Json<Value>> {
    let (total, rows) = audit_list(&st, &q).await?;
    Ok(Json(json!({ "total": total, "rows": rows })))
}

fn csv_field(s: &str) -> String {
    // 防公式注入：以 = + - @ 开头的单元格前加单引号
    let s = if s.starts_with(['=', '+', '-', '@']) { format!("'{s}") } else { s.to_owned() };
    format!("\"{}\"", s.replace('"', "\"\""))
}

pub async fn audit_csv(State(st): St, me: Admin, Query(mut q): Query<AuditQuery>) -> AdminResult<Response> {
    q.page = 1;
    q.size = 1000;
    let (_, rows) = audit_list(&st, &q).await?;
    let mut out = String::from("\u{feff}时间(Unix),主体,对象,类别,详情\n");
    for r in rows {
        out.push_str(&format!("{},{},{},{},{}\n", r.at, csv_field(&r.actor), csv_field(&r.target), csv_field(&r.label), csv_field(&r.detail)));
    }
    repo::admin_audit(&st.app.db, Some(me.user.id), "audit.export", &q.kind, "", &me.ip).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"mydesk-audit-{}.csv\"", q.kind)),
        ],
        out,
    )
        .into_response())
}

// ───────────────────────────── 设置 ─────────────────────────────

pub async fn settings(State(st): St, _me: Admin) -> AdminResult<Json<Value>> {
    let db = &st.app.db;
    let pk = std::fs::read_to_string("id_ed25519.pub").unwrap_or_default();
    Ok(Json(json!({
        "require_admin_tfa": ra::setting(db, "require_admin_tfa").await? != "0",
        "log_credential_release": ra::setting(db, "log_credential_release").await? != "0",
        "min_client_version": ra::setting(db, "min_client_version").await?,
        "domain": st.cfg.domain,
        "public_key": pk.trim(),
        "server_version": env!("CARGO_PKG_VERSION"),
        "admin_path": st.cfg.path,
        "admin_allow": st.cfg.allow.iter().map(|n| n.to_string()).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct SettingsReq {
    require_admin_tfa: bool,
    log_credential_release: bool,
    #[serde(default)]
    min_client_version: String,
}

pub async fn save_settings(State(st): St, me: Admin, Json(r): Json<SettingsReq>) -> AdminResult<Json<Value>> {
    let v = r.min_client_version.trim();
    if !v.is_empty() && !v.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        return Err(AdminError::bad("版本号格式应为 1.5.3"));
    }
    let db = &st.app.db;
    ra::set_setting(db, "require_admin_tfa", if r.require_admin_tfa { "1" } else { "0" }).await?;
    ra::set_setting(db, "log_credential_release", if r.log_credential_release { "1" } else { "0" }).await?;
    ra::set_setting(db, "min_client_version", v).await?;
    log_op(&st, &me, "settings.update", "", &format!("强制两步验证={} 记录下发={} 最低版本={}", r.require_admin_tfa, r.log_credential_release, v)).await?;
    Ok(ok())
}
