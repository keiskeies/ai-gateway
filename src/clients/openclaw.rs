//! OpenClaw：在 `models.providers.ai-gateway` 累加 OpenAI 兼容供应商，
//! 并把 `agents.defaults.model.primary` 指向它。JSON5 读、JSON 写，原文件先备份。

use super::*;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub struct OpenClawAdapter;

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
    root.get("models")
        .and_then(Value::as_object)
        .and_then(|models| models.get("providers"))
        .and_then(Value::as_object)
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

fn primary(root: &Value) -> Option<String> {
    root.get("agents")
        .and_then(Value::as_object)
        .and_then(|agents| agents.get("defaults"))
        .and_then(Value::as_object)
        .and_then(|defaults| defaults.get("model"))
        .and_then(Value::as_object)
        .and_then(|model| model.get("primary"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn non_secret_matches(current: Option<&Map<String, Value>>, expected: &Map<String, Value>) -> bool {
    let Some(current) = current else { return false };
    current.get("baseUrl") == expected.get("baseUrl")
        && current.get("api") == expected.get("api")
        && current.get("models") == expected.get("models")
}

fn prune_empty(root: &mut Value) {
    if let Some(models) = root.get_mut("models").and_then(Value::as_object_mut) {
        if models
            .get("providers")
            .and_then(Value::as_object)
            .is_some_and(Map::is_empty)
        {
            models.remove("providers");
        }
        if models.is_empty() {
            root.as_object_mut().expect("root").remove("models");
        }
    }
    if let Some(agents) = root.get_mut("agents").and_then(Value::as_object_mut) {
        if let Some(defaults) = agents.get_mut("defaults").and_then(Value::as_object_mut) {
            if defaults
                .get("model")
                .and_then(Value::as_object)
                .is_some_and(Map::is_empty)
            {
                defaults.remove("model");
            }
            if defaults.is_empty() {
                agents.remove("defaults");
            }
        }
        if agents.is_empty() {
            root.as_object_mut().expect("root").remove("agents");
        }
    }
}

impl Adapter for OpenClawAdapter {
    fn id(&self) -> ClientId {
        ClientId::Openclaw
    }

    fn path(&self, paths: &ClientPaths) -> PathBuf {
        paths.openclaw_config.clone()
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
        let mut warnings = vec![];
        if provider.is_some() && managed.is_none() {
            warnings.push(format!("models.providers.{PROVIDER_ID} already exists and is not managed by AI Gateway."));
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
                model: primary(&root),
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
        let model = format!("models.providers.{PROVIDER_ID}.models[0]");
        vec![
            ExtraSpec::number(
                "contextWindow",
                "clientEnvMaxContext",
                &format!("{model}.contextWindow"),
            ),
            ExtraSpec::number("maxTokens", "clientEnvMaxOutput", &format!("{model}.maxTokens")),
            ExtraSpec::flag("reasoning", "clientReasoning", &format!("{model}.reasoning")),
        ]
    }

    fn read_extras(
        &self,
        paths: &ClientPaths,
    ) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let empty = (BTreeMap::new(), BTreeMap::new());
        let Ok(Some(text)) = read_text(&self.path(paths)) else {
            return empty;
        };
        let Ok(root) = json5::from_str::<Value>(&text) else {
            return empty;
        };
        let Some(model) = root
            .get("models")
            .and_then(|models| models.get("providers"))
            .and_then(|providers| providers.get(PROVIDER_ID))
            .and_then(|entry| entry.get("models"))
            .and_then(Value::as_array)
            .and_then(|list| list.first())
        else {
            return empty;
        };
        let out = model
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .filter(|(key, _)| {
                        matches!(key.as_str(), "contextWindow" | "maxTokens" | "reasoning")
                    })
                    .filter_map(|(key, value)| extra_text_from(value).map(|text| (key.clone(), text)))
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
                "OpenClaw already has models.providers.{PROVIDER_ID}; remove or rename it before applying AI Gateway."
            )));
        }
        let before_token = token_of(before.as_ref());
        let before_primary = primary(&root);
        let openai_base = openai_base(base_url);
        let primary_value = format!("{PROVIDER_ID}/{}", models.default);
        // OpenClaw 的窗口/输出上限与推理标记挂在模型条目上
        let mut provider = json!({
            "baseUrl": openai_base,
            "apiKey": token,
            "api": "openai-completions",
            "models": [{ "id": models.default, "name": models.default }]
        });
        let mut extra_rows = Vec::new();
        let specs = self.extra_specs();
        for (key, value) in &extras.env {
            let Some(spec) = specs.iter().find(|spec| spec.key == *key) else {
                continue;
            };
            let typed = typed_extra_value(&spec.kind, value);
            if let Some(model) = provider
                .get_mut("models")
                .and_then(Value::as_array_mut)
                .and_then(|list| list.first_mut())
            {
                model
                    .as_object_mut()
                    .expect("model object")
                    .insert(key.clone(), typed.clone());
            }
            let previous = before
                .as_ref()
                .and_then(|entry| entry.get("models"))
                .and_then(Value::as_array)
                .and_then(|list| list.first().cloned())
                .and_then(|model| model.get(key).cloned());
            extra_rows.push((spec.path.clone(), previous, typed));
        }

        let root_obj = root.as_object_mut().expect("root checked by load");
        let models_obj = root_obj
            .entry("models".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let models_obj = models_obj
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("models must be an object".to_string()))?;
        let providers_obj = models_obj
            .entry("providers".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        providers_obj
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("models.providers must be an object".to_string()))?
            .insert(PROVIDER_ID.to_string(), provider.clone());

        let agents = root_obj
            .entry("agents".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let agents = agents
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("agents must be an object".to_string()))?;
        let defaults = agents
            .entry("defaults".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let defaults = defaults
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("agents.defaults must be an object".to_string()))?;
        let model = defaults
            .entry("model".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        model
            .as_object_mut()
            .ok_or_else(|| AppError::BadRequest("agents.defaults.model must be an object".to_string()))?
            .insert("primary".to_string(), json!(primary_value));

        let mut changes = vec![
            change(
                format!("models.providers.{PROVIDER_ID}.baseUrl"),
                before.as_ref().and_then(|entry| entry.get("baseUrl")).cloned(),
                json!(openai_base),
                false,
            ),
            change(
                format!("models.providers.{PROVIDER_ID}.apiKey"),
                None,
                Value::Null,
                true,
            ),
            change(
                format!("models.providers.{PROVIDER_ID}.api"),
                before.as_ref().and_then(|entry| entry.get("api")).cloned(),
                json!("openai-completions"),
                false,
            ),
            change(
                format!("models.providers.{PROVIDER_ID}.models"),
                before.as_ref().and_then(|entry| entry.get("models")).cloned(),
                provider.get("models").cloned().unwrap_or(Value::Null),
                false,
            ),
            change(
                "agents.defaults.model.primary",
                before_primary.clone().map(Value::String),
                json!(primary_value),
                false,
            ),
        ];

        let mut warnings = vec![
            "OpenClaw JSON5 comments/formatting will be normalized on write; the original file is in the backup."
                .to_string(),
        ];
        if before.is_some() {
            warnings.push(format!(
                "Existing models.providers.{PROVIDER_ID} was replaced; the previous file is in the backup."
            ));
        }
        if before_token.as_deref().is_some_and(|previous| previous != token) {
            warnings.push(
                "An existing AI Gateway API key was replaced; the previous value is only in the backup."
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
                "baseUrl": openai_base,
                "api": "openai-completions",
                "models": provider.get("models").cloned().unwrap_or(Value::Null)
            },
            "primary": { "before": before_primary, "after": primary_value },
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
            .ok_or_else(|| AppError::BadRequest("Managed OpenClaw state is invalid".to_string()))?;
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
                if let Some(providers) = root
                    .get_mut("models")
                    .and_then(Value::as_object_mut)
                    .and_then(|models| models.get_mut("providers"))
                    .and_then(Value::as_object_mut)
                {
                    providers.remove(PROVIDER_ID);
                    restored.push(format!("models.providers.{PROVIDER_ID}"));
                }
            } else {
                kept.push(KeptChange {
                    path: format!("models.providers.{PROVIDER_ID}"),
                    reason: "Provider entry was changed or its token was rotated after apply.".to_string(),
                });
                warnings.push(
                    "The AI Gateway provider entry was left in place; use the pre-apply backup for a full rollback."
                        .to_string(),
                );
            }
        }

        if let Some(entry) = managed.get("primary").and_then(Value::as_object) {
            let after = entry.get("after").and_then(Value::as_str);
            let current_primary = primary(&root);
            if current_primary.as_deref() == after {
                let before = entry.get("before").and_then(Value::as_str);
                if let Some(model) = root
                    .get_mut("agents")
                    .and_then(Value::as_object_mut)
                    .and_then(|agents| agents.get_mut("defaults"))
                    .and_then(Value::as_object_mut)
                    .and_then(|defaults| defaults.get_mut("model"))
                    .and_then(Value::as_object_mut)
                {
                    match before {
                        Some(value) => { model.insert("primary".to_string(), json!(value)); }
                        None => { model.remove("primary"); }
                    }
                    restored.push("agents.defaults.model.primary".to_string());
                }
            } else if current_primary.is_some() {
                kept.push(KeptChange {
                    path: "agents.defaults.model.primary".to_string(),
                    reason: "Value changed after this client was applied.".to_string(),
                });
            }
        }

        prune_empty(&mut root);
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
