//! Codex：写 `~/.codex/config.toml` 的 `model` / `model_provider` 与
//! `[model_providers.ai-gateway]`。使用 Chat wire API，复用 AI Gateway 的
//! `/v1/chat/completions`；`auth.json`（官方登录）完全不碰。
//!
//! 用 toml_edit 做增量编辑，其他表（MCP、projects、profiles…）的注释与顺序保留。

use super::*;
use serde_json::json;
use std::path::PathBuf;
use toml_edit::{value, DocumentMut, Item, Table};
use crate::clients::ExtraSpec;

pub struct CodexAdapter;

const PROVIDER_ID: &str = "ai-gateway";
const TOKEN_FIELD: &str = "experimental_bearer_token";

fn load(path: &std::path::Path) -> AppResult<(DocumentMut, Option<Vec<u8>>)> {
    let original = read_bytes(path)?;
    let text = match &original {
        Some(bytes) => std::str::from_utf8(bytes)
            .map_err(|_| AppError::BadRequest(format!("Config is not valid UTF-8: {}", path.display())))?,
        None => "",
    };
    let doc = text.parse::<DocumentMut>().map_err(|error| {
        AppError::BadRequest(format!("Cannot parse {}: {error}", path.display()))
    })?;
    Ok((doc, original))
}

fn provider_table<'a>(doc: &'a DocumentMut) -> Option<&'a Table> {
    doc.get("model_providers")
        .and_then(Item::as_table)
        .and_then(|providers| providers.get(PROVIDER_ID))
        .and_then(Item::as_table)
}

fn field(table: Option<&Table>, key: &str) -> Option<String> {
    table
        .and_then(|table| table.get(key))
        .and_then(Item::as_str)
        .map(str::to_string)
}

fn bool_field(table: Option<&Table>, key: &str) -> Option<bool> {
    table.and_then(|table| table.get(key)).and_then(Item::as_bool)
}

/// 顶层附加键的类型由字段表决定：布尔开关必须写成真布尔，否则 Codex 不认。
fn typed_top(key: &str, setting: &str) -> Value {
    if key == "disable_response_storage" {
        json!(setting == "1" || setting == "true")
    } else {
        json!(setting)
    }
}

fn json_of_item(item: Option<&Item>) -> Option<Value> {
    match item {
        Some(Item::Value(toml_edit::Value::String(text))) => Some(json!(text.value().clone())),
        Some(Item::Value(value)) => {
            let text = value.to_string();
            let text = text.trim();
            if text == "true" || text == "false" {
                Some(json!(text == "true"))
            } else if let Ok(number) = text.parse::<i64>() {
                Some(json!(number))
            } else {
                Some(json!(text.trim_matches('"').to_string()))
            }
        }
        _ => None,
    }
}

fn json_to_toml(value: &Value) -> toml_edit::Value {
    match value {
        Value::Bool(flag) => toml_edit::Value::from(*flag),
        Value::String(text) => toml_edit::Value::from(text.clone()),
        Value::Number(number) => toml_edit::Value::from(number.as_i64().unwrap_or(0)),
        other => toml_edit::Value::from(other.to_string()),
    }
}

impl Adapter for CodexAdapter {
    fn id(&self) -> ClientId {
        ClientId::Codex
    }

    fn path(&self, paths: &ClientPaths) -> PathBuf {
        paths.codex_config.clone()
    }

