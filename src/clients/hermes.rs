//! Hermes：`~/.hermes/config.yaml`（Windows 为 `%LOCALAPPDATA%\hermes\config.yaml`）。
//! 在 `custom_providers` 列表里累加一条 `ai-gateway`，并把 `model` 段指向它。
//! YAML 注释/格式会在写回时规范化；原文件先备份。

use super::*;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub struct HermesAdapter;

const PROVIDER_ID: &str = "ai-gateway";

fn load(path: &std::path::Path) -> AppResult<(Value, Option<Vec<u8>>)> {
    let original = read_bytes(path)?;
    let mut root = match &original {
        Some(bytes) => {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| AppError::BadRequest(format!("Config is not valid UTF-8: {}", path.display())))?;
            if text.trim().is_empty() {
                json!({})
            } else {
                serde_yaml::from_str::<Value>(text).map_err(|error| {
                    AppError::BadRequest(format!("Cannot parse {}: {error}", path.display()))
                })?
            }
        }
        None => json!({}),
    };
    if root.is_null() {
        root = json!({});
    }
    if !root.is_object() {
        return Err(AppError::BadRequest(format!(
            "{} must contain a YAML mapping",
            path.display()
        )));
    }
    Ok((root, original))
}

fn providers(root: &Value) -> Option<&Vec<Value>> {
    root.get("custom_providers").and_then(Value::as_array)
}

fn entry<'a>(root: &'a Value) -> Option<&'a Map<String, Value>> {
    providers(root)?
        .iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some(PROVIDER_ID))?
        .as_object()
}

fn token_of(entry: Option<&Map<String, Value>>) -> Option<String> {
    entry
        .and_then(|entry| entry.get("api_key"))
        .and_then(Value::as_str)
        .filter(|token| !token.trim().is_empty())
        .map(str::to_string)
}

fn model_section(root: &Value) -> Option<&Map<String, Value>> {
    root.get("model").and_then(Value::as_object)
}

fn non_secret_matches(current: Option<&Map<String, Value>>, expected: &Map<String, Value>) -> bool {
    let Some(current) = current else { return false };
    current.get("name") == expected.get("name")
        && current.get("base_url") == expected.get("base_url")
        && current.get("model") == expected.get("model")
        && current.get("models") == expected.get("models")
}

fn yaml_bytes(value: &Value) -> AppResult<Vec<u8>> {
    serde_yaml::to_string(value)
        .map(|text| text.into_bytes())
        .map_err(|error| AppError::Internal(format!("Cannot serialize YAML: {error}")))
}

impl Adapter for HermesAdapter {
    fn id(&self) -> ClientId {
        ClientId::Hermes
    }

    fn path(&self, paths: &ClientPaths) -> PathBuf {
        paths.hermes_config.clone()
    }

