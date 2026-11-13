//! 数据访问：所有 SQL 集中于此，HTTP 层不直接写 SQL。

use crate::{auth, store::now, store::Db};
use sqlx::{sqlite::SqliteRow, Row};

// ───────────────────────────── 用户与令牌 ─────────────────────────────

#[derive(Debug, Clone)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub display_name: String,
    pub email: String,
    pub note: String,
    pub role: String,
    pub status: i64,
}

const USER_COLS: &str = "id, username, password_hash, display_name, email, note, role, status";

fn user_from(r: &SqliteRow) -> Result<User, sqlx::Error> {
    Ok(User {
        id: r.try_get("id")?,
        username: r.try_get("username")?,
        password_hash: r.try_get("password_hash")?,
        display_name: r.try_get("display_name")?,
        email: r.try_get("email")?,
        note: r.try_get("note")?,
        role: r.try_get("role")?,
        status: r.try_get("status")?,
    })
}

pub async fn user_by_name(db: &Db, username: &str) -> Result<Option<User>, sqlx::Error> {
    let sql = format!("SELECT {USER_COLS} FROM users WHERE username = ?");
    sqlx::query(&sql).bind(username).fetch_optional(db).await?.as_ref().map(user_from).transpose()
}

/// 由明文令牌查用户：令牌未过期且用户启用；距上次使用超过 1 小时则滑动续期。
pub async fn user_by_token(db: &Db, token: &str) -> Result<Option<User>, sqlx::Error> {
    let digest = auth::token_digest(token);
    let t = now();
    let sql = format!(
        "SELECT u.{}, t.last_used_at FROM tokens t JOIN users u ON u.id = t.user_id \
         WHERE t.token_hash = ? AND t.expires_at > ? AND u.status = 1",
        USER_COLS.replace(", ", ", u.")
    );
    let Some(row) = sqlx::query(&sql).bind(&digest).bind(t).fetch_optional(db).await? else {
        return Ok(None);
    };
    let last: i64 = row.try_get("last_used_at")?;
    if t - last > 3600 {
        sqlx::query("UPDATE tokens SET last_used_at = ?, expires_at = ? WHERE token_hash = ?")
            .bind(t)
            .bind(t + auth::TOKEN_TTL_SECS)
            .bind(&digest)
            .execute(db)
            .await?;
    }
    Ok(Some(user_from(&row)?))
}

/// 签发令牌，返回明文。
pub async fn create_token(db: &Db, user_id: i64, client_id: &str, client_uuid: &str) -> Result<String, sqlx::Error> {
    let token = auth::new_token();
    let t = now();
    sqlx::query(
        "INSERT INTO tokens (token_hash, user_id, client_id, client_uuid, created_at, last_used_at, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(auth::token_digest(&token))
    .bind(user_id)
    .bind(client_id)
    .bind(client_uuid)
    .bind(t)
    .bind(t)
    .bind(t + auth::TOKEN_TTL_SECS)
    .execute(db)
    .await?;
    Ok(token)
}

pub async fn delete_token(db: &Db, token: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM tokens WHERE token_hash = ?").bind(auth::token_digest(token)).execute(db).await?;
    Ok(())
}

pub async fn count_users(db: &Db) -> Result<i64, sqlx::Error> {
    sqlx::query("SELECT COUNT(*) FROM users").fetch_one(db).await?.try_get(0)
}

pub async fn create_user(db: &Db, username: &str, password_hash: &str, role: &str, display_name: &str) -> Result<i64, sqlx::Error> {
    let t = now();
    let r = sqlx::query(
        "INSERT INTO users (username, password_hash, display_name, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(username)
    .bind(password_hash)
    .bind(display_name)
    .bind(role)
    .bind(t)
    .bind(t)
    .execute(db)
    .await?;
    Ok(r.last_insert_rowid())
}

pub async fn set_user_password(db: &Db, username: &str, password_hash: &str) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE username = ?")
        .bind(password_hash)
        .bind(now())
        .bind(username)
        .execute(db)
        .await?;
    Ok(r.rows_affected() > 0)
}

/// 分组面板用户列表：管理员看全部启用用户，其他人只看自己。
pub async fn visible_users(db: &Db, me: &User) -> Result<Vec<User>, sqlx::Error> {
    let sql = if me.role == "admin" {
        format!("SELECT {USER_COLS} FROM users WHERE status = 1 ORDER BY username")
    } else {
        format!("SELECT {USER_COLS} FROM users WHERE id = ?")
    };
    let rows = sqlx::query(&sql).bind(me.id).fetch_all(db).await?;
    rows.iter().map(user_from).collect()
}

pub async fn user_by_id(db: &Db, id: i64) -> Result<Option<User>, sqlx::Error> {
    let sql = format!("SELECT {USER_COLS} FROM users WHERE id = ?");
    sqlx::query(&sql).bind(id).fetch_optional(db).await?.as_ref().map(user_from).transpose()
}

// ───────────────────────────── 分组与设备 ─────────────────────────────

#[derive(Debug, Clone)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub note: String,
}

