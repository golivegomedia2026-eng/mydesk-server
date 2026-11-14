//! MyDesk 扩展：员工“请求 IT 协助”工单。

use super::{empty_ok, parse, ApiError, ApiResult, App, CurrentUser};
use crate::repo;
use axum::{
    body::Bytes,
    extract::{Path, State},
    response::Response,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize, Default)]
#[serde(default)]
struct HelpReq {
    id: String,
    uuid: String,
    message: String,
}

/// 被控端发起：无鉴权，按 id+uuid 校验为已登记设备。
pub async fn create(State(app): State<Arc<App>>, body: Bytes) -> ApiResult<Response> {
    let r: HelpReq = parse(&body)?;
    if !repo::device_matches(&app.db, &r.id, &r.uuid).await? {
        return Err(ApiError::forbidden("设备未登记，请稍后再试"));
    }
    let msg: String = r.message.trim().chars().take(200).collect();
    repo::upsert_help(&app.db, &r.id, &msg).await?;
    Ok(empty_ok())
}

pub async fn list(State(app): State<Arc<App>>, me: CurrentUser) -> ApiResult<Json<Value>> {
    let items = repo::open_help(&app.db, me.user.id, me.is_admin())
        .await?
        .into_iter()
        .map(|h| json!({ "id": h.id, "device_id": h.device_id, "hostname": h.hostname, "message": h.message, "created_at": h.created_at }))
        .collect();
    Ok(Json(Value::Array(items)))
}

pub async fn done(State(app): State<Arc<App>>, me: CurrentUser, Path(id): Path<i64>) -> ApiResult<Response> {
    if !repo::close_help(&app.db, id, me.user.id, me.is_admin()).await? {
        return Err(ApiError::not_found("工单不存在或已处理"));
    }
    Ok(empty_ok())
}
