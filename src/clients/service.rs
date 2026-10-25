//! 客户端接入的编排层：模型/地址校验、Key 复用或创建、备份、写入、状态落库、撤销。

use super::*;
use crate::db::{self, client_config::ClientConfigRow, DbPool};
use crate::models::api_key::{generate_secret, CreateApiKeyRequest, StoredApiKey};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct ApplyOutcome {
    pub result: ClientApplyResult,
    pub cache_refresh: bool,
}

pub struct RestoreOutcome {
    pub result: ClientRestoreResult,
    pub cache_refresh: bool,
}

struct KeyChoice {
    reuse: bool,
    stored: Option<StoredApiKey>,
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

/// 附加项只有 Claude Code 认：其他客户端传了就明确拒绝，而不是悄悄丢掉。
fn extras_for(
    id: ClientId,
    request: &ClientApplyRequest,
    managed: Option<Value>,
) -> AppResult<ClientExtras> {
    let extras = request.extras(managed)?;
    if id != ClientId::ClaudeCode && (!extras.env.is_empty() || !extras.tier_names.is_empty()) {
        return Err(AppError::BadRequest(format!(
            "{id} has no extra settings to manage; only Claude Code does"
        )));
    }
    Ok(extras)
}

fn parse_managed(row: &ClientConfigRow) -> AppResult<Value> {
    serde_json::from_str::<Value>(&row.managed)
        .map_err(|error| AppError::BadRequest(format!("Managed client state is invalid: {error}")))
}

fn with_hash(mut managed: Value, token_hash: &str) -> Value {
    if let Some(object) = managed.as_object_mut() {
        object.insert("token_hash".to_string(), json!(token_hash));
    }
    managed
}

fn choose_key(pool: &DbPool, row: Option<&ClientConfigRow>, current: Option<&str>) -> AppResult<KeyChoice> {
    let stored = match row.and_then(|row| row.key_id.as_deref()) {
        Some(key_id) => db::api_key::get(pool, key_id)?,
        None => None,
    };
    let reuse = row
        .and_then(|row| row.token_hash.as_deref())
        .zip(current.map(str::trim).filter(|token| !token.is_empty()))
        .is_some_and(|(hash, token)| sha256_hex(token) == hash)
        && stored.as_ref().is_some_and(|key| key.enabled);
    Ok(KeyChoice { reuse, stored })
}

fn validate_models(pool: &DbPool, request: &ClientApplyRequest) -> AppResult<ResolvedModels> {
    let models = request.resolved()?;
    let available: HashSet<String> = db::proxy::list(pool)?.into_iter().map(|proxy| proxy.name).collect();
    for name in request.models() {
        if !available.contains(name) {
            return Err(AppError::BadRequest(format!(
                "Virtual model '{name}' does not exist in AI Gateway"
            )));
        }
    }
    Ok(models)
}

fn resolve_base(request: &ClientApplyRequest, admin_port: u16) -> AppResult<String> {
    let raw = request
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("http://127.0.0.1:{admin_port}"));
    let parsed = reqwest::Url::parse(&raw)
        .map_err(|_| AppError::BadRequest(format!("Invalid gateway base URL: {raw}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::BadRequest("Gateway base URL must use http or https".to_string()));
    }
    if parsed.host_str().is_none() {
        return Err(AppError::BadRequest("Gateway base URL has no host".to_string()));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::BadRequest(
            "Gateway base URL must not contain credentials".to_string(),
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(AppError::BadRequest(
            "Gateway base URL must not contain a query or fragment".to_string(),
        ));
    }
    let mut base = parsed.to_string();
    while base.ends_with('/') {
        base.pop();
    }
    Ok(base)
}

fn backup_dir(root: &Path, id: ClientId) -> PathBuf {
    root.join(id.as_str())
}

fn write_backup(root: &Path, id: ClientId, path: &Path, bytes: &[u8]) -> AppResult<PathBuf> {
    let directory = backup_dir(root, id);
    std::fs::create_dir_all(&directory)?;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "config".to_string());
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let mut target = directory.join(format!("{file_name}.{stamp}.bak"));
    let mut suffix = 1;
    while target.exists() {
        target = directory.join(format!("{file_name}.{stamp}-{suffix}.bak"));
        suffix += 1;
    }
    std::fs::write(&target, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600));
    }
    Ok(target)
}