/// 用户可访问的分组。管理员：全部；运维：被授权的分组。
pub async fn accessible_groups(db: &Db, user_id: i64, is_admin: bool) -> Result<Vec<Group>, sqlx::Error> {
    let rows = if is_admin {
        sqlx::query("SELECT id, name, note FROM device_groups ORDER BY name").fetch_all(db).await?
    } else {
        sqlx::query(
            "SELECT g.id, g.name, g.note FROM device_groups g JOIN user_group_access a ON a.group_id = g.id \
             WHERE a.user_id = ? ORDER BY g.name",
        )
        .bind(user_id)
        .fetch_all(db)
        .await?
    };
    rows.iter()
        .map(|r| Ok(Group { id: r.try_get("id")?, name: r.try_get("name")?, note: r.try_get("note")? }))
        .collect()
}

#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
    pub uuid: String,
    pub hostname: String,
    pub os_username: String,
    pub os: String,
    pub version: String,
    pub alias: String,
    pub owner_name: String,
    pub note: String,
    pub group_id: Option<i64>,
    pub group_name: String,
    pub secret_cipher: String,
    pub last_online_at: i64,
}

const DEVICE_SELECT: &str = "SELECT d.id, d.uuid, d.hostname, d.os_username, d.os, d.version, d.alias, d.owner_name, \
     d.note, d.group_id, COALESCE(g.name, '') AS group_name, d.secret_cipher, d.last_online_at \
     FROM devices d LEFT JOIN device_groups g ON g.id = d.group_id";

fn device_from(r: &SqliteRow) -> Result<Device, sqlx::Error> {
    Ok(Device {
        id: r.try_get("id")?,
        uuid: r.try_get("uuid")?,
        hostname: r.try_get("hostname")?,
        os_username: r.try_get("os_username")?,
        os: r.try_get("os")?,
        version: r.try_get("version")?,
        alias: r.try_get("alias")?,
        owner_name: r.try_get("owner_name")?,
        note: r.try_get("note")?,
        group_id: r.try_get("group_id")?,
        group_name: r.try_get("group_name")?,
        secret_cipher: r.try_get("secret_cipher")?,
        last_online_at: r.try_get("last_online_at")?,
    })
}

/// 某分组下的设备；`None` = 未分组。
pub async fn devices_in_group(db: &Db, group_id: Option<i64>) -> Result<Vec<Device>, sqlx::Error> {
    let rows = match group_id {
        Some(g) => sqlx::query(&format!("{DEVICE_SELECT} WHERE d.group_id = ? ORDER BY d.id")).bind(g).fetch_all(db).await?,
        None => sqlx::query(&format!("{DEVICE_SELECT} WHERE d.group_id IS NULL ORDER BY d.id")).fetch_all(db).await?,
    };
    rows.iter().map(device_from).collect()
}

/// 用户可访问的全部设备（分组面板用，不含凭证）。
pub async fn accessible_devices(db: &Db, user_id: i64, is_admin: bool) -> Result<Vec<Device>, sqlx::Error> {
    let rows = if is_admin {
        sqlx::query(&format!("{DEVICE_SELECT} ORDER BY d.id")).fetch_all(db).await?
    } else {
        sqlx::query(&format!(
            "{DEVICE_SELECT} JOIN user_group_access a ON a.group_id = d.group_id WHERE a.user_id = ? ORDER BY d.id"
        ))
        .bind(user_id)
        .fetch_all(db)
        .await?
    };
    rows.iter().map(device_from).collect()
}

