//! 客户端“分组”面板（契约 §5）：只读列表，不含任何凭证。

use super::{ab::page, ab::platform_of, account::user_payload, ApiResult, App, CurrentUser};
use crate::repo;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct Paging {
    #[serde(default = "one")]
    current: usize,
    #[serde(default = "hundred", rename = "pageSize")]
    page_size: usize,
}
fn one() -> usize {
    1
}
fn hundred() -> usize {
    100
}

pub async fn accessible_groups(State(app): State<Arc<App>>, me: CurrentUser, Query(q): Query<Paging>) -> ApiResult<Json<Value>> {
    let items = repo::accessible_groups(&app.db, me.user.id, me.is_admin())
        .await?
        .into_iter()
        .map(|g| json!({ "name": g.name }))
        .collect();
    Ok(Json(page(items, q.current, q.page_size)))
}

pub async fn users(State(app): State<Arc<App>>, me: CurrentUser, Query(q): Query<Paging>) -> ApiResult<Json<Value>> {
    let items = repo::visible_users(&app.db, &me.user).await?.iter().map(user_payload).collect();
    Ok(Json(page(items, q.current, q.page_size)))
}

pub async fn peers(State(app): State<Arc<App>>, me: CurrentUser, Query(q): Query<Paging>) -> ApiResult<Json<Value>> {
    let items = repo::accessible_devices(&app.db, me.user.id, me.is_admin())
        .await?
        .into_iter()
        .map(|d| {
            json!({
                "id": d.id,
                "info": {
                    "username": d.os_username,
                    "os": platform_of(&d.os).to_lowercase().replace("mac os", "macos"),
                    "device_name": if d.alias.is_empty() { &d.hostname } else { &d.alias },
                },
                "status": 1,
                "user": "",
                "user_name": d.owner_name,
                "device_group_name": d.group_name,
                "note": d.note,
            })
        })
        .collect();
    Ok(Json(page(items, q.current, q.page_size)))
}
