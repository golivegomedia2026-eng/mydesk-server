//! 审计上报（契约 §7）。成功必须 200 + 空 body，否则客户端会重试。

use super::{parse, ApiResult, App};
use crate::repo;
use axum::{body::Bytes, extract::State};
use hbb_common::log;
use serde_json::{Map, Value};
use std::sync::Arc;

fn s<'a>(v: &'a Map<String, Value>, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn i(v: &Map<String, Value>, k: &str) -> i64 {
    v.get(k).and_then(Value::as_i64).unwrap_or(0)
}

/// 被控端审计须来自已登记且 uuid 匹配的设备；不匹配的静默丢弃（仍返回 200，避免客户端重试风暴）。
async fn from_known_device(app: &App, v: &Map<String, Value>) -> ApiResult<bool> {
    let ok = repo::device_matches(&app.db, s(v, "id"), s(v, "uuid")).await?;
    if !ok {
        log::warn!("丢弃未登记设备 {} 的审计上报", s(v, "id"));
    }
    Ok(ok)
}

pub async fn conn(State(app): State<Arc<App>>, body: Bytes) -> ApiResult<&'static str> {
    let v: Map<String, Value> = parse(&body)?;
    let raw = Value::Object(v.clone()).to_string();
    let session_id = v.get("session_id").map(|x| x.to_string()).unwrap_or_default();
    // 控制端写的会话备注：{id, session_id, note}，无 uuid
    if v.contains_key("note") && !v.contains_key("uuid") {
        repo::insert_conn_audit(&app.db, None, s(&v, "id"), 0, &session_id, "note", "", "", "", 0, &raw).await?;
        return Ok("");
    }
    if !from_known_device(&app, &v).await? {
        return Ok("");
    }
    let nonce = Some(s(&v, "nonce")).filter(|n| !n.is_empty());
    // 认证成功事件无 action，带 peer: [id, name]
    let (action, peer_id, peer_name) = match v.get("peer").and_then(Value::as_array) {
        Some(p) => (
            "auth",
            p.first().and_then(Value::as_str).unwrap_or(""),
            p.get(1).and_then(Value::as_str).unwrap_or(""),
        ),
        None => (s(&v, "action"), "", ""),
    };
    repo::insert_conn_audit(
        &app.db,
        nonce,
        s(&v, "id"),
        i(&v, "conn_id"),
        &session_id,
        action,
        peer_id,
        peer_name,
        s(&v, "ip"),
        i(&v, "type"),
        &raw,
    )
    .await?;
    Ok("")
}

pub async fn file(State(app): State<Arc<App>>, body: Bytes) -> ApiResult<&'static str> {
    let v: Map<String, Value> = parse(&body)?;
    if !from_known_device(&app, &v).await? {
        return Ok("");
    }
    let raw = Value::Object(v.clone()).to_string();
    let is_file = v.get("is_file").and_then(Value::as_bool).unwrap_or(true);
    let nonce = Some(s(&v, "nonce")).filter(|n| !n.is_empty());
    repo::insert_file_audit(&app.db, nonce, s(&v, "id"), s(&v, "peer_id"), s(&v, "path"), is_file, i(&v, "type"), &raw).await?;
    Ok("")
}

pub async fn alarm(State(app): State<Arc<App>>, body: Bytes) -> ApiResult<&'static str> {
    let v: Map<String, Value> = parse(&body)?;
    if !from_known_device(&app, &v).await? {
        return Ok("");
    }
    let raw = Value::Object(v.clone()).to_string();
    let nonce = Some(s(&v, "nonce")).filter(|n| !n.is_empty());
    repo::insert_alarm_audit(&app.db, nonce, s(&v, "id"), i(&v, "typ"), &raw).await?;
    Ok("")
}