/// 设备上报结果。
#[derive(Debug, PartialEq, Eq)]
pub enum Report {
    /// 新设备或 uuid 匹配，已更新
    Accepted,
    /// 同一 ID 已绑定其他机器（uuid 不符），拒绝覆盖
    UuidMismatch,
}

/// sysinfo 中服务端关心的字段。
#[derive(Debug, Default)]
pub struct SysInfo<'a> {
    pub id: &'a str,
    pub uuid: &'a str,
    pub hostname: &'a str,
    pub os_username: &'a str,
    pub os: &'a str,
    pub version: &'a str,
    pub raw: &'a str,
}

/// 首次见到的设备按 uuid 绑定（TOFU）；之后 uuid 不符的上报一律拒绝，防止冒用 ID 覆盖托管凭证。
pub async fn upsert_sysinfo(db: &Db, s: &SysInfo<'_>) -> Result<Report, sqlx::Error> {
    let t = now();
    let existing: Option<String> =
        sqlx::query("SELECT uuid FROM devices WHERE id = ?").bind(s.id).fetch_optional(db).await?.map(|r| r.get(0));
    match existing {
        Some(u) if !u.is_empty() && u != s.uuid => Ok(Report::UuidMismatch),
        Some(_) => {
            sqlx::query(
                "UPDATE devices SET uuid = ?, hostname = ?, os_username = ?, os = ?, version = ?, sysinfo = ?, \
                 last_online_at = ? WHERE id = ?",
            )
            .bind(s.uuid)
            .bind(s.hostname)
            .bind(s.os_username)
            .bind(s.os)
            .bind(s.version)
            .bind(s.raw)
            .bind(t)
            .bind(s.id)
            .execute(db)
            .await?;
            Ok(Report::Accepted)
        }
        None => {
            sqlx::query(
                "INSERT INTO devices (id, uuid, hostname, os_username, os, version, sysinfo, first_seen_at, last_online_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(s.id)
            .bind(s.uuid)
            .bind(s.hostname)
            .bind(s.os_username)
            .bind(s.os)
            .bind(s.version)
            .bind(s.raw)
            .bind(t)
            .bind(t)
            .execute(db)
            .await?;
            Ok(Report::Accepted)
        }
    }
}

/// 保存托管凭证密文（调用方已确认 uuid 匹配）。
pub async fn set_device_secret(db: &Db, id: &str, cipher: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE devices SET secret_cipher = ?, secret_at = ? WHERE id = ?")
        .bind(cipher)
        .bind(now())
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// 将设备移入分组（按名称，不存在则创建）。
pub async fn assign_group_by_name(db: &Db, device_id: &str, group_name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR IGNORE INTO device_groups (name, created_at) VALUES (?, ?)")
        .bind(group_name)
        .bind(now())
        .execute(db)
        .await?;
    sqlx::query("UPDATE devices SET group_id = (SELECT id FROM device_groups WHERE name = ?) WHERE id = ? AND group_id IS NULL")
        .bind(group_name)
        .bind(device_id)
        .execute(db)
        .await?;
    Ok(())
}

/// 心跳：更新在线时间。返回 false 表示设备未知或 uuid 不符。
pub async fn touch_online(db: &Db, id: &str, uuid: &str) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("UPDATE devices SET last_online_at = ? WHERE id = ? AND (uuid = ? OR uuid = '')")
        .bind(now())
        .bind(id)
        .bind(uuid)
        .execute(db)
        .await?;
    Ok(r.rows_affected() > 0)
}

/// 设备身份校验（审计等无鉴权上报用）。
pub async fn device_matches(db: &Db, id: &str, uuid: &str) -> Result<bool, sqlx::Error> {
    let n: i64 = sqlx::query("SELECT COUNT(*) FROM devices WHERE id = ? AND uuid = ?")
        .bind(id)
        .bind(uuid)
        .fetch_one(db)
        .await?
        .try_get(0)?;
    Ok(n > 0)
}

// ───────────────────────────── 个人地址簿 ─────────────────────────────

pub async fn personal_peers(db: &Db, user_id: i64) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query("SELECT data FROM ab_personal WHERE user_id = ? ORDER BY peer_id")
        .bind(user_id)
        .fetch_all(db)
        .await?
        .iter()
        .map(|r| r.try_get("data"))
        .collect()
}

