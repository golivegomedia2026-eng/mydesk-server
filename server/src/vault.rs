//! 托管凭证加密：设备上报的连接凭证用服务端主密钥（data/master.key）以 XSalsa20-Poly1305 加密后入库。
//! 主密钥与数据库分开存放；只拿到数据库文件无法还原任何设备凭证。

use hbb_common::{bail, ResultType};
use sodiumoxide::crypto::secretbox;
use std::path::Path;

const KEY_FILE: &str = "master.key";

pub struct Vault {
    key: secretbox::Key,
}

impl Vault {
    /// 从当前目录（数据目录）加载主密钥，不存在则生成并以 0600 权限保存。
    pub fn load_or_create() -> ResultType<Self> {
        let path = Path::new(KEY_FILE);
        if path.exists() {
            let raw = std::fs::read_to_string(path)?;
            let bytes = base64::decode(raw.trim())?;
            let Some(key) = secretbox::Key::from_slice(&bytes) else {
                bail!("{} 格式错误", KEY_FILE);
            };
            return Ok(Self { key });
        }
        let key = secretbox::gen_key();
        write_private(path, &base64::encode(key.0))?;
        Ok(Self { key })
    }

    /// 加密，输出 base64(nonce || ciphertext)。
    pub fn seal(&self, plain: &str) -> String {
        let nonce = secretbox::gen_nonce();
        let mut out = nonce.0.to_vec();
        out.extend(secretbox::seal(plain.as_bytes(), &nonce, &self.key));
        base64::encode(out)
    }

    /// 解密 [`seal`](Self::seal) 的输出。
    pub fn open(&self, sealed: &str) -> ResultType<String> {
        let raw = base64::decode(sealed)?;
        if raw.len() < secretbox::NONCEBYTES {
            bail!("密文过短");
        }
        let (n, c) = raw.split_at(secretbox::NONCEBYTES);
        let nonce = secretbox::Nonce::from_slice(n).expect("长度已检查");
        let Ok(plain) = secretbox::open(c, &nonce, &self.key) else {
            bail!("解密失败：主密钥不匹配或数据被篡改");
        };
        Ok(String::from_utf8(plain)?)
    }
}

/// 写入仅属主可读写的文件。
pub fn write_private(path: &Path, content: &str) -> ResultType<()> {
    #[cfg(unix)]
    {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    #[cfg(not(unix))]
    std::fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip_and_tamper() {
        let v = Vault { key: secretbox::gen_key() };
        let s = v.seal("p@ss");
        assert_eq!(v.open(&s).unwrap(), "p@ss");
        let mut raw = base64::decode(&s).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 1;
        assert!(v.open(&base64::encode(raw)).is_err());
        let other = Vault { key: secretbox::gen_key() };
        assert!(other.open(&s).is_err());
    }
}
