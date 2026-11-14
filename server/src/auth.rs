//! 账号密码与登录令牌。

use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use hbb_common::{
    rand::{self, RngCore},
    sha2::{Digest, Sha256},
    ResultType,
};

/// 令牌有效期：30 天，使用时滑动续期。
pub const TOKEN_TTL_SECS: i64 = 30 * 24 * 3600;

/// argon2id 哈希密码。
pub fn hash_password(password: &str) -> ResultType<String> {
    let mut raw = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut raw);
    let salt = SaltString::encode_b64(&raw).map_err(|e| hbb_common::anyhow::anyhow!("盐生成失败: {e}"))?;
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| hbb_common::anyhow::anyhow!("密码哈希失败: {e}"))?
        .to_string())
}

/// 校验密码；哈希格式错误按不匹配处理。
pub fn verify_password(password: &str, phc: &str) -> bool {
    PasswordHash::new(phc)
        .map(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

/// 生成随机令牌（明文，交给客户端）。
pub fn new_token() -> String {
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    hex(&b)
}

/// 生成指定长度的随机密码（字母数字，去掉易混字符）。
pub fn random_password(len: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789";
    let mut b = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut b);
    b.iter().map(|x| CHARS[*x as usize % CHARS.len()] as char).collect()
}

/// 令牌入库前取 SHA-256。
pub fn token_digest(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let h = hash_password("s3cret!").unwrap();
        assert!(verify_password("s3cret!", &h));
        assert!(!verify_password("wrong", &h));
        assert!(!verify_password("s3cret!", "not-a-phc"));
    }

    #[test]
    fn tokens_are_random_and_digest_stable() {
        let a = new_token();
        assert_eq!(a.len(), 64);
        assert_ne!(a, new_token());
        assert_eq!(token_digest(&a), token_digest(&a));
        assert_ne!(token_digest(&a), a);
        assert_eq!(random_password(12).len(), 12);
    }
}
