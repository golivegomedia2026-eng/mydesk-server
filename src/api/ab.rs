//! 地址簿（契约 §2）。
//!
//! - 个人簿 guid = `personal`：每个用户自己的收藏，可读写，存客户端提交的 peer JSON。
//! - 共享簿 guid = `g-<分组ID>`（`g-0` = 未分组，仅管理员可见）：由服务端按分组自动生成，只读；
//!   peer 带托管密码（`password` 明文），运维从此簿发起连接即免输密码。每次下发凭证都记审计。

use super::{empty_ok, parse, ApiError, ApiResult, App, CurrentUser};
use crate::repo::{self, Device};
use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, Query, State},
    response::Response,
    Json,
};
use hbb_common::log;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

const PERSONAL: &str = "personal";

#[derive(Deserialize)]
pub struct PageQuery {
    #[serde(default = "one")]
    current: usize,
    #[serde(default = "hundred", rename = "pageSize")]
    page_size: usize,
    #[serde(default)]
    ab: String,
}
fn one() -> usize {
    1
}
fn hundred() -> usize {
    100
}

/// 分页包装 `{total, data}`。
pub fn page(items: Vec<Value>, current: usize, page_size: usize) -> Value {
    let size = page_size.clamp(1, 1000);
    let start = current.max(1).saturating_sub(1).saturating_mul(size);
    let total = items.len();
    let data: Vec<Value> = items.into_iter().skip(start).take(size).collect();
    json!({ "total": total, "data": data })
}

/// 由 sysinfo 的 os 字符串推断客户端用于显示图标的平台名。
pub fn platform_of(os: &str) -> &'static str {
    let o = os.to_lowercase();
    if o.contains("windows") {
        "Windows"
    } else if o.contains("mac") || o.contains("darwin") {
        "Mac OS"
    } else if o.contains("android") {
        "Android"
    } else if o.contains("linux") || o.contains("ubuntu") || o.contains("debian") || o.contains("centos") {
        "Linux"
    } else {
        ""
    }
}

pub async fn personal(_me: CurrentUser) -> Json<Value> {
    Json(json!({ "guid": PERSONAL }))
}

pub async fn settings(_me: CurrentUser) -> Json<Value> {
    Json(json!({ "max_peer_one_ab": 0 }))
}

/// 可访问的共享簿 = 可访问的分组（管理员额外看到“未分组”）。
async fn shared_books(app: &App, me: &CurrentUser) -> ApiResult<Vec<(String, String, String)>> {
    let mut books: Vec<(String, String, String)> = repo::accessible_groups(&app.db, me.user.id, me.is_admin())
        .await?
        .into_iter()
        .map(|g| (format!("g-{}", g.id), format!("设备库 · {}", g.name), g.note))
        .collect();
    if me.is_admin() {
        books.push(("g-0".into(), "设备库 · 未分组".into(), "新上线、尚未分配分组的设备".into()));
    }
    Ok(books)
}

pub async fn shared_profiles(State(app): State<Arc<App>>, me: CurrentUser, Query(q): Query<PageQuery>) -> ApiResult<Json<Value>> {
    let items = shared_books(&app, &me)
        .await?
        .into_iter()
        .map(|(guid, name, note)| json!({ "guid": guid, "name": name, "owner": "MyDesk", "note": note, "rule": 1, "info": {} }))
        .collect();
    Ok(Json(page(items, q.current, q.page_size)))
}

/// 解析共享簿 guid 并校验访问权限，返回分组 ID（`None` = 未分组）。
async fn authorize_book(app: &App, me: &CurrentUser, guid: &str) -> ApiResult<Option<i64>> {
    let id: i64 = guid
        .strip_prefix("g-")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ApiError::not_found("地址簿不存在"))?;
    if !shared_books(app, me).await?.iter().any(|(g, _, _)| g == guid) {
        return Err(ApiError::forbidden("无权访问该设备库"));
    }
    Ok((id != 0).then_some(id))
}

fn shared_peer(d: &Device, password: Option<String>) -> Value {
    let mut note = d.owner_name.clone();
    if !d.note.is_empty() {
        if !note.is_empty() {
            note.push_str(" · ");
        }
        note.push_str(&d.note);
    }
    let mut v = json!({
        "id": d.id,
        "alias": if d.alias.is_empty() { &d.hostname } else { &d.alias },
        "hostname": d.hostname,
        "username": d.os_username,
        "platform": platform_of(&d.os),
        "tags": [],
        "note": note,
        "device_group_name": d.group_name,
        "same_server": true,
    });
    if let Some(p) = password {
        v["password"] = json!(p);
    }
    v
}

pub async fn peers(
    State(app): State<Arc<App>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    me: CurrentUser,
    Query(q): Query<PageQuery>,
) -> ApiResult<Json<Value>> {
    if q.ab == PERSONAL {
        let items = repo::personal_peers(&app.db, me.user.id)
            .await?
            .iter()
            .filter_map(|s| serde_json::from_str::<Value>(s).ok())
            .collect();
        return Ok(Json(page(items, q.current, q.page_size)));
    }
    let group = authorize_book(&app, &me, &q.ab).await?;
    let devices = repo::devices_in_group(&app.db, group).await?;
    let mut released = 0usize;
    let items = devices
        .iter()
        .map(|d| {
            let pw = (!d.secret_cipher.is_empty())
                .then(|| app.vault.open(&d.secret_cipher))
                .and_then(|r| r.map_err(|e| log::error!("设备 {} 凭证解密失败: {e}", d.id)).ok());
            released += pw.is_some() as usize;
            shared_peer(d, pw)
        })
        .collect::<Vec<_>>();
    if released > 0 && q.current <= 1 {
        repo::admin_audit(&app.db, Some(me.user.id), "credential.release", &q.ab, &format!("{released} 台设备"), &addr.ip().to_string())
            .await?;
    }
    Ok(Json(page(items, q.current, q.page_size)))
}