pub async fn personal_peer(db: &Db, user_id: i64, peer_id: &str) -> Result<Option<String>, sqlx::Error> {
    Ok(sqlx::query("SELECT data FROM ab_personal WHERE user_id = ? AND peer_id = ?")
        .bind(user_id)
        .bind(peer_id)
        .fetch_optional(db)
        .await?
        .map(|r| r.get(0)))
}

pub async fn personal_peer_put(db: &Db, user_id: i64, peer_id: &str, data: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO ab_personal (user_id, peer_id, data, updated_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT(user_id, peer_id) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(peer_id)
    .bind(data)
    .bind(now())
    .execute(db)
    .await?;
    Ok(())
}

pub async fn personal_peer_delete(db: &Db, user_id: i64, peer_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM ab_personal WHERE user_id = ? AND peer_id = ?").bind(user_id).bind(peer_id).execute(db).await?;
    Ok(())
}

pub async fn personal_tags(db: &Db, user_id: i64) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query("SELECT name, color FROM ab_personal_tags WHERE user_id = ? ORDER BY name")
        .bind(user_id)
        .fetch_all(db)
        .await?
        .iter()
        .map(|r| Ok((r.try_get("name")?, r.try_get("color")?)))
        .collect()
}

pub async fn personal_tag_put(db: &Db, user_id: i64, name: &str, color: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO ab_personal_tags (user_id, name, color) VALUES (?, ?, ?) \
         ON CONFLICT(user_id, name) DO UPDATE SET color = excluded.color",
    )
    .bind(user_id)
    .bind(name)
    .bind(color)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn personal_tag_delete(db: &Db, user_id: i64, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM ab_personal_tags WHERE user_id = ? AND name = ?").bind(user_id).bind(name).execute(db).await?;
    Ok(())
}

pub async fn personal_tag_rename(db: &Db, user_id: i64, old: &str, new: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE ab_personal_tags SET name = ? WHERE user_id = ? AND name = ?")
        .bind(new)
        .bind(user_id)
        .bind(old)
        .execute(db)
        .await?;
    Ok(())
}

// ───────────────────────────── 审计 ─────────────────────────────

/// 写入一条审计；`nonce` 重复时忽略（客户端重试）。返回是否新插入。
pub async fn insert_conn_audit(
    db: &Db,
    nonce: Option<&str>,
    device_id: &str,
    conn_id: i64,
    session_id: &str,
    action: &str,
    peer_id: &str,
    peer_name: &str,
    ip: &str,
    conn_type: i64,
    raw: &str,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "INSERT OR IGNORE INTO audit_conns (nonce, device_id, conn_id, session_id, action, peer_id, peer_name, ip, conn_type, raw, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(nonce)
    .bind(device_id)
    .bind(conn_id)
    .bind(session_id)
    .bind(action)
    .bind(peer_id)
    .bind(peer_name)
    .bind(ip)
    .bind(conn_type)
    .bind(raw)
    .bind(now())
    .execute(db)
    .await?;
    Ok(r.rows_affected() > 0)
}

pub async fn insert_file_audit(
    db: &Db,
    nonce: Option<&str>,
    device_id: &str,
    peer_id: &str,
    path: &str,
    is_file: bool,
    direction: i64,
    raw: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR IGNORE INTO audit_files (nonce, device_id, peer_id, path, is_file, direction, raw, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(nonce)
    .bind(device_id)
    .bind(peer_id)
    .bind(path)
    .bind(is_file)
    .bind(direction)
    .bind(raw)
    .bind(now())
    .execute(db)
    .await?;
    Ok(())
}

pub async fn insert_alarm_audit(db: &Db, nonce: Option<&str>, device_id: &str, typ: i64, raw: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR IGNORE INTO audit_alarms (nonce, device_id, alarm_type, raw, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(nonce)
        .bind(device_id)
        .bind(typ)
        .bind(raw)
        .bind(now())
        .execute(db)
        .await?;
    Ok(())
}

/// 管理/敏感操作审计（含托管凭证下发）。
pub async fn admin_audit(db: &Db, user_id: Option<i64>, action: &str, target: &str, detail: &str, ip: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO audit_admin (user_id, action, target, detail, ip, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(user_id)
        .bind(action)
        .bind(target)
        .bind(detail)
        .bind(ip)
        .bind(now())
        .execute(db)
        .await?;
    Ok(())
}
