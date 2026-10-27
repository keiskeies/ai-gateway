//! 上游凭据保险箱（`platform_credentials`）。
//!
//! 与 `platforms.api_key` 的分工：
//! - `api_key` 是**单个字符串**，适用于标准 OpenAI / Anthropic 上游
//! - 本表存**结构化凭据**（access_token + refresh_token + uid + enterprise_id + domain
//!   + expires_at），适用于 Trae / WorkBuddy 这类需要 OAuth 的私有协议上游
//!
//! 明文只在进程内出现：落库前 AES-256-GCM 加密，管理接口只返回元信息。

use serde::{Deserialize, Serialize};

use crate::db::DbPool;
use crate::error::{AppError, AppResult};

/// 凭据形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    /// 只有一个 access_token（Trae 的 Cloud-IDE-JWT、WorkBuddy 的 Bearer）
    ApiKey,
    /// access_token + refresh_token，可自刷新
    OAuth,
}

impl CredentialKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::OAuth => "oauth",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "oauth" => Self::OAuth,
            _ => Self::ApiKey,
        }
    }
}

/// 解密后的凭据明文。**永远不要序列化到 HTTP 响应里。**
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CredentialSecret {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// WorkBuddy 的 X-User-Id
    #[serde(default)]
    pub uid: Option<String>,
    /// WorkBuddy 的 X-Enterprise-Id
    #[serde(default)]
    pub enterprise_id: Option<String>,
    /// WorkBuddy 的 X-Domain
    #[serde(default)]
    pub domain: Option<String>,
    /// Trae 的 x-device-id
    #[serde(default)]
    pub device_id: Option<String>,
    /// Trae 的 x-machine-id
    #[serde(default)]
    pub machine_id: Option<String>,
    /// 过期时刻（RFC3339）。None = 不过期 / 未知。
    #[serde(default)]
    pub expires_at: Option<String>,
}

impl CredentialSecret {
    /// 基本校验：至少要有 access_token。
    pub fn validate(&self) -> Result<(), String> {
        if self.access_token.trim().is_empty() {
            return Err("access_token is required".to_string());
        }
        if let Some(at) = &self.expires_at {
            if chrono::DateTime::parse_from_rfc3339(at).is_err() {
                return Err("expires_at must be RFC3339, e.g. 2026-11-03T14:22:00+08:00".to_string());
            }
        }
        Ok(())
    }

    /// 是否已过期（留 5 分钟余量，与 WorkBuddy 的官方建议一致）。
    pub fn is_expiring_within(&self, minutes: i64) -> bool {
        let Some(at) = &self.expires_at else {
            return false;
        };
        let Ok(expires) = chrono::DateTime::parse_from_rfc3339(at) else {
            return false;
        };
        let deadline = chrono::Utc::now() + chrono::Duration::minutes(minutes);
        expires.with_timezone(&chrono::Utc) <= deadline
    }
}

/// 对外暴露的凭据**元信息**。不含任何 secret 字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialMeta {
    pub platform_id: String,
    pub kind: String,
    pub refreshed_at: Option<String>,
    pub failure_count: i64,
    pub last_error: Option<String>,
    pub allow_self_refresh: bool,
    pub created_at: String,
    pub updated_at: String,
    /// 过期时刻（来自 secret 内的 expires_at；只回时间，不回 token）
    pub expires_at: Option<String>,
    /// 是否已过期（供 UI 显示状态）
    pub expired: bool,
    /// 凭据是否仍在用默认 encrypt_key 加密（UI 提示用）
    pub uses_default_key: bool,
}

/// 一条完整的库记录（secret 已解密）。
pub struct StoredCredential {
    pub meta: CredentialMeta,
    pub secret: CredentialSecret,
}

const COLS: &str = "platform_id, secret_blob, kind, refreshed_at, failure_count, last_error, \
    allow_self_refresh, created_at, updated_at";

