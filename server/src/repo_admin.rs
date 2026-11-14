//! 管理后台专用的数据访问。

use crate::store::{now, Db};
use serde::Serialize;
use sqlx::Row;

/// 心跳间隔 15 秒，超过 45 秒未心跳视为离线。
pub const ONLINE_WINDOW: i64 = 45;

#[derive(Serialize)]
pub struct DeviceRow {
    pub id: String,
    pub hostname: String,
    pub alias: String,
    pub owner_name: String,
    pub note: String,
    pub os: String,
    pub version: String,
    pub group_id: Option<i64>,
    pub group_name: String,
    pub managed: bool,
    pub secret_at: i64,
    pub online: bool,
    pub last_online_at: i64,
    pub rotate_pending: bool,
}

pub async fn devices(db: &Db) -> Result<Vec<DeviceRow>, sqlx::Error> {
    let t = now();
    let rows = sqlx::query(
        "SELECT d.id, d.hostname, d.alias, d.owner_name, d.note, d.os, d.version, d.group_id, COALESCE(g.name, '') AS group_name, \
         d.secret_cipher != '' AS managed, d.secret_at, d.last_online_at, d.rotate_pending \
         FROM devices d LEFT JOIN device_groups g ON g.id = d.group_id ORDER BY d.group_id IS NOT NULL, g.name, d.id",
    )
    .fetch_all(db)
    .await?;
    rows.iter()
        .map(|r| {
            let last: i64 = r.try_get("last_online_at")?;
            Ok(DeviceRow {
                id: r.try_get("id")?,
                hostname: r.try_get("hostname")?,
                alias: r.try_get("alias")?,
                owner_name: r.try_get("owner_name")?,
                note: r.try_get("note")?,
                os: r.try_get("os")?,
                version: r.try_get("version")?,
                group_id: r.try_get("group_id")?,
                group_name: r.try_get("group_name")?,
                managed: r.try_get("managed")?,
                secret_at: r.try_get("secret_at")?,
                online: t - last <= ONLINE_WINDOW,
                last_online_at: last,
                rotate_pending: r.try_get::<i64, _>("rotate_pending")? != 0,
            })
        })
        .collect()
}

pub async fn update_device(db: &Db, id: &str, alias: &str, owner: &str, note: &str, group_id: Option<i64>) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("UPDATE devices SET alias = ?, owner_name = ?, note = ?, group_id = ? WHERE id = ?")
        .bind(alias)
        .bind(owner)
        .bind(note)
        .bind(group_id)
        .bind(id)
        .execute(db)
        .await?;
    Ok(r.rows_affected() > 0)
}

pub async fn set_device_flag(db: &Db, id: &str, column: DeviceFlag) -> Result<bool, sqlx::Error> {
    let sql = match column {
        DeviceFlag::Rotate => "UPDATE devices SET rotate_pending = 1 WHERE id = ?",
        DeviceFlag::Disconnect => "UPDATE devices SET disconnect_pending = 1 WHERE id = ?",
    };
    Ok(sqlx::query(sql).bind(id).execute(db).await?.rows_affected() > 0)
}

#[derive(Clone, Copy)]
pub enum DeviceFlag {
    Rotate,
    Disconnect,
}

pub async fn device_secret(db: &Db, id: &str) -> Result<Option<String>, sqlx::Error> {
    Ok(sqlx::query("SELECT secret_cipher FROM devices WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?
        .map(|r| r.get::<String, _>(0))
        .filter(|s| !s.is_empty()))
}

pub async fn delete_device(db: &Db, id: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM devices WHERE id = ?").bind(id).execute(db).await?.rows_affected() > 0)
}

// ───────────────────────────── 分组 ─────────────────────────────

#[derive(Serialize)]
pub struct GroupRow {
    pub id: i64,
    pub name: String,
    pub note: String,
    pub devices: i64,
    pub online: i64,
    pub operators: Vec<String>,
}

pub async fn groups(db: &Db) -> Result<Vec<GroupRow>, sqlx::Error> {
    let t = now();
    let rows = sqlx::query(
        "SELECT g.id, g.name, g.note, \
         (SELECT COUNT(*) FROM devices d WHERE d.group_id = g.id) AS devices, \
         (SELECT COUNT(*) FROM devices d WHERE d.group_id = g.id AND ? - d.last_online_at <= ?) AS online \
         FROM device_groups g ORDER BY g.name",
    )
    .bind(t)
    .bind(ONLINE_WINDOW)
    .fetch_all(db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let id: i64 = r.try_get("id")?;
        let operators = sqlx::query(
            "SELECT COALESCE(NULLIF(u.display_name, ''), u.username) FROM user_group_access a JOIN users u ON u.id = a.user_id \
             WHERE a.group_id = ? AND u.status = 1 ORDER BY u.username",
        )
        .bind(id)
        .fetch_all(db)
        .await?
        .iter()
        .map(|x| x.get::<String, _>(0))
        .collect();
        out.push(GroupRow {
            id,
            name: r.try_get("name")?,
            note: r.try_get("note")?,
            devices: r.try_get("devices")?,
            online: r.try_get("online")?,
            operators,
        });
    }
    Ok(out)
}

pub async fn create_group(db: &Db, name: &str, note: &str) -> Result<i64, sqlx::Error> {
    Ok(sqlx::query("INSERT INTO device_groups (name, note, created_at) VALUES (?, ?, ?)")
        .bind(name)
        .bind(note)
        .bind(now())
        .execute(db)
        .await?
        .last_insert_rowid())
}

pub async fn update_group(db: &Db, id: i64, name: &str, note: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE device_groups SET name = ?, note = ? WHERE id = ?")
        .bind(name)
        .bind(note)
        .bind(id)
        .execute(db)
        .await?
        .rows_affected()
        > 0)
}

