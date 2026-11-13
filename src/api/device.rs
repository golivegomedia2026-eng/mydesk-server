//! 被控端上报（契约 §6）：无鉴权，设备身份为 body 中的 `id` + `uuid`，首见即绑定（TOFU）。

use super::{parse, ApiError, ApiResult, App};
use crate::repo::{self, Report, SysInfo};
use axum::{body::Bytes, extract::{ConnectInfo, State}, Json};
use hbb_common::log;
use serde_json::{json, Map, Value};
use std::{net::SocketAddr, sync::Arc};

/// MyDesk 客户端在 sysinfo 中附带的托管凭证字段（被控端永久密码）。官方客户端不会发送。
pub const SECRET_FIELD: &str = "mydesk-secret";
/// 部署时预置的分组名（官方字段）。
const PRESET_GROUP: &str = "preset-device-group-name";

fn str_field<'a>(v: &'a Map<String, Value>, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub async fn sysinfo(State(app): State<Arc<App>>, ConnectInfo(addr): ConnectInfo<SocketAddr>, body: Bytes) -> ApiResult<&'static str> {
    let mut v: Map<String, Value> = parse(&body)?;
    let id = str_field(&v, "id").to_owned();
    let uuid = str_field(&v, "uuid").to_owned();
    if !valid_id(&id) || uuid.is_empty() {
        return Err(ApiError::bad_request("缺少 id/uuid"));
    }
    // 凭证不进 sysinfo 原文存档
    let secret = v.remove(SECRET_FIELD).and_then(|s| s.as_str().map(str::to_owned));
    let raw = Value::Object(v.clone()).to_string();
    let info = SysInfo {
        id: &id,
        uuid: &uuid,
        hostname: str_field(&v, "hostname"),
        os_username: str_field(&v, "username"),
        os: str_field(&v, "os"),
        version: str_field(&v, "version"),
        raw: &raw,
    };
    match repo::upsert_sysinfo(&app.db, &info).await? {
        Report::UuidMismatch => {
            log::warn!("设备 {} 的 uuid 与已登记不符，拒绝上报（来源 {}）", id, addr);
            repo::admin_audit(&app.db, None, "device.uuid_mismatch", &id, &uuid, &addr.ip().to_string()).await?;
            // 非 SYSINFO_UPDATED 的文本 → 客户端 120 秒后重试
            return Ok("ID_CONFLICT");
        }
        Report::Accepted => {}
    }
    if let Some(s) = secret.filter(|s| !s.is_empty() && s.len() <= 128) {
        repo::set_device_secret(&app.db, &id, &app.vault.seal(&s)).await?;
    }
    let group = str_field(&v, PRESET_GROUP).trim();
    if !group.is_empty() && group.chars().count() <= 64 {
        repo::assign_group_by_name(&app.db, &id, group).await?;
    }
    Ok("SYSINFO_UPDATED")
}

pub async fn sysinfo_ver() -> &'static str {
    ""
}

pub async fn heartbeat(State(app): State<Arc<App>>, body: Bytes) -> ApiResult<Json<Value>> {
    let v: Map<String, Value> = parse(&body)?;
    let id = str_field(&v, "id");
    let uuid = str_field(&v, "uuid");
    if !valid_id(id) {
        return Err(ApiError::bad_request("缺少 id"));
    }
    if repo::touch_online(&app.db, id, uuid).await? {
        Ok(Json(json!({})))
    } else {
        // 未登记的设备：要求客户端重新上传 sysinfo 完成登记
        Ok(Json(json!({ "sysinfo": 1 })))
    }
}

#[cfg(test)]
mod tests {
    use super::valid_id;

    #[test]
    fn id_validation() {
        assert!(valid_id("123456789"));
        assert!(valid_id("my-pc_1"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id("1;DROP"));
    }
}