/// 读取并解密。凭据不存在返回 Ok(None)，解密失败返回 Err（不静默当成没有）。
pub fn get(pool: &DbPool, platform_id: &str, encrypt_key: &str) -> AppResult<Option<StoredCredential>> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let row = conn.query_row(
        &format!("SELECT {COLS} FROM platform_credentials WHERE platform_id = ?1"),
        [platform_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        },
    );
    let (pid, blob, kind, refreshed_at, failure_count, last_error, allow_self_refresh, created_at, updated_at) =
        match row {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

    let plaintext = crate::crypto::decrypt(&blob, encrypt_key)?;
    let secret: CredentialSecret = serde_json::from_str(&plaintext)
        .map_err(|e| AppError::Internal(format!("credential json invalid: {e}")))?;
    let expired = secret
        .expires_at
        .as_ref()
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|t| t.with_timezone(&chrono::Utc) <= chrono::Utc::now())
        .unwrap_or(false);

    Ok(Some(StoredCredential {
        meta: CredentialMeta {
            platform_id: pid,
            kind,
            refreshed_at,
            failure_count,
            last_error,
            allow_self_refresh: allow_self_refresh != 0,
            created_at,
            updated_at,
            expires_at: secret.expires_at.clone(),
            expired,
            uses_default_key: crate::crypto::is_default_key(encrypt_key),
        },
        secret,
    }))
}

/// 写入 / 覆盖凭据。保留已有的 `created_at` 与 `refreshed_at` 语义：
/// 用户手动重新导入视为「重新开始」，失败计数清零。
pub fn upsert(
    pool: &DbPool,
    platform_id: &str,
    secret: &CredentialSecret,
    kind: CredentialKind,
    allow_self_refresh: bool,
    encrypt_key: &str,
) -> AppResult<()> {
    secret
        .validate()
        .map_err(AppError::BadRequest)?;
    let json = serde_json::to_string(secret)
        .map_err(|e| AppError::Internal(format!("credential serialize failed: {e}")))?;
    let blob = crate::crypto::encrypt(&json, encrypt_key)?;
    let now = chrono::Utc::now().to_rfc3339();

    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO platform_credentials
            (platform_id, secret_blob, kind, refreshed_at, failure_count, last_error,
             allow_self_refresh, created_at, updated_at)
         VALUES (?1, ?2, ?3, NULL, 0, NULL, ?4, ?5, ?5)
         ON CONFLICT(platform_id) DO UPDATE SET
            secret_blob = excluded.secret_blob,
            kind = excluded.kind,
            failure_count = 0,
            last_error = NULL,
            allow_self_refresh = excluded.allow_self_refresh,
            updated_at = excluded.updated_at",
        rusqlite::params![
            platform_id,
            blob,
            kind.as_str(),
            if allow_self_refresh { 1 } else { 0 },
            now,
        ],
    )?;
    Ok(())
}

pub fn delete(pool: &DbPool, platform_id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM platform_credentials WHERE platform_id = ?1", [platform_id])?;
    Ok(())
}

/// 刷新成功后调用：更新 blob、清空失败计数、记 refreshed_at。
pub fn mark_refreshed(
    pool: &DbPool,
    platform_id: &str,
    secret: &CredentialSecret,
    encrypt_key: &str,
) -> AppResult<()> {
    let json = serde_json::to_string(secret)
        .map_err(|e| AppError::Internal(format!("credential serialize failed: {e}")))?;
    let blob = crate::crypto::encrypt(&json, encrypt_key)?;
    let now = chrono::Utc::now().to_rfc3339();
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE platform_credentials
            SET secret_blob = ?2, refreshed_at = ?3, failure_count = 0, last_error = NULL, updated_at = ?3
          WHERE platform_id = ?1",
        rusqlite::params![platform_id, blob, now],
    )?;
    Ok(())
}

/// 刷新失败：失败计数 +1 并记录最近错误。返回新的计数。
pub fn mark_failure(pool: &DbPool, platform_id: &str, error: &str) -> AppResult<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE platform_credentials
            SET failure_count = failure_count + 1, last_error = ?2, updated_at = ?3
          WHERE platform_id = ?1",
        rusqlite::params![platform_id, error, now],
    )?;
    let count: i64 = conn.query_row(
        "SELECT failure_count FROM platform_credentials WHERE platform_id = ?1",
        [platform_id],
        |r| r.get(0),
    )?;
    Ok(count)
}

