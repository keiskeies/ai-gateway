//! OpenCode：在 `provider.ai-gateway` 下累加一条 OpenAI 兼容供应商。
//! 支持 `.jsonc` 读取（注释/尾逗号），写回会规范化为 JSON；备份保留原文件。

use super::*;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub struct OpenCodeAdapter;

const PROVIDER_ID: &str = "ai-gateway";

fn load(path: &std::path::Path) -> AppResult<(Value, Option<Vec<u8>>)> {
    let original = read_bytes(path)?;
    let root = match &original {
        Some(bytes) => {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| AppError::BadRequest(format!("Config is not valid UTF-8: {}", path.display())))?;
            json5::from_str::<Value>(text).map_err(|error| {
                AppError::BadRequest(format!("Cannot parse {}: {error}", path.display()))
            })?
        }
        None => json!({}),
    };
    if !root.is_object() {
        return Err(AppError::BadRequest(format!(
            "{} must contain a JSON object",
            path.display()
        )));
    }
    Ok((root, original))
}

fn providers(root: &Value) -> Option<&Map<String, Value>> {
    root.get("provider").and_then(Value::as_object)
}

fn entry<'a>(root: &'a Value) -> Option<&'a Map<String, Value>> {
    providers(root)?.get(PROVIDER_ID)?.as_object()
}

fn token_of(entry: Option<&Map<String, Value>>) -> Option<String> {
    entry
        .and_then(|entry| entry.get("options"))
        .and_then(Value::as_object)
        .and_then(|options| options.get("apiKey"))
        .and_then(Value::as_str)
        .filter(|token| !token.trim().is_empty())
        .map(str::to_string)
}

fn non_secret_matches(current: Option<&Map<String, Value>>, expected: &Map<String, Value>) -> bool {
    let Some(current) = current else { return false };
    current.get("npm") == expected.get("npm")
        && current.get("name") == expected.get("name")
        && current
            .get("options")
            .and_then(Value::as_object)
            .and_then(|options| options.get("baseURL"))
            == expected
                .get("options")
                .and_then(Value::as_object)
                .and_then(|options| options.get("baseURL"))
        && current.get("models") == expected.get("models")
}

impl Adapter for OpenCodeAdapter {
    fn id(&self) -> ClientId {
        ClientId::Opencode
    }

    fn path(&self, paths: &ClientPaths) -> PathBuf {
        paths.opencode_config.clone()
    }