fn write_config(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".to_string());
    let tmp = path.with_file_name(format!(".{file_name}.aigw-{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        if path.exists() {
            if let Err(error) = std::fs::remove_file(path) {
                let _ = std::fs::remove_file(&tmp);
                return Err(error.into());
            }
        }
    }
    if let Err(error) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error.into());
    }
    Ok(())
}

fn restore_original(path: &Path, original: &Option<Vec<u8>>) {
    match original {
        Some(bytes) => {
            if let Err(error) = write_config(path, bytes) {
                tracing::warn!("Failed to restore {} after a client-config error: {}", path.display(), error);
            }
        }
        None => {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn disable_key(pool: &DbPool, key_id: Option<&str>) -> (Option<String>, bool, Vec<String>) {
    let Some(key_id) = key_id else { return (None, false, vec![]) };
    match db::api_key::get(pool, key_id) {
        Ok(Some(key)) if key.enabled => match db::api_key::set_enabled(pool, key_id, false) {
            Ok(()) => (Some(key.name), true, vec![]),
            Err(error) => (None, false, vec![format!("Failed to disable gateway key '{}': {error}", key.name)]),
        },
        Ok(_) => (None, false, vec![]),
        Err(error) => (None, false, vec![format!("Failed to look up gateway key: {error}")]),
    }
}

pub fn list(pool: &DbPool, paths: &ClientPaths) -> AppResult<Vec<ClientStatus>> {
    let mut statuses = Vec::new();
    for id in ClientId::ALL {
        let adapter = adapter(id);
        let path = adapter.path(paths);
        let row = db::client_config::get(pool, id.as_str())?;
        let managed_value = row
            .as_ref()
            .and_then(|row| serde_json::from_str::<Value>(&row.managed).ok());
        let mut status = match adapter.status(paths, managed_value.as_ref()) {
            Ok(status) => status,
            Err(error) => ClientStatus {
                id: id.as_str().into(),
                name: id.display_name().into(),
                path: path.display().to_string(),
                exists: path.exists(),
                detected: path.exists(),
                managed: false,
                backup_path: None,
                current: ClientCurrent::default(),
                error: Some(error.to_string()),
                warnings: vec![],
            },
        };
        status.id = id.as_str().to_string();
        status.name = id.display_name().to_string();
        status.path = path.display().to_string();
        status.detected = status.exists;
        status.managed = row.is_some();
        status.backup_path = row.as_ref().and_then(|row| row.backup_path.clone());

        if let Some(row) = &row {
            if managed_value.is_none() {
                status.warnings.push(
                    "Managed state is corrupt; re-apply or restore the pre-apply backup.".to_string(),
                );
            }
            if !status.exists {
                status.warnings.push(
                    "A managed client config is missing on disk; re-apply or restore the backup.".to_string(),
                );
            }
            if row.key_id.is_some() {
                match db::api_key::get(pool, row.key_id.as_deref().unwrap_or_default()) {
                    Ok(Some(key)) if !key.enabled => status.warnings.push(
                        "The managed gateway key is disabled; re-apply to issue a new key.".to_string(),
                    ),
                    Ok(None) => status.warnings.push(
                        "The managed gateway key no longer exists; re-apply to issue a new key.".to_string(),
                    ),
                    Ok(_) => {}
                    Err(error) => status
                        .warnings
                        .push(format!("Failed to check the managed gateway key: {error}")),
                }
            }
        }
        statuses.push(status);
    }
    Ok(statuses)
}

pub fn preview(
    pool: &DbPool,
    paths: &ClientPaths,
    id: ClientId,
    request: &ClientApplyRequest,
    admin_port: u16,
) -> AppResult<ClientPreview> {
    let adapter = adapter(id);
    let path = adapter.path(paths);
    let models = validate_models(pool, request)?;
    let base_url = resolve_base(request, admin_port)?;
    let row = db::client_config::get(pool, id.as_str())?;
    let managed = match &row {
        Some(row) => Some(parse_managed(row)?),
        None => None,
    };
    let current = adapter.current_token(paths)?;
    let key = choose_key(pool, row.as_ref(), current.as_deref())?;
    let plan_token = if key.reuse {
        current.clone().unwrap_or_default()
    } else {
        "sk-aigw-preview-placeholder".to_string()
    };
    let extras = extras_for(id, request, managed.clone())?;
    let plan = adapter.plan(paths, &models, &base_url, &plan_token, row.is_some(), &extras)?;
    let placeholder = if key.reuse { "<managed key>" } else { "<new key on apply>" };
    let mut changes = plan.changes;
    for change in &mut changes {
        if change.secret {
            change.after = placeholder.to_string();
        }
    }
    let mut warnings = plan.warnings;
    if row.is_some() && !key.reuse {
        warnings.push(
            "The previously managed gateway key is missing or disabled; applying will create a new one."
                .to_string(),
        );
    }
    let current_summary = adapter
        .status(paths, managed.as_ref())
        .map(|status| status.current)
        .unwrap_or_default();

    let (env, tier_names) = adapter.read_extras(paths);
    Ok(ClientPreview {
        id: id.as_str().into(),
        path: path.display().to_string(),
        exists: path.exists(),
        managed: row.is_some(),
        backup_path: row.as_ref().and_then(|row| row.backup_path.clone()),
        key_action: if key.reuse { "reuse" } else { "create" }.to_string(),
        changes,
        warnings,
        current: current_summary,
        env,
        tier_names,
    })
}

pub fn apply(
    pool: &DbPool,
    paths: &ClientPaths,
    id: ClientId,
    request: &ClientApplyRequest,
    admin_port: u16,
    backup_root: &Path,
) -> AppResult<ApplyOutcome> {
    let adapter = adapter(id);
    let models = validate_models(pool, request)?;
    let base_url = resolve_base(request, admin_port)?;
    let row = db::client_config::get(pool, id.as_str())?;
    let managed = match &row {
        Some(row) => Some(parse_managed(row)?),
        None => None,
    };
    let extras = extras_for(id, request, managed.clone())?;
    let current = adapter.current_token(paths)?;
    let key = choose_key(pool, row.as_ref(), current.as_deref())?;
    let (token, created_key, key_id, key_name, key_mask) = if key.reuse {
        let stored = key
            .stored
            .as_ref()
            .ok_or_else(|| AppError::Internal("Reusable gateway key disappeared".to_string()))?;
        (
            current.clone().unwrap_or_default(),
            false,
            stored.id.clone(),
            stored.name.clone(),
            stored.key_mask.clone(),
        )
    } else {
        let token = generate_secret();
        let name = format!(
            "client-{}-{}",
            id.as_str(),
            Utc::now().format("%Y%m%d-%H%M%S")
        );
        let stored = db::api_key::create(
            pool,
            &CreateApiKeyRequest {
                name: name.clone(),
                key: Some(token.clone()),
                proxy_id: None,
                expires_at: None,
                quota_requests: None,
                quota_tokens: None,
                quota_cost: None,
                quota_window: None,
            },
        )?;
        (token, true, stored.id, name, stored.key_mask)
    };

    let plan = match adapter.plan(paths, &models, &base_url, &token, row.is_some(), &extras) {
        Ok(plan) => plan,
        Err(error) => {
            if created_key {
                let _ = db::api_key::delete(pool, &key_id);
            }
            return Err(error);
        }
    };
    let content_changed = plan.original.as_deref() != Some(plan.content.as_slice());
    let backup_path = if content_changed {
        match &plan.original {
            Some(bytes) => match write_backup(backup_root, id, &plan.path, bytes) {
                Ok(path) => Some(path.display().to_string()),
                Err(error) => {
                    if created_key {
                        let _ = db::api_key::delete(pool, &key_id);
                    }
                    return Err(error);
                }
            },
            None => None,
        }
    } else {
        row.as_ref().and_then(|row| row.backup_path.clone())
    };

    if content_changed {
        if let Err(error) = write_config(&plan.path, &plan.content) {
            restore_original(&plan.path, &plan.original);
            if created_key {
                let _ = db::api_key::delete(pool, &key_id);
            }
            return Err(error);
        }
    }

    let token_hash = sha256_hex(token.trim());
    let managed = with_hash(plan.managed.clone(), &token_hash);
    let stamp = now();
    let state = ClientConfigRow {
        client_id: id.as_str().to_string(),
        config_path: plan.path.display().to_string(),
        managed: managed.to_string(),
        token_hash: Some(token_hash),
        key_id: Some(key_id.clone()),
        key_name: Some(key_name.clone()),
        backup_path: backup_path.clone(),
        applied_at: row
            .as_ref()
            .map(|row| row.applied_at.clone())
            .unwrap_or_else(|| stamp.clone()),
        updated_at: stamp,
    };
    if let Err(error) = db::client_config::upsert(pool, &state) {
        restore_original(&plan.path, &plan.original);
        if created_key {
            let _ = db::api_key::delete(pool, &key_id);
        }
        return Err(error);
    }

    let mut warnings = plan.warnings;
    let mut cache_refresh = created_key;
    if created_key {
        if let Some(old_id) = row.as_ref().and_then(|row| row.key_id.as_deref()) {
            if old_id != key_id.as_str() {
                if let Ok(Some(old)) = db::api_key::get(pool, old_id) {
                    if old.enabled {
                        match db::api_key::set_enabled(pool, old_id, false) {
                            Ok(()) => {
                                cache_refresh = true;
                                warnings.push(format!("Previous gateway key '{}' was disabled.", old.name));
                            }
                            Err(error) => warnings.push(format!(
                                "Failed to disable previous gateway key '{}': {error}",
                                old.name
                            )),
                        }
                    }
                }
            }
        }
    }

    let mut changes = plan.changes;
    for change in &mut changes {
        if change.secret {
            change.after = "<configured>".to_string();
        }
    }
    Ok(ApplyOutcome {
        result: ClientApplyResult {
            id: id.as_str().into(),
            path: plan.path.display().to_string(),
            backup_path,
            key_id,
            key_name,
            key_mask,
            created_key,
            changes,
            warnings,
        },
        cache_refresh,
    })
}

pub fn restore(
    pool: &DbPool,
    paths: &ClientPaths,
    id: ClientId,
    backup_root: &Path,
) -> AppResult<RestoreOutcome> {
    let adapter = adapter(id);
    let row = db::client_config::get(pool, id.as_str())?
        .ok_or_else(|| AppError::BadRequest("This client is not managed by AI Gateway".to_string()))?;
    let managed = parse_managed(&row)?;
    let plan = adapter.restore(paths, &managed)?;
    let mut warnings = plan.warnings;
    let key_id = row.key_id.clone();

    if !plan.changed {
        db::client_config::delete(pool, id.as_str())?;
        let (disabled_name, key_refresh, key_warnings) = disable_key(pool, key_id.as_deref());
        warnings.extend(key_warnings);
        return Ok(RestoreOutcome {
            result: ClientRestoreResult {
                id: id.as_str().into(),
                path: plan.path.display().to_string(),
                backup_path: None,
                restored: plan.restored,
                kept: plan.kept,
                disabled_key: disabled_name,
                warnings,
            },
            cache_refresh: key_refresh,
        });
    }

    let backup_path = match &plan.original {
        Some(bytes) => match write_backup(backup_root, id, &plan.path, bytes) {
            Ok(path) => Some(path.display().to_string()),
            Err(error) => return Err(error),
        },
        None => None,
    };

    let write_result = if plan.remove_file {
        match std::fs::remove_file(&plan.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    } else {
        write_config(&plan.path, &plan.content)
    };
    if let Err(error) = write_result {
        restore_original(&plan.path, &plan.original);
        return Err(error);
    }
    db::client_config::delete(pool, id.as_str())?;
    let (disabled_name, key_refresh, key_warnings) = disable_key(pool, key_id.as_deref());
    warnings.extend(key_warnings);

    Ok(RestoreOutcome {
        result: ClientRestoreResult {
            id: id.as_str().into(),
            path: plan.path.display().to_string(),
            backup_path,
            restored: plan.restored,
            kept: plan.kept,
            disabled_key: disabled_name,
            warnings,
        },
        cache_refresh: key_refresh,
    })
}

/// 读配置文件原文。文件不存在时返回空内容，让前端可以据此新建。
pub fn read_config_file(paths: &ClientPaths, id: ClientId) -> AppResult<ConfigFile> {
    let path = adapter(id).path(paths);
    let content = match std::fs::read(&path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(AppError::Internal(error.to_string())),
    };
    Ok(ConfigFile {
        id: id.as_str().to_string(),
        path: path.display().to_string(),
        exists: path.exists(),
        content,
    })
}

/// 直接写配置文件：只校验"这个客户端还读得懂"，不解释内容，写前照例备份。
/// 用户手改过的值，撤销接入时不会被我们覆盖回去（restore 只回滚值仍等于我们写入内容的字段）。
pub fn write_config_file(
    paths: &ClientPaths,
    backup_root: &Path,
    id: ClientId,
    content: &str,
) -> AppResult<ConfigFileWrite> {
    if content.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Config content must not be empty".to_string(),
        ));
    }
    if content.len() > 512 * 1024 {
        return Err(AppError::BadRequest(
            "Config content is too large (512 KB limit)".to_string(),
        ));
    }
    validate_config_text(id, content)?;
    let path = adapter(id).path(paths);
    let original = read_bytes(&path)?;
    let backup_path = match &original {
        Some(bytes) => Some(write_backup(backup_root, id, &path, bytes)?.display().to_string()),
        None => None,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Err(error) = write_config(&path, content.as_bytes()) {
        restore_original(&path, &original);
        return Err(error);
    }
    Ok(ConfigFileWrite {
        path: path.display().to_string(),
        backup_path,
        replaced: original.is_some(),
    })
}

fn validate_config_text(id: ClientId, text: &str) -> AppResult<()> {
    let message = match id {
        ClientId::ClaudeCode => serde_json::from_str::<Value>(text)
            .err()
            .map(|error| format!("settings.json is not valid JSON: {error}"))
            .or_else(|| {
                serde_json::from_str::<Value>(text)
                    .ok()
                    .filter(|value| !value.is_object())
                    .map(|_| "settings.json must contain a JSON object".to_string())
            }),
        ClientId::Codex => text
            .parse::<toml_edit::DocumentMut>()
            .err()
            .map(|error| format!("config.toml is not valid TOML: {error}")),
        ClientId::Opencode | ClientId::Openclaw | ClientId::Pi => {
            json5::from_str::<Value>(text)
                .err()
                .map(|error| format!("config is not valid JSON5: {error}"))
        }
        ClientId::Hermes => serde_yaml::from_str::<serde_yaml::Value>(text)
            .err()
            .map(|error| format!("config.yaml is not valid YAML: {error}")),
    };
    match message {
        None => Ok(()),
        Some(text) => Err(AppError::BadRequest(text)),
    }
}

pub fn restore_backup(
    pool: &DbPool,
    paths: &ClientPaths,
    id: ClientId,
    backup_root: &Path,
) -> AppResult<RestoreOutcome> {
    let row = db::client_config::get(pool, id.as_str())?
        .ok_or_else(|| AppError::BadRequest("This client is not managed by AI Gateway".to_string()))?;
    let backup = row
        .backup_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::BadRequest("No pre-apply backup is recorded for this client".to_string()))?;
    let root = backup_dir(backup_root, id);
    if !backup.starts_with(&root) {
        return Err(AppError::BadRequest(
            "Recorded backup path is outside the client backup directory".to_string(),
        ));
    }
    let bytes = std::fs::read(&backup)
        .map_err(|error| AppError::BadRequest(format!("Cannot read backup {}: {error}", backup.display())))?;
    let path = adapter(id).path(paths);
    let original = read_bytes(&path)?;
    let new_backup = match &original {
        Some(current) => Some(write_backup(backup_root, id, &path, current)?.display().to_string()),
        None => None,
    };
    if let Err(error) = write_config(&path, &bytes) {
        restore_original(&path, &original);
        return Err(error);
    }
    db::client_config::delete(pool, id.as_str())?;
    let (disabled_name, key_refresh, mut warnings) = disable_key(pool, row.key_id.as_deref());
    warnings.push(
        "Full file restore used the pre-apply backup; changes made after apply were overwritten."
            .to_string(),
    );
    Ok(RestoreOutcome {
        result: ClientRestoreResult {
            id: id.as_str().into(),
            path: path.display().to_string(),
            backup_path: new_backup,
            restored: vec![path.display().to_string()],
            kept: vec![],
            disabled_key: disabled_name,
            warnings,
        },
        cache_refresh: key_refresh,
    })
}