pub async fn tags(State(app): State<Arc<App>>, me: CurrentUser, Path(guid): Path<String>) -> ApiResult<Json<Value>> {
    if guid != PERSONAL {
        authorize_book(&app, &me, &guid).await?;
        return Ok(Json(json!([])));
    }
    let tags = repo::personal_tags(&app.db, me.user.id)
        .await?
        .into_iter()
        .map(|(name, color)| json!({ "name": name, "color": color }))
        .collect();
    Ok(Json(Value::Array(tags)))
}

/// 写接口只对个人簿开放。
fn personal_only(guid: &str) -> ApiResult<()> {
    if guid == PERSONAL {
        Ok(())
    } else {
        Err(ApiError::forbidden("共享设备库为只读，请联系管理员在后台调整"))
    }
}

/// 取对象里的非空 `id`。
fn peer_id(v: &Map<String, Value>) -> ApiResult<String> {
    v.get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 64)
        .map(str::to_owned)
        .ok_or_else(|| ApiError::bad_request("缺少设备 ID"))
}

pub async fn peer_add(State(app): State<Arc<App>>, me: CurrentUser, Path(guid): Path<String>, body: Bytes) -> ApiResult<Response> {
    personal_only(&guid)?;
    let mut v: Map<String, Value> = parse(&body)?;
    let id = peer_id(&v)?;
    v.remove("password"); // 个人簿不存明文密码
    repo::personal_peer_put(&app.db, me.user.id, &id, &Value::Object(v).to_string()).await?;
    Ok(empty_ok())
}

pub async fn peer_update(State(app): State<Arc<App>>, me: CurrentUser, Path(guid): Path<String>, body: Bytes) -> ApiResult<Response> {
    personal_only(&guid)?;
    let patch: Map<String, Value> = parse(&body)?;
    let id = peer_id(&patch)?;
    let mut cur: Map<String, Value> = match repo::personal_peer(&app.db, me.user.id, &id).await? {
        Some(s) => serde_json::from_str(&s).unwrap_or_default(),
        None => Map::new(),
    };
    for (k, val) in patch {
        if k != "password" {
            cur.insert(k, val);
        }
    }
    repo::personal_peer_put(&app.db, me.user.id, &id, &Value::Object(cur).to_string()).await?;
    Ok(empty_ok())
}

pub async fn peer_delete(State(app): State<Arc<App>>, me: CurrentUser, Path(guid): Path<String>, body: Bytes) -> ApiResult<Response> {
    personal_only(&guid)?;
    let ids: Vec<String> = parse_list(&body)?;
    for id in ids {
        repo::personal_peer_delete(&app.db, me.user.id, &id).await?;
    }
    Ok(empty_ok())
}

#[derive(Deserialize)]
struct TagReq {
    name: String,
    #[serde(default)]
    color: i64,
}

pub async fn tag_add(State(app): State<Arc<App>>, me: CurrentUser, Path(guid): Path<String>, body: Bytes) -> ApiResult<Response> {
    personal_only(&guid)?;
    let t: TagReq = parse(&body)?;
    if t.name.trim().is_empty() {
        return Err(ApiError::bad_request("标签名不能为空"));
    }
    repo::personal_tag_put(&app.db, me.user.id, t.name.trim(), t.color).await?;
    Ok(empty_ok())
}

pub async fn tag_update(State(app): State<Arc<App>>, me: CurrentUser, Path(guid): Path<String>, body: Bytes) -> ApiResult<Response> {
    tag_add(State(app), me, Path(guid), body).await
}

pub async fn tag_rename(State(app): State<Arc<App>>, me: CurrentUser, Path(guid): Path<String>, body: Bytes) -> ApiResult<Response> {
    personal_only(&guid)?;
    let r: HashMap<String, String> = parse(&body)?;
    let (Some(old), Some(new)) = (r.get("old"), r.get("new")) else {
        return Err(ApiError::bad_request("缺少 old/new"));
    };
    repo::personal_tag_rename(&app.db, me.user.id, old, new.trim()).await?;
    Ok(empty_ok())
}

pub async fn tag_delete(State(app): State<Arc<App>>, me: CurrentUser, Path(guid): Path<String>, body: Bytes) -> ApiResult<Response> {
    personal_only(&guid)?;
    for name in parse_list(&body)? {
        repo::personal_tag_delete(&app.db, me.user.id, &name).await?;
    }
    Ok(empty_ok())
}

/// DELETE 请求体：JSON 字符串数组。
fn parse_list(body: &Bytes) -> ApiResult<Vec<String>> {
    serde_json::from_slice(body).map_err(|_| ApiError::bad_request("请求体应为字符串数组"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paging_shape() {
        let items: Vec<Value> = (0..250).map(|i| json!(i)).collect();
        let p = page(items.clone(), 3, 100);
        assert_eq!(p["total"], 250);
        assert_eq!(p["data"].as_array().unwrap().len(), 50);
        assert_eq!(page(items, 4, 100)["data"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn platform_mapping() {
        assert_eq!(platform_of("windows / Windows 11 Pro - 10.0.22631"), "Windows");
        assert_eq!(platform_of("macos / macOS 15.1"), "Mac OS");
        assert_eq!(platform_of("android / Android 15"), "Android");
        assert_eq!(platform_of("ubuntu / Ubuntu 24.04 LTS"), "Linux");
    }
}