pub fn set_allow_self_refresh(pool: &DbPool, platform_id: &str, allow: bool) -> AppResult<()> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    conn.execute(
        "UPDATE platform_credentials SET allow_self_refresh = ?2, updated_at = ?3 WHERE platform_id = ?1",
        rusqlite::params![platform_id, if allow { 1 } else { 0 }, chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// 后台任务用：列出所有平台的 id + 元信息（不解密，省一次 AES）。
pub fn list_meta(pool: &DbPool) -> AppResult<Vec<CredentialMeta>> {
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT platform_id, kind, refreshed_at, failure_count, last_error,
                allow_self_refresh, created_at, updated_at
           FROM platform_credentials ORDER BY created_at",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CredentialMeta {
            platform_id: row.get(0)?,
            kind: row.get(1)?,
            refreshed_at: row.get(2)?,
            failure_count: row.get(3)?,
            last_error: row.get(4)?,
            allow_self_refresh: row.get::<_, i64>(5)? != 0,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
            expires_at: None,
            expired: false,
            uses_default_key: false,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// 删平台时一并删凭据。表上没有外键（V28 建表时刻意没加，
/// 因为 platforms 的删除路径已有事务），所以这里显式清理。
pub fn delete_for_platform(pool: &DbPool, platform_id: &str) -> AppResult<()> {
    delete(pool, platform_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_kind_parses() {
        assert_eq!(CredentialKind::parse("oauth"), CredentialKind::OAuth);
        assert_eq!(CredentialKind::parse("OAuth"), CredentialKind::OAuth);
        assert_eq!(CredentialKind::parse("api_key"), CredentialKind::ApiKey);
        assert_eq!(CredentialKind::parse("garbage"), CredentialKind::ApiKey);
        assert_eq!(CredentialKind::ApiKey.as_str(), "api_key");
    }

    #[test]
    fn empty_access_token_is_rejected() {
        let s = CredentialSecret::default();
        assert!(s.validate().is_err());
        let s = CredentialSecret { access_token: "   ".into(), ..Default::default() };
        assert!(s.validate().is_err());
    }

    #[test]
    fn valid_secret_passes() {
        let s = CredentialSecret {
            access_token: "abc".into(),
            refresh_token: Some("r".into()),
            ..Default::default()
        };
        assert!(s.validate().is_ok());
    }

    #[test]
    fn bad_expires_at_is_rejected() {
        let s = CredentialSecret {
            access_token: "abc".into(),
            expires_at: Some("not-a-date".into()),
            ..Default::default()
        };
        assert!(s.validate().is_err());
        let ok = CredentialSecret {
            access_token: "abc".into(),
            expires_at: Some("2026-11-03T14:22:00+08:00".into()),
            ..Default::default()
        };
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn expiry_window_detection() {
        let soon = chrono::Utc::now() + chrono::Duration::minutes(2);
        let s = CredentialSecret {
            access_token: "a".into(),
            expires_at: Some(soon.to_rfc3339()),
            ..Default::default()
        };
        assert!(s.is_expiring_within(5), "2 分钟后过期，5 分钟窗口内应命中");
        assert!(!s.is_expiring_within(1), "2 分钟后过期，1 分钟窗口内不该命中");

        let far = chrono::Utc::now() + chrono::Duration::hours(2);
        let s = CredentialSecret {
            access_token: "a".into(),
            expires_at: Some(far.to_rfc3339()),
            ..Default::default()
        };
        assert!(!s.is_expiring_within(30));
    }

    #[test]
    fn missing_expiry_never_expires() {
        let s = CredentialSecret { access_token: "a".into(), ..Default::default() };
        assert!(!s.is_expiring_within(60 * 24 * 365));
    }

    #[test]
    fn secret_serializes_without_leaking_into_meta() {
        // CredentialMeta 的字段里不能出现 token 字段名
        let meta = CredentialMeta {
            platform_id: "p1".into(),
            kind: "oauth".into(),
            refreshed_at: None,
            failure_count: 0,
            last_error: None,
            allow_self_refresh: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            expires_at: None,
            expired: false,
            uses_default_key: false,
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("access_token"));
        assert!(!json.contains("refresh_token"));
        assert!(!json.contains("secret"));
    }
}
