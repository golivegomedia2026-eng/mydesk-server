//! 首次启动初始化：没有任何用户时创建管理员 admin，随机初始密码写入 data/admin-initial-password.txt（0600）。

use crate::{auth, repo, store::Db, vault::write_private};
use hbb_common::{log, ResultType};
use std::path::Path;

pub const INITIAL_PASSWORD_FILE: &str = "admin-initial-password.txt";

pub async fn ensure_admin(db: &Db) -> ResultType<()> {
    if repo::count_users(db).await? > 0 {
        return Ok(());
    }
    let password = auth::random_password(16);
    repo::create_user(db, "admin", &auth::hash_password(&password)?, "admin", "管理员").await?;
    let path = Path::new(INITIAL_PASSWORD_FILE);
    let _ = std::fs::remove_file(path);
    write_private(path, &format!("用户名: admin\n初始密码: {password}\n登录后请立即修改密码，并删除本文件。\n"))?;
    log::warn!("已创建管理员 admin，初始密码见数据目录下的 {}", INITIAL_PASSWORD_FILE);
    Ok(())
}

/// 重置指定用户的密码，返回新密码（命令行 `--reset-password <用户名>` 使用）。
pub async fn reset_password(db: &Db, username: &str) -> ResultType<String> {
    let password = auth::random_password(16);
    if !repo::set_user_password(db, username, &auth::hash_password(&password)?).await? {
        hbb_common::bail!("用户 {} 不存在", username);
    }
    Ok(password)
}