    fn status(&self, paths: &ClientPaths, managed: Option<&Value>) -> AppResult<ClientStatus> {
        let path = self.path(paths);
        let Some(_) = read_bytes(&path)? else {
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
        let (root, _) = load(&path)?;
        let provider = entry(&root);
        let model = provider
            .and_then(|entry| entry.get("models"))
            .and_then(Value::as_object)
            .and_then(|models| models.keys().next().cloned())
            .or_else(|| root.get("model").and_then(Value::as_str).map(str::to_string));
        let mut warnings = vec![];
        if root.get("providers").is_some() {
            warnings.push(
                "OpenCode's newer `providers` block is present; AI Gateway writes the `provider` block."
                    .to_string(),
            );
        }
        if provider.is_some() && managed.is_none() {
            warnings.push(format!("provider.{PROVIDER_ID} already exists and is not managed by AI Gateway."));
        }
        Ok(ClientStatus {
            id: self.id().as_str().into(),
            name: self.id().display_name().into(),
            path: path.display().to_string(),
            exists: true,
            detected: true,
            managed: false,
            backup_path: None,
            current: ClientCurrent {
                base_url: provider
                    .and_then(|entry| entry.get("options"))
                    .and_then(Value::as_object)
                    .and_then(|options| options.get("baseURL"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                model,
                key_present: token_of(provider).is_some(),
                provider: provider.map(|_| PROVIDER_ID.to_string()),
            },
            error: None,
            warnings,
        })
    }

    fn current_token(&self, paths: &ClientPaths) -> AppResult<Option<String>> {
        let path = self.path(paths);
        if read_bytes(&path)?.is_none() {
            return Ok(None);
        }
        let (root, _) = load(&path)?;
        Ok(token_of(entry(&root)))
    }

    fn extra_specs(&self) -> Vec<ExtraSpec> {
        vec![ExtraSpec::number(
            "timeout",
            "clientEnvTimeout",
            &format!("provider.{PROVIDER_ID}.settings.timeout"),
        )]
    }

    fn read_extras(
        &self,
        paths: &ClientPaths,
    ) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let empty = (BTreeMap::new(), BTreeMap::new());
        let Some(text) = read_text(&self.path(paths)).expect("read") else {
            return empty;
        };
        let Ok(root) = json5::from_str::<Value>(&text) else {
            return empty;
        };
        let Some(entry) = root.get("provider").and_then(|p| p.get(PROVIDER_ID)) else {
            return empty;
        };
        let out = entry
            .get("settings")
            .and_then(Value::as_object)
            .map(|settings| {
                settings
                    .iter()
                    .filter_map(|(key, value)| {
                        extra_text_from(value).map(|text| (key.clone(), text))
                    })
                    .collect()
            })
            .unwrap_or_default();
        (out, BTreeMap::new())
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
        let (mut root, original) = load(&path)?;
        let before = entry(&root).cloned();
        if before.is_some() && !allow_existing {
            return Err(AppError::Conflict(format!(
                "OpenCode already has provider.{PROVIDER_ID}; remove or rename it before applying AI Gateway."
            )));
        }
        let before_token = token_of(before.as_ref());
        let openai_base = openai_base(base_url);
        let mut model_map = Map::new();
        model_map.insert(models.default.clone(), json!({ "name": models.default }));
        let mut provider = json!({
            "npm": "@ai-sdk/openai-compatible",
            "name": "AI Gateway",
            "options": { "baseURL": openai_base, "apiKey": token },
            "models": Value::Object(model_map)
        });
        // OpenCode 的超时在 provider 的 settings 里，数字或 false
        let mut extra_rows = Vec::new();
        let specs = self.extra_specs();
        for (key, value) in &extras.env {
            let Some(spec) = specs.iter().find(|spec| spec.key == *key) else {
                continue;
            };
            let typed = typed_extra_value(&spec.kind, value);
            provider
                .as_object_mut()
                .expect("provider object")
                .entry("settings".to_string())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .expect("settings object")
                .insert(key.clone(), typed.clone());
            let previous = before
                .as_ref()
                .and_then(|entry| entry.get("settings"))
                .and_then(|settings| settings.get(key))
                .cloned();
            extra_rows.push((spec.path.clone(), previous, typed));
        }

        let root_obj = root.as_object_mut().expect("root checked by load");
        let providers_obj = root_obj
            .entry("provider".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let providers_obj = providers_obj
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("provider must be an object".to_string()))?;
        providers_obj.insert(PROVIDER_ID.to_string(), provider.clone());

        let mut changes = vec![
            change(
                format!("provider.{PROVIDER_ID}.npm"),
                before.as_ref().and_then(|entry| entry.get("npm")).cloned(),
                json!("@ai-sdk/openai-compatible"),
                false,
            ),
            change(
                format!("provider.{PROVIDER_ID}.name"),
                before.as_ref().and_then(|entry| entry.get("name")).cloned(),
                json!("AI Gateway"),
                false,
            ),
            change(
                format!("provider.{PROVIDER_ID}.options.baseURL"),
                before
                    .as_ref()
                    .and_then(|entry| entry.get("options"))
                    .and_then(Value::as_object)
                    .and_then(|options| options.get("baseURL"))
                    .cloned(),
                json!(openai_base),
                false,
            ),
            change(
                format!("provider.{PROVIDER_ID}.options.apiKey"),
                None,
                Value::Null,
                true,
            ),
            change(
                format!("provider.{PROVIDER_ID}.models"),
                before.as_ref().and_then(|entry| entry.get("models")).cloned(),
                provider.get("models").cloned().unwrap_or(Value::Null),
                false,
            ),
        ];

        let mut warnings = vec![
            "OpenCode JSONC comments/formatting will be normalized on write; the original file is in the backup."
                .to_string(),
        ];
        if root.get("providers").is_some() {
            warnings.push(
                "OpenCode's newer `providers` block is present; AI Gateway writes the `provider` block."
                    .to_string(),
            );
        }
        if before_token.as_deref().is_some_and(|previous| previous != token) {
            warnings.push(
                "An existing provider.ai-gateway API key was replaced; the previous value is only in the backup."
                    .to_string(),
            );
        }

        for (path, previous, typed) in extra_rows {
            changes.push(change(path, previous, typed, false));
        }

        let managed = json!({
            "created_file": original.is_none(),
            "provider_id": PROVIDER_ID,
            "provider": {
                "npm": "@ai-sdk/openai-compatible",
                "name": "AI Gateway",
                "options": { "baseURL": openai_base },
                "models": provider.get("models").cloned().unwrap_or(Value::Null)
            },
            "token_before_present": before_token.is_some()
        });

        Ok(ClientPlan {
            path,
            original,
            content: json_pretty(&root)?,
            changes,
            managed,
            warnings,
        })
    }

    fn restore(&self, paths: &ClientPaths, managed: &Value) -> AppResult<RestorePlan> {
        let path = self.path(paths);
        let (mut root, original) = load(&path)?;
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
        let expected = managed
            .get("provider")
            .and_then(Value::as_object)
            .ok_or_else(|| AppError::BadRequest("Managed OpenCode state is invalid".to_string()))?;
        let token_hash = managed.get("token_hash").and_then(Value::as_str);
        let mut restored = Vec::new();
        let mut kept = Vec::new();
        let mut warnings = vec![];
        let current = entry(&root).cloned();
        let token_matches = match (&current, token_hash) {
            (Some(current), Some(hash)) => token_of(Some(current)).is_some_and(|token| sha256_hex(token.trim()) == hash),
            (Some(current), None) => token_of(Some(current)).is_none(),
            (None, _) => true,
        };
        if let Some(current) = current {
            if non_secret_matches(Some(&current), expected) && token_matches {
                if let Some(providers) = root.get_mut("provider").and_then(Value::as_object_mut) {
                    providers.remove(PROVIDER_ID);
                    restored.push(format!("provider.{PROVIDER_ID}"));
                }
            } else {
                kept.push(KeptChange {
                    path: format!("provider.{PROVIDER_ID}"),
                    reason: "Provider entry was changed or its token was rotated after apply.".to_string(),
                });
                warnings.push(
                    "The AI Gateway provider entry was left in place; use the pre-apply backup for a full rollback."
                        .to_string(),
                );
            }
        }
        if root
            .get("provider")
            .and_then(Value::as_object)
            .is_some_and(Map::is_empty)
        {
            root.as_object_mut().expect("root object").remove("provider");
        }
        let created_file = managed
            .get("created_file")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let remove_file = created_file && root.as_object().is_some_and(Map::is_empty);
        let content = if remove_file { vec![] } else { json_pretty(&root)? };
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
