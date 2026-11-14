//! MyDesk 定制：编译期内置服务器配置 + 被控端托管密码。
//!
//! 1. 内置配置：CI 把 JSON 写进环境变量 `MYDESK_CONFIG` 后编译，格式与官方 custom.txt 解码后的内容一致
//!    （`app-name`、`override-settings`、`default-settings`、其余键进 HARD_SETTINGS）。编进二进制即可信，无需签名。
//! 2. 托管密码：被控端（服务进程）首次运行生成随机强密码设为永久密码，明文以本机密钥加密保存在
//!    配置目录 `mydesk.secret`（仅服务账户可读），随 sysinfo 上报服务端（字段 `mydesk-secret`）。
//!    若本机永久密码被改动而与保存值不符，则重新生成——服务端是唯一可信来源。

use hbb_common::{
    config::{self, compute_permanent_password_h1, decode_permanent_password_h1_from_storage, Config},
    log,
    password_security::{decrypt_str_or_original, encrypt_str_or_original},
    rand::{self, Rng},
};
use std::collections::HashMap;

/// sysinfo 中携带托管密码的字段名（与 mydesk-server 约定）。
pub const SECRET_FIELD: &str = "mydesk-secret";
/// 心跳响应中要求轮换托管密码的字段名（与 mydesk-server 约定）。
pub const ROTATE_FIELD: &str = "mydesk-rotate";
const SECRET_FILE: &str = "mydesk.secret";
const SECRET_ENC_VERSION: &str = "00";
const SECRET_LEN: usize = 20;

/// 编译期内置的配置（JSON）。未设置时为 None，客户端行为与官方一致。
pub fn builtin_config() -> Option<&'static str> {
    option_env!("MYDESK_CONFIG").filter(|s| !s.trim().is_empty())
}

/// 应用内置配置。在 `load_custom_client` 开头调用；若同时存在 custom.txt，后者覆盖同名项。
pub fn apply_builtin_config() {
    let Some(raw) = builtin_config() else {
        return;
    };
    match serde_json::from_str::<HashMap<String, serde_json::Value>>(raw) {
        Ok(data) => crate::common::apply_custom_client_data(data),
        Err(e) => log::error!("MyDesk 内置配置解析失败: {e}"),
    }
}

/// 是否为 MyDesk 构建（有内置配置）。
pub fn is_mydesk() -> bool {
    builtin_config().is_some()
}

fn random_secret() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..SECRET_LEN).map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char).collect()
}

fn load_saved_secret() -> Option<String> {
    let raw = std::fs::read_to_string(Config::path(SECRET_FILE)).ok()?;
    let (plain, decrypted, _) = decrypt_str_or_original(raw.trim(), SECRET_ENC_VERSION);
    (decrypted && !plain.is_empty()).then_some(plain)
}

fn save_secret(secret: &str) -> bool {
    let enc = encrypt_str_or_original(secret, SECRET_ENC_VERSION, config::ENCRYPT_MAX_LEN);
    let path = Config::path(SECRET_FILE);
    let res = std::fs::write(&path, enc);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    if let Err(e) = res {
        log::error!("保存 MyDesk 托管密码失败: {e}");
        return false;
    }
    true
}

/// 服务端要求轮换：丢弃本机保存的托管密码，下次构建 sysinfo 时会生成新密码并上报
///（服务端同时要求重传 sysinfo）。
pub fn rotate_secret() {
    if !is_mydesk() {
        return;
    }
    match std::fs::remove_file(Config::path(SECRET_FILE)) {
        Ok(()) => log::info!("收到服务端轮换指令，已丢弃旧的托管密码"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::error!("丢弃旧托管密码失败: {e}"),
    }
}

/// 本机永久密码是否等于 `secret`。
fn permanent_password_is(secret: &str) -> bool {
    let (storage, salt) = Config::get_local_permanent_password_storage_and_salt();
    match decode_permanent_password_h1_from_storage(&storage) {
        Some(stored) => compute_permanent_password_h1(secret, &salt) == stored,
        None => false,
    }
}

/// 确保托管密码存在且与本机永久密码一致，返回要上报的明文。仅在 MyDesk 构建的服务进程中调用。
pub fn ensure_device_secret() -> Option<String> {
    if !is_mydesk() {
        return None;
    }
    if let Some(s) = load_saved_secret() {
        if permanent_password_is(&s) {
            return Some(s);
        }
        log::warn!("本机永久密码与托管密码不一致，重新生成");
    }
    let s = random_secret();
    if !save_secret(&s) {
        return None;
    }
    if !Config::set_permanent_password(&s) || !permanent_password_is(&s) {
        log::error!("设置托管永久密码失败（可能被禁止修改永久密码）");
        return None;
    }
    log::info!("已生成新的托管永久密码，将随 sysinfo 上报服务端");
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_shape() {
        let a = random_secret();
        assert_eq!(a.len(), SECRET_LEN);
        assert_ne!(a, random_secret());
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
