//! Pi：只在 `~/.pi/agent/models.json` 的 `providers` 下累加一条供应商。
//! Pi 的登录与当前模型在 `settings.json`，这里刻意不碰——由用户在 Pi 里选择。

use super::*;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub struct PiAdapter;

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
    root.get("providers").and_then(Value::as_object)
}

fn entry<'a>(root: &'a Value) -> Option<&'a Map<String, Value>> {
    providers(root)?.get(PROVIDER_ID)?.as_object()
}

fn token_of(entry: Option<&Map<String, Value>>) -> Option<String> {
    entry
        .and_then(|entry| entry.get("apiKey"))
        .and_then(Value::as_str)
        .filter(|token| !token.trim().is_empty())
        .map(str::to_string)
}

fn non_secret_matches(current: Option<&Map<String, Value>>, expected: &Map<String, Value>) -> bool {
    let Some(current) = current else { return false };
    current.get("name") == expected.get("name")
        && current.get("baseUrl") == expected.get("baseUrl")
        && current.get("api") == expected.get("api")
        && current.get("models") == expected.get("models")
}

impl Adapter for PiAdapter {
    fn id(&self) -> ClientId {
        ClientId::Pi
    }

    fn path(&self, paths: &ClientPaths) -> PathBuf {
        paths.pi_models.clone()
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
        let model = provider
            .and_then(|entry| entry.get("models"))
            .and_then(Value::as_array)
            .and_then(|models| models.first())
            .and_then(|model| model.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut warnings = vec![
            "Pi's active model is stored in settings.json and is not modified; select the AI Gateway model inside Pi."
                .to_string(),
        ];
        if provider.is_some() && managed.is_none() {
            warnings.push(format!("providers.{PROVIDER_ID} already exists and is not managed by AI Gateway."));
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
                    .and_then(|entry| entry.get("baseUrl"))
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
                "Pi already has providers.{PROVIDER_ID}; remove or rename it before applying AI Gateway."
            )));
        }
        let before_token = token_of(before.as_ref());
        let openai_base = openai_base(base_url);
        let provider = json!({
            "name": "AI Gateway",
            "baseUrl": openai_base,
            "api": "openai-completions",
            "apiKey": token,
            "models": [{ "id": models.default }]
        });

        let root_obj = root.as_object_mut().expect("root checked by load");
        let providers_obj = root_obj
            .entry("providers".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        providers_obj
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("providers must be an object".to_string()))?
            .insert(PROVIDER_ID.to_string(), provider.clone());

        let changes = vec![
            change(
                format!("providers.{PROVIDER_ID}.name"),
                before.as_ref().and_then(|entry| entry.get("name")).cloned(),
                json!("AI Gateway"),
                false,
            ),
            change(
                format!("providers.{PROVIDER_ID}.baseUrl"),
                before.as_ref().and_then(|entry| entry.get("baseUrl")).cloned(),
                json!(openai_base),
                false,
            ),
            change(
                format!("providers.{PROVIDER_ID}.api"),
                before.as_ref().and_then(|entry| entry.get("api")).cloned(),
                json!("openai-completions"),
                false,
            ),
            change(
                format!("providers.{PROVIDER_ID}.apiKey"),
                None,
                Value::Null,
                true,
            ),
            change(
                format!("providers.{PROVIDER_ID}.models"),
                before.as_ref().and_then(|entry| entry.get("models")).cloned(),
                provider.get("models").cloned().unwrap_or(Value::Null),
                false,
            ),
        ];

        let mut warnings = vec![
            "Pi JSON5 comments/formatting will be normalized on write; the original file is in the backup."
                .to_string(),
            "Select the AI Gateway model inside Pi. Do not configure a default model here; Pi owns settings.json."
                .to_string(),
        ];
        if before.is_some() {
            warnings.push(format!(
                "Existing providers.{PROVIDER_ID} was replaced; the previous file is in the backup."
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
            "provider_id": PROVIDER_ID,
            "provider": {
                "name": "AI Gateway",
                "baseUrl": openai_base,
                "api": "openai-completions",
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
            .ok_or_else(|| AppError::BadRequest("Managed Pi state is invalid".to_string()))?;
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
                if let Some(providers) = root.get_mut("providers").and_then(Value::as_object_mut) {
                    providers.remove(PROVIDER_ID);
                    restored.push(format!("providers.{PROVIDER_ID}"));
                }
            } else {
                kept.push(KeptChange {
                    path: format!("providers.{PROVIDER_ID}"),
                    reason: "Provider entry was changed or its token was rotated after apply.".to_string(),
                });
                warnings.push(
                    "The AI Gateway provider entry was left in place; use the pre-apply backup for a full rollback."
                        .to_string(),
                );
            }
        }
        if root
            .get("providers")
            .and_then(Value::as_object)
            .is_some_and(Map::is_empty)
        {
            root.as_object_mut().expect("root").remove("providers");
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