    fn status(&self, paths: &ClientPaths, managed: Option<&Value>) -> AppResult<ClientStatus> {
        let path = self.path(paths);
        if read_bytes(&path)?.is_none() {
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
        }
        let (root, _) = load(&path)?;
        let provider = entry(&root);
        let model = model_section(&root);
        let mut warnings = vec![];
        if provider.is_some() && managed.is_none() {
            warnings.push(format!("custom_providers.{PROVIDER_ID} already exists and is not managed by AI Gateway."));
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
                base_url: model
                    .and_then(|model| model.get("base_url"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| {
                        provider
                            .and_then(|entry| entry.get("base_url"))
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    }),
                model: model
                    .and_then(|model| model.get("default"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| {
                        provider
                            .and_then(|entry| entry.get("model"))
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    }),
                key_present: token_of(provider).is_some(),
                provider: model
                    .and_then(|model| model.get("provider"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| provider.map(|_| PROVIDER_ID.to_string())),
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

    fn plan(
        &self,
        paths: &ClientPaths,
        models: &ResolvedModels,
        base_url: &str,
        token: &str,
        allow_existing: bool,
    ) -> AppResult<ClientPlan> {
        let path = self.path(paths);
        let (mut root, original) = load(&path)?;
        let before = entry(&root).cloned();
        if before.is_some() && !allow_existing {
            return Err(AppError::Conflict(format!(
                "Hermes already has custom_providers.{PROVIDER_ID}; remove or rename it before applying AI Gateway."
            )));
        }
        let before_token = token_of(before.as_ref());
        let before_model = model_section(&root).cloned();
        let openai_base = openai_base(base_url);
        let mut model_map = Map::new();
        model_map.insert(models.default.clone(), json!({}));
        let provider = json!({
            "name": PROVIDER_ID,
            "base_url": openai_base,
            "api_key": token,
            "model": models.default,
            "models": Value::Object(model_map)
        });

        let root_obj = root.as_object_mut().expect("root checked by load");
        let list = root_obj
            .entry("custom_providers".to_string())
            .or_insert_with(|| Value::Array(vec![]));
        let list = list
            .as_array_mut()
            .ok_or_else(|| AppError::BadRequest("custom_providers must be a list".to_string()))?;
        list.retain(|entry| entry.get("name").and_then(Value::as_str) != Some(PROVIDER_ID));
        list.push(provider.clone());

        let model = root_obj
            .entry("model".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let model = model
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("model must be a mapping".to_string()))?;
        let before_default = before_model
            .as_ref()
            .and_then(|model| model.get("default"))
            .cloned();
        let before_provider = before_model
            .as_ref()
            .and_then(|model| model.get("provider"))
            .cloned();
        let before_base = before_model
            .as_ref()
            .and_then(|model| model.get("base_url"))
            .cloned();
        model.insert("default".to_string(), json!(models.default));
        model.insert("provider".to_string(), json!(PROVIDER_ID));
        model.insert("base_url".to_string(), json!(openai_base));

        let changes = vec![
            change(
                "custom_providers.ai-gateway.base_url",
                before.as_ref().and_then(|entry| entry.get("base_url")).cloned(),
                json!(openai_base),
                false,
            ),
            change("custom_providers.ai-gateway.api_key", None, Value::Null, true),
            change(
                "custom_providers.ai-gateway.model",
                before.as_ref().and_then(|entry| entry.get("model")).cloned(),
                json!(models.default),
                false,
            ),
            change(
                "model.default",
                before_default.clone(),
                json!(models.default),
                false,
            ),
            change(
                "model.provider",
                before_provider.clone(),
                json!(PROVIDER_ID),
                false,
            ),
            change(
                "model.base_url",
                before_base.clone(),
                json!(openai_base),
                false,
            ),
        ];

        let mut warnings = vec![
            "Hermes YAML comments/formatting will be normalized on write; the original file is in the backup."
                .to_string(),
        ];
        if before.is_some() {
            warnings.push(format!(
                "Existing custom_providers.{PROVIDER_ID} was replaced; the previous file is in the backup."
            ));
        }
        if before_token.as_deref().is_some_and(|previous| previous != token) {
            warnings.push(
                "An existing AI Gateway API key was replaced; the previous value is only in the backup."
                    .to_string(),
            );
        }

        let managed = json!({
            "created_file": original.is_none(),
            "created_model": before_model.is_none(),
            "provider_id": PROVIDER_ID,
            "provider": {
                "name": PROVIDER_ID,
                "base_url": openai_base,
                "model": models.default,
                "models": provider.get("models").cloned().unwrap_or(Value::Null)
            },
            "model": {
                "default": { "before": before_default, "after": models.default },
                "provider": { "before": before_provider, "after": PROVIDER_ID },
                "base_url": { "before": before_base, "after": openai_base }
            },
            "token_before_present": before_token.is_some()
        });

        Ok(ClientPlan {
            path,
            original,
            content: yaml_bytes(&root)?,
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
            .ok_or_else(|| AppError::BadRequest("Managed Hermes state is invalid".to_string()))?;
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
                if let Some(list) = root.get_mut("custom_providers").and_then(Value::as_array_mut) {
                    list.retain(|entry| entry.get("name").and_then(Value::as_str) != Some(PROVIDER_ID));
                    restored.push(format!("custom_providers.{PROVIDER_ID}"));
                }
            } else {
                kept.push(KeptChange {
                    path: format!("custom_providers.{PROVIDER_ID}"),
                    reason: "Provider entry was changed or its token was rotated after apply.".to_string(),
                });
                warnings.push(
                    "The AI Gateway provider entry was left in place; use the pre-apply backup for a full rollback."
                        .to_string(),
                );
            }
        }

        if let Some(model_state) = managed.get("model").and_then(Value::as_object) {
            if let Some(model) = root.get_mut("model").and_then(Value::as_object_mut) {
                for key in ["default", "provider", "base_url"] {
                    let Some(entry) = model_state.get(key) else { continue };
                    let after = entry.get("after");
                    if model.get(key) != after {
                        if model.get(key).is_some() {
                            kept.push(KeptChange {
                                path: format!("model.{key}"),
                                reason: "Value changed after this client was applied.".to_string(),
                            });
                        }
                        continue;
                    }
                    match entry.get("before") {
                        Some(Value::Null) | None => {
                            model.remove(key);
                        }
                        Some(before) => {
                            model.insert(key.to_string(), before.clone());
                        }
                    }
                    restored.push(format!("model.{key}"));
                }
            }
        }

        if root
            .get("custom_providers")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            root.as_object_mut().expect("root").remove("custom_providers");
        }
        let created_model = managed
            .get("created_model")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if created_model && root.get("model").and_then(Value::as_object).is_some_and(Map::is_empty) {
            root.as_object_mut().expect("root").remove("model");
        }
        let created_file = managed
            .get("created_file")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let remove_file = created_file && root.as_object().is_some_and(Map::is_empty);
        let content = if remove_file { vec![] } else { yaml_bytes(&root)? };
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
