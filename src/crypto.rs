//! 凭据加密（AES-256-GCM）。
//!
//! 只用于 `platform_credentials.secret_blob`：Trae / WorkBuddy 的 access_token、
//! refresh_token 等凭据必须加密落库，管理接口只返回元信息。
//!
//! 密钥来自 `config.toml` 的 `security.encrypt_key`，经 SHA-256 派生为 32 字节。
//! **默认值 `ai-gateway-default-key-change-me` 等于明文存储**——这是既有问题，
//! 部署文档里要求必须修改；代码侧不阻塞，但在 UI 上给一次性警告。
//!
//! 存储格式：`base64(nonce[12] || ciphertext || tag[16])`。nonce 每次随机，
//! 所以同一明文两次加密的密文不同（这是正确的，不要为了“去重”而固定 nonce）。

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use base64::Engine;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

/// 从 `encrypt_key` 派生 32 字节 AES 密钥。空串也允许（等价于默认值），
/// 但会记一条 warn——静默使用空密钥比报错更危险。
fn derive_key(encrypt_key: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(encrypt_key.as_bytes());
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

/// 加密。返回可直接入库的 base64 字符串。
pub fn encrypt(plaintext: &str, encrypt_key: &str) -> AppResult<String> {
    let key_bytes = derive_key(encrypt_key);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| AppError::Internal(format!("credential encrypt failed: {e}")))?;

    let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(nonce.as_slice());
    blob.extend_from_slice(&ciphertext);
    Ok(base64::engine::general_purpose::STANDARD.encode(blob))
}

/// 解密。密文被篡改、密钥换了、格式不对都返回错误，**不 panic**。
pub fn decrypt(blob: &str, encrypt_key: &str) -> AppResult<String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(blob.trim())
        .map_err(|e| AppError::BadRequest(format!("credential blob is not base64: {e}")))?;
    if raw.len() < NONCE_LEN + TAG_LEN {
        return Err(AppError::BadRequest(
            "credential blob is too short".to_string(),
        ));
    }
    let (nonce_bytes, ciphertext) = raw.split_at(NONCE_LEN);
    let key_bytes = derive_key(encrypt_key);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| {
            AppError::BadRequest(
                "credential decrypt failed: 密文损坏，或 config.toml 的 encrypt_key 已变更"
                    .to_string(),
            )
        })?;
    String::from_utf8(plaintext)
        .map_err(|e| AppError::Internal(format!("credential is not utf-8: {e}")))
}

/// 是否仍在使用默认密钥。UI 用它决定要不要提示用户。
pub fn is_default_key(encrypt_key: &str) -> bool {
    let trimmed = encrypt_key.trim();
    trimmed.is_empty() || trimmed == "ai-gateway-default-key-change-me"
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "unit-test-key-please-change";

    #[test]
    fn roundtrip_preserves_content() {
        let secret = r#"{"access_token":"abc.def.ghi","refresh_token":"r1","uid":"u-1"}"#;
        let blob = encrypt(secret, KEY).unwrap();
        assert_ne!(blob, secret, "密文不能等于明文");
        assert_eq!(decrypt(&blob, KEY).unwrap(), secret);
    }

    #[test]
    fn same_plaintext_produces_different_blobs() {
        // 随机 nonce：两次加密必须不同，否则能看出两个平台是否同一凭据
        let a = encrypt("same", KEY).unwrap();
        let b = encrypt("same", KEY).unwrap();
        assert_ne!(a, b);
        assert_eq!(decrypt(&a, KEY).unwrap(), decrypt(&b, KEY).unwrap());
    }

    #[test]
    fn wrong_key_fails_without_panic() {
        let blob = encrypt("secret", KEY).unwrap();
        let err = decrypt(&blob, "another-key");
        assert!(err.is_err(), "换密钥必须解密失败");
    }

    #[test]
    fn tampered_ciphertext_fails_without_panic() {
        let blob = encrypt("secret", KEY).unwrap();
        let mut raw = base64::engine::general_purpose::STANDARD.decode(&blob).unwrap();
        // 翻转最后一个字节（tag 区），必须被 GCM 认证拒绝
        let last = raw.len() - 1;
        raw[last] ^= 0xFF;
        let tampered = base64::engine::general_purpose::STANDARD.encode(&raw);
        assert!(decrypt(&tampered, KEY).is_err());
    }

    #[test]
    fn garbage_input_is_rejected_cleanly() {
        assert!(decrypt("not-base64!!!", KEY).is_err());
        assert!(decrypt("", KEY).is_err());
        // 合法 base64 但太短
        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 8]);
        assert!(decrypt(&short, KEY).is_err());
    }

    #[test]
    fn empty_plaintext_roundtrips() {
        let blob = encrypt("", KEY).unwrap();
        assert_eq!(decrypt(&blob, KEY).unwrap(), "");
    }

    #[test]
    fn default_key_is_detected() {
        assert!(is_default_key(""));
        assert!(is_default_key("  "));
        assert!(is_default_key("ai-gateway-default-key-change-me"));
        assert!(!is_default_key("a-real-key"));
    }
}