    fn status(&self, paths: &ClientPaths, managed: Option<&Value>) -> AppResult<ClientStatus> {
        let path = self.path(paths);
        let Some(bytes) = read_bytes(&path)? else {
            return Ok(ClientStatus {
                id: self.id().as_str().into(),
                name: self.id().display_name().into(),
                path: path.display().to_string(),
                exists: false,
                detected: false,
                managed: false,
                backup_path: None,
                current: ClientCurrent::default(),
                error: None,
                warnings: vec![],
            });
        };
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| AppError::BadRequest(format!("Config is not valid UTF-8: {}", path.display())))?;
        let doc = text.parse::<DocumentMut>().map_err(|error| {
            AppError::BadRequest(format!("Cannot parse {}: {error}", path.display()))
        })?;
        let table = provider_table(&doc);
        let current = ClientCurrent {
            base_url: field(table, "base_url"),
            model: doc.get("model").and_then(Item::as_str).map(str::to_string),
            key_present: field(table, TOKEN_FIELD).is_some_and(|token| !token.trim().is_empty()),
            provider: doc.get("model_provider").and_then(Item::as_str).map(str::to_string),
        };
        let mut warnings = vec![];
        if table.is_some() && managed.is_none() {
            warnings.push(format!(
                "[model_providers.{PROVIDER_ID}] already exists and is not managed by AI Gateway."
            ));
        }
        Ok(ClientStatus {
            id: self.id().as_str().into(),
            name: self.id().display_name().into(),
            path: path.display().to_string(),
            exists: true,
            detected: true,
            managed: false,
            backup_path: None,
            current,
            error: None,
            warnings,
        })
    }

    fn current_token(&self, paths: &ClientPaths) -> AppResult<Option<String>> {
        let path = self.path(paths);
        let Some(bytes) = read_bytes(&path)? else {
            return Ok(None);
        };
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| AppError::BadRequest(format!("Config is not valid UTF-8: {}", path.display())))?;
        let doc = text.parse::<DocumentMut>().map_err(|error| {
            AppError::BadRequest(format!("Cannot parse {}: {error}", path.display()))
        })?;
        Ok(field(provider_table(&doc), TOKEN_FIELD).filter(|token| !token.trim().is_empty()))
    }

    /// 顶层键，不是 provider 表里的字段——写错层 Codex 会直接忽略。
    fn extra_specs(&self) -> Vec<ExtraSpec> {
        vec![
            ExtraSpec::choice(
                "model_reasoning_effort",
                "clientCodexReasoning",
                "model_reasoning_effort",
                &["minimal", "low", "medium", "high"],
            ),
            ExtraSpec::flag(
                "disable_response_storage",
                "clientCodexNoStorage",
                "disable_response_storage",
            ),
        ]
    }

    fn read_extras(
        &self,
        paths: &ClientPaths,
    ) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let Ok(Some(text)) = read_text(&self.path(paths)) else {
            return (BTreeMap::new(), BTreeMap::new());
        };
        let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
            return (BTreeMap::new(), BTreeMap::new());
        };
        let read = |key: &str| {
            doc.get(key).and_then(|item| match item {
                toml_edit::Item::Value(toml_edit::Value::String(text)) => Some(text.value().clone()),
                toml_edit::Item::Value(other) => Some(other.to_string().trim().to_string()),
                _ => None,
            })
        };
        let extras = ["model_reasoning_effort", "disable_response_storage"]
            .iter()
            .filter_map(|key| {
                read(key)
                    .map(|value| (key.to_string(), value))
                    .filter(|(_, value)| !value.trim().is_empty() && value != "None")
            })
            .collect();
        (extras, BTreeMap::new())
    }

    fn plan(
        &self,
        paths: &ClientPaths,
        models: &ResolvedModels,
        base_url: &str,
        token: &str,
        allow_existing: bool,
        extras: &ClientExtras,
    ) -> AppResult<ClientPlan> {
        let path = self.path(paths);
        let (mut doc, original) = load(&path)?;
        let existing_table = provider_table(&doc).cloned();
        if existing_table.is_some() && !allow_existing {
            return Err(AppError::Conflict(format!(
                "Codex already has a [model_providers.{PROVIDER_ID}] table; remove or rename it before applying AI Gateway."
            )));
        }

        let before_model = doc.get("model").and_then(Item::as_str).map(str::to_string);
        let before_provider = doc.get("model_provider").and_then(Item::as_str).map(str::to_string);
        let before_token = field(existing_table.as_ref(), TOKEN_FIELD).filter(|token| !token.trim().is_empty());
        let openai_base = openai_base(base_url);

        doc.insert("model_provider", value(PROVIDER_ID));
        doc.insert("model", value(models.default.as_str()));
        if !doc.contains_key("model_providers") {
            doc.insert("model_providers", Item::Table(Table::new()));
        }
        let providers = doc
            .get_mut("model_providers")
            .and_then(Item::as_table_mut)
            .ok_or_else(|| AppError::BadRequest("model_providers must be a TOML table".to_string()))?;
        let mut table = Table::new();
        table.insert("name", value("AI Gateway"));
        table.insert("base_url", value(openai_base.as_str()));
        table.insert("wire_api", value("chat"));
        table.insert(TOKEN_FIELD, value(token));
        table.insert("requires_openai_auth", value(false));
        providers.insert(PROVIDER_ID, Item::Table(table));
        // 先记下原值再写：写完才读会把"我们刚写的值"当成用户原件，撤销就回不去
        let mut before_extras = std::collections::BTreeMap::new();
        for (key, setting) in &extras.env {
            before_extras.insert(key.clone(), json_of_item(doc.get(key.as_str())));
            doc.insert(key.as_str(), value(json_to_toml(&typed_top(key, setting))));
        }

        let mut changes = vec![
            change("model", before_model.clone().map(Value::String), json!(models.default), false),
            change(
                "model_provider",
                before_provider.clone().map(Value::String),
                json!(PROVIDER_ID),
                false,
            ),
        ];
        for (key, before, after, secret) in [
            ("name", field(existing_table.as_ref(), "name"), "AI Gateway".to_string(), false),
            ("base_url", field(existing_table.as_ref(), "base_url"), openai_base.clone(), false),
            ("wire_api", field(existing_table.as_ref(), "wire_api"), "chat".to_string(), false),
            (TOKEN_FIELD, None, String::new(), true),
            (
                "requires_openai_auth",
                bool_field(existing_table.as_ref(), "requires_openai_auth")
                    .map(|value| value.to_string()),
                "false".to_string(),
                false,
            ),
        ] {
            changes.push(change(
                format!("model_providers.{PROVIDER_ID}.{key}"),
                before.map(Value::String),
                json!(after),
                secret,
            ));
        }

        let mut top_extra = serde_json::Map::new();
        for (key, setting) in &extras.env {
            let before = before_extras.get(key).cloned().flatten();
            let after = typed_top(key, setting);
            changes.push(change(
                key.clone(),
                before.clone(),
                after.clone(),
                false,
            ));
            top_extra.insert(
                key.clone(),
                json!({ "before": before.unwrap_or(Value::Null), "after": after }),
            );
        }

        let mut warnings = vec![
            "Codex is configured with wire_api = \"chat\"; Responses-only features may be unavailable."
                .to_string(),
            "This requires a Codex version that reads experimental_bearer_token for custom providers."
                .to_string(),
            "~/.codex/auth.json was left untouched; requires_openai_auth is false so official login is not sent."
                .to_string(),
        ];
        if existing_table.is_some() {
            warnings.push(format!(
                "Existing [model_providers.{PROVIDER_ID}] was replaced; the previous file is in the backup."
            ));
        }
        if before_token.as_deref().is_some_and(|previous| previous != token) {
            warnings.push(
                "An existing AI Gateway token was replaced; the previous value is only in the backup."
                    .to_string(),
            );
        }

        let managed = json!({
            "created_file": original.is_none(),
            "provider_id": PROVIDER_ID,
            "top": {
                "model": {"before": before_model, "after": models.default},
                "model_provider": {"before": before_provider, "after": PROVIDER_ID},
                "extra": top_extra
            },
            "provider": {
                "name": "AI Gateway",
                "base_url": openai_base,
                "wire_api": "chat",
                "requires_openai_auth": false
            },
            "token_path": format!("model_providers.{PROVIDER_ID}.{TOKEN_FIELD}"),
            "token_before_present": before_token.is_some()
        });

        Ok(ClientPlan {
            path,
            original,
            content: doc.to_string().into_bytes(),
            changes,
            managed,
            warnings,
        })
    }

    fn restore(&self, paths: &ClientPaths, managed: &Value) -> AppResult<RestorePlan> {
        let path = self.path(paths);
        let (mut doc, original) = load(&path)?;
        if original.is_none() {
            return Ok(RestorePlan {
                path,
                original,
                content: vec![],
                restored: vec![],
                kept: vec![],
                warnings: vec!["The config file no longer exists; nothing to restore.".to_string()],
                remove_file: false,
                changed: false,
            });
        }

        let token_path = managed.get("token_path").and_then(Value::as_str);
        let token_hash = managed.get("token_hash").and_then(Value::as_str);
        let mut restored = Vec::new();
        let mut kept = Vec::new();
        let mut warnings = Vec::new();

        if let Some(top) = managed.get("top").and_then(Value::as_object) {
            let mut entries: Vec<(String, Value)> = ["model", "model_provider"]
                .iter()
                .filter_map(|key| top.get(*key).map(|entry| ((*key).to_string(), entry.clone())))
                .collect();
            // 附加顶层键是后加的，键名不固定，只能照着记录本身回滚
            if let Some(extra) = top.get("extra").and_then(Value::as_object) {
                entries.extend(extra.iter().map(|(key, entry)| (key.clone(), entry.clone())));
            }
            for (key, entry) in entries {
                let after = entry.get("after");
                let current = json_of_item(doc.get(key.as_str()));
                let untouched = match after {
                    None => continue,
                    Some(after) => current == Some(after.clone()) || (current.is_none() && after.is_null()),
                };
                if !untouched {
                    if current.is_some() {
                        kept.push(KeptChange {
                            path: key.clone(),
                            reason: "Value changed after this client was applied.".to_string(),
                        });
                    }
                    continue;
                }
                match entry.get("before") {
                    Some(before) if !before.is_null() => {
                        doc.insert(key.as_str(), value(json_to_toml(before)));
                    }
                    _ => {
                        doc.remove(key.as_str());
                    }
                }
                restored.push(key);
            }
        }

        let provider_matches = managed
            .get("provider")
            .and_then(Value::as_object)
            .is_some_and(|expected| {
                let table = provider_table(&doc);
                let current_token = field(table, TOKEN_FIELD);
                let token_matches = match (&current_token, token_hash) {
                    (Some(token), Some(hash)) => sha256_hex(token.trim()) == hash,
                    (None, Some(_)) => false,
                    (None, None) => true,
                    (Some(_), None) => true,
                };
                token_matches
                    && field(table, "name") == expected.get("name").and_then(Value::as_str).map(str::to_string)
                    && field(table, "base_url") == expected.get("base_url").and_then(Value::as_str).map(str::to_string)
                    && field(table, "wire_api") == expected.get("wire_api").and_then(Value::as_str).map(str::to_string)
                    && bool_field(table, "requires_openai_auth")
                        == expected.get("requires_openai_auth").and_then(Value::as_bool)
            });
        if provider_matches {
            if let Some(providers) = doc.get_mut("model_providers").and_then(Item::as_table_mut) {
                providers.remove(PROVIDER_ID);
                restored.push(format!("model_providers.{PROVIDER_ID}"));
            }
        } else if provider_table(&doc).is_some() {
            kept.push(KeptChange {
                path: format!("model_providers.{PROVIDER_ID}"),
                reason: "Provider table was changed or its token was rotated after apply.".to_string(),
            });
            warnings.push(
                "The AI Gateway provider table was left in place; use the pre-apply backup for a full rollback."
                    .to_string(),
            );
        }
        if token_path.is_some() && token_hash.is_none() {
            warnings.push("Managed token hash is missing; token removal was skipped.".to_string());
        }

        let created_file = managed
            .get("created_file")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let remove_file = created_file && doc.as_table().is_empty();
        let content = if remove_file { vec![] } else { doc.to_string().into_bytes() };
        let changed = remove_file || original.as_deref() != Some(content.as_slice());

        Ok(RestorePlan {
            path,
            original,
            content,
            restored,
            kept,
            warnings,
            remove_file,
            changed,
        })
    }
}