/// 删除分组：组内设备回到“未分组”（外键 ON DELETE SET NULL）。
pub async fn delete_group(db: &Db, id: i64) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM device_groups WHERE id = ?").bind(id).execute(db).await?.rows_affected() > 0)
}

// ───────────────────────────── 用户 ─────────────────────────────

#[derive(Serialize)]
pub struct UserRow {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub status: i64,
    pub tfa: bool,
    pub group_ids: Vec<i64>,
    pub last_login_at: i64,
}

pub async fn users(db: &Db) -> Result<Vec<UserRow>, sqlx::Error> {
    let rows = sqlx::query("SELECT id, username, display_name, role, status, totp_secret != '' AS tfa, last_login_at FROM users ORDER BY role, username")
        .fetch_all(db)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let id: i64 = r.try_get("id")?;
        out.push(UserRow {
            id,
            username: r.try_get("username")?,
            display_name: r.try_get("display_name")?,
            role: r.try_get("role")?,
            status: r.try_get("status")?,
            tfa: r.try_get("tfa")?,
            group_ids: user_group_ids(db, id).await?,
            last_login_at: r.try_get("last_login_at")?,
        });
    }
    Ok(out)
}

pub async fn user_group_ids(db: &Db, user_id: i64) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query("SELECT group_id FROM user_group_access WHERE user_id = ? ORDER BY group_id")
        .bind(user_id)
        .fetch_all(db)
        .await?
        .iter()
        .map(|r| r.try_get(0))
        .collect()
}

pub async fn update_user(db: &Db, id: i64, display_name: &str, role: &str, status: i64, group_ids: &[i64]) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    let n = sqlx::query("UPDATE users SET display_name = ?, role = ?, status = ?, updated_at = ? WHERE id = ?")
        .bind(display_name)
        .bind(role)
        .bind(status)
        .bind(now())
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    sqlx::query("DELETE FROM user_group_access WHERE user_id = ?").bind(id).execute(&mut *tx).await?;
    for g in group_ids {
        sqlx::query("INSERT OR IGNORE INTO user_group_access (user_id, group_id) SELECT ?, id FROM device_groups WHERE id = ?")
            .bind(id)
            .bind(g)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(n > 0)
}

pub async fn set_user_password_by_id(db: &Db, id: i64, hash: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
        .bind(hash)
        .bind(now())
        .bind(id)
        .execute(db)
        .await?
        .rows_affected()
        > 0)
}

pub async fn totp_secret(db: &Db, id: i64) -> Result<String, sqlx::Error> {
    Ok(sqlx::query("SELECT totp_secret FROM users WHERE id = ?").bind(id).fetch_optional(db).await?.map(|r| r.get(0)).unwrap_or_default())
}

pub async fn set_totp_secret(db: &Db, id: i64, secret: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET totp_secret = ?, updated_at = ? WHERE id = ?").bind(secret).bind(now()).bind(id).execute(db).await?;
    Ok(())
}

/// 启用中的管理员数量（防止停用/降级最后一个管理员）。
pub async fn active_admins(db: &Db) -> Result<i64, sqlx::Error> {
    sqlx::query("SELECT COUNT(*) FROM users WHERE role = 'admin' AND status = 1").fetch_one(db).await?.try_get(0)
}

// ───────────────────────────── 设置 ─────────────────────────────

pub async fn setting(db: &Db, key: &str) -> Result<String, sqlx::Error> {
    Ok(sqlx::query("SELECT value FROM settings WHERE key = ?").bind(key).fetch_optional(db).await?.map(|r| r.get(0)).unwrap_or_default())
}

pub async fn set_setting(db: &Db, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(db)
        .await?;
    Ok(())
}

// ───────────────────────────── 审计查询 ─────────────────────────────

/// 审计类别。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    Conn,
    File,
    Alarm,
    Admin,
}

impl AuditKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "conn" => Self::Conn,
            "file" => Self::File,
            "alarm" => Self::Alarm,
            "admin" => Self::Admin,
            _ => return None,
        })
    }
}

/// 审计记录（统一形状，便于前端表格与 CSV）。
#[derive(Serialize)]
pub struct AuditRow {
    pub id: i64,
    pub at: i64,
    /// 主体：连接/文件 = 控制端（运维）；告警 = 设备；后台 = 操作人
    pub actor: String,
    /// 对象：设备名或后台动作
    pub target: String,
    /// 类别标签：连接类型、方向、告警类型、后台对象
    pub label: String,
    pub detail: String,
}

