//! TOTP 两步验证（RFC 6238：HMAC-SHA1，30 秒，6 位），兼容 Google Authenticator 等验证器。

use data_encoding::BASE32_NOPAD;
use hbb_common::rand::{self, RngCore};
use hmac::{Hmac, Mac};
use sha1::Sha1;

const STEP: u64 = 30;
const DIGITS: u32 = 6;

/// 生成 20 字节随机密钥，返回 Base32（无填充）。
pub fn new_secret() -> String {
    let mut b = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut b);
    BASE32_NOPAD.encode(&b)
}

fn hotp(key: &[u8], counter: u64) -> u32 {
    let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(key).expect("HMAC 接受任意长度密钥");
    mac.update(&counter.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let off = (h[19] & 0x0f) as usize;
    let bin = ((h[off] as u32 & 0x7f) << 24) | ((h[off + 1] as u32) << 16) | ((h[off + 2] as u32) << 8) | h[off + 3] as u32;
    bin % 10u32.pow(DIGITS)
}

/// 校验动态码，允许前后各一个时间窗（±30 秒时钟误差）。
pub fn verify(secret_b32: &str, code: &str, unix_now: u64) -> bool {
    let code = code.trim();
    if code.len() != DIGITS as usize || !code.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let Ok(key) = BASE32_NOPAD.decode(secret_b32.trim_end_matches('=').as_bytes()) else {
        return false;
    };
    let want: u32 = code.parse().unwrap_or(u32::MAX);
    let t = unix_now / STEP;
    [t.saturating_sub(1), t, t + 1].iter().any(|c| hotp(&key, *c) == want)
}

/// 生成验证器绑定用的 otpauth URI。
pub fn otpauth_uri(issuer: &str, account: &str, secret_b32: &str) -> String {
    fn enc(s: &str) -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect()
    }
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits=6&period=30",
        enc(issuer),
        enc(account),
        secret_b32,
        enc(issuer)
    )
}

/// 把文本编码为二维码 SVG（供后台页面直接显示，无需外部服务）。
pub fn qr_svg(text: &str) -> String {
    match qrcode::QrCode::new(text.as_bytes()) {
        Ok(code) => code.render::<qrcode::render::svg::Color>().min_dimensions(200, 200).quiet_zone(true).build(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 附录 B 的 SHA1 测试向量（密钥 "12345678901234567890"），取后 6 位。
    #[test]
    fn rfc6238_vectors() {
        let secret = BASE32_NOPAD.encode(b"12345678901234567890");
        assert!(verify(&secret, "287082", 59));
        assert!(verify(&secret, "081804", 1111111109));
        assert!(verify(&secret, "005924", 1234567890));
        assert!(!verify(&secret, "000000", 59));
        assert!(!verify(&secret, "28708", 59));
    }

    #[test]
    fn secret_and_uri() {
        let s = new_secret();
        assert_eq!(s.len(), 32);
        assert!(otpauth_uri("MyDesk", "admin", &s).starts_with("otpauth://totp/MyDesk:admin?secret="));
        assert!(qr_svg("hello").contains("<svg"));
    }
}
