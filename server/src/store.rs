//! 数据库：打开 mydesk.db 并按 `PRAGMA user_version` 顺序执行迁移。

use hbb_common::{log, ResultType};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous},
    Executor, Row,
};
use std::{str::FromStr, time::Duration};

/// 迁移脚本，下标 + 1 即目标 user_version；只追加，不修改已发布的脚本。
const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_init.sql"), include_str!("../migrations/0002_admin.sql")];

pub type Db = SqlitePool;

/// 打开（不存在则创建）数据库并迁移到最新版本。
pub async fn open(path: &str) -> ResultType<Db> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{path}"))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let db = SqlitePoolOptions::new().max_connections(8).connect_with(opts).await?;
    migrate(&db).await?;
    Ok(db)
}

async fn migrate(db: &Db) -> ResultType<()> {
    let current: i64 = sqlx::query("PRAGMA user_version").fetch_one(db).await?.try_get(0)?;
    let target = MIGRATIONS.len() as i64;
    if current > target {
        hbb_common::bail!("数据库版本 {} 高于本程序支持的 {}，请使用更新版本的 mydesk-server", current, target);
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let ver = i as i64 + 1;
        let mut tx = db.begin().await?;
        tx.execute(*sql).await?;
        tx.execute(format!("PRAGMA user_version = {ver}").as_str()).await?;
        tx.commit().await?;
        log::info!("数据库已迁移到版本 {}", ver);
    }
    Ok(())
}

/// 当前 Unix 秒。
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