/// 分页查询审计，`page` 从 1 开始。返回 (总数, 本页记录)。
pub async fn audit(db: &Db, kind: AuditKind, page: i64, size: i64) -> Result<(i64, Vec<AuditRow>), sqlx::Error> {
    let size = size.clamp(1, 1000);
    let offset = (page.max(1) - 1) * size;
    let dev_name = "COALESCE((SELECT COALESCE(NULLIF(alias, ''), hostname) FROM devices WHERE id = x.device_id), x.device_id)";
    let (count_sql, sql) = match kind {
        AuditKind::Conn => (
            "SELECT COUNT(*) FROM audit_conns".to_owned(),
            format!(
                "SELECT x.id, x.created_at AS at, CASE WHEN x.peer_name != '' THEN x.peer_name || ' (' || x.peer_id || ')' ELSE x.peer_id END AS actor, \
                 {dev_name} AS target, x.action || ':' || x.conn_type AS label, x.ip AS detail FROM audit_conns x ORDER BY x.id DESC LIMIT ? OFFSET ?"
            ),
        ),
        AuditKind::File => (
            "SELECT COUNT(*) FROM audit_files".to_owned(),
            format!(
                "SELECT x.id, x.created_at AS at, x.peer_id AS actor, {dev_name} AS target, CAST(x.direction AS TEXT) AS label, x.path AS detail \
                 FROM audit_files x ORDER BY x.id DESC LIMIT ? OFFSET ?"
            ),
        ),
        AuditKind::Alarm => (
            "SELECT COUNT(*) FROM audit_alarms".to_owned(),
            format!(
                "SELECT x.id, x.created_at AS at, {dev_name} AS actor, x.device_id AS target, CAST(x.alarm_type AS TEXT) AS label, \
                 COALESCE(json_extract(x.raw, '$.info'), '') AS detail FROM audit_alarms x ORDER BY x.id DESC LIMIT ? OFFSET ?"
            ),
        ),
        AuditKind::Admin => (
            "SELECT COUNT(*) FROM audit_admin".to_owned(),
            "SELECT x.id, x.created_at AS at, COALESCE((SELECT COALESCE(NULLIF(display_name, ''), username) FROM users WHERE id = x.user_id), '系统') AS actor, \
             x.action AS target, x.target AS label, TRIM(x.detail || ' ' || x.ip) AS detail FROM audit_admin x ORDER BY x.id DESC LIMIT ? OFFSET ?"
                .to_owned(),
        ),
    };
    let total: i64 = sqlx::query(&count_sql).fetch_one(db).await?.try_get(0)?;
    let rows = sqlx::query(&sql)
        .bind(size)
        .bind(offset)
        .fetch_all(db)
        .await?
        .iter()
        .map(|r| {
            Ok(AuditRow {
                id: r.try_get("id")?,
                at: r.try_get("at")?,
                actor: r.try_get("actor")?,
                target: r.try_get("target")?,
                label: r.try_get("label")?,
                detail: r.try_get("detail")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok((total, rows))
}

// ───────────────────────────── 概览 ─────────────────────────────

#[derive(Serialize)]
pub struct Overview {
    pub devices: i64,
    pub online: i64,
    pub ungrouped: i64,
    pub unmanaged: i64,
    pub open_help: i64,
    pub conns_today: i64,
    pub alarms_24h: i64,
    pub uuid_conflicts_24h: i64,
}

pub async fn overview(db: &Db, day_start: i64) -> Result<Overview, sqlx::Error> {
    let t = now();
    let one = |sql: &'static str| async move { sqlx::query(sql).fetch_one(db).await?.try_get::<i64, _>(0) };
    Ok(Overview {
        devices: one("SELECT COUNT(*) FROM devices").await?,
        online: sqlx::query("SELECT COUNT(*) FROM devices WHERE ? - last_online_at <= ?").bind(t).bind(ONLINE_WINDOW).fetch_one(db).await?.try_get(0)?,
        ungrouped: one("SELECT COUNT(*) FROM devices WHERE group_id IS NULL").await?,
        unmanaged: one("SELECT COUNT(*) FROM devices WHERE secret_cipher = ''").await?,
        open_help: one("SELECT COUNT(*) FROM help_requests WHERE status = 'open'").await?,
        conns_today: sqlx::query("SELECT COUNT(*) FROM audit_conns WHERE action = 'auth' AND created_at >= ?").bind(day_start).fetch_one(db).await?.try_get(0)?,
        alarms_24h: sqlx::query("SELECT COUNT(*) FROM audit_alarms WHERE created_at >= ?").bind(t - 86400).fetch_one(db).await?.try_get(0)?,
        uuid_conflicts_24h: sqlx::query("SELECT COUNT(*) FROM audit_admin WHERE action = 'device.uuid_mismatch' AND created_at >= ?")
            .bind(t - 86400)
            .fetch_one(db)
            .await?
            .try_get(0)?,
    })
}
