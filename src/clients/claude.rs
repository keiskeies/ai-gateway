//! Claude Code：只改 `~/.claude/settings.json` 的 `env` 段。
//!
//! `permissions`、`hooks`、`mcpServers`、`projects` 等键原样保留；应用前备份整份文件。
//! 撤销时只回滚"当前值仍等于我们写入值"的环境变量，用户后来改过的不动。

use super::*;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub struct ClaudeCodeAdapter;

const BASE_KEY: &str = "ANTHROPIC_BASE_URL";
const TOKEN_KEY: &str = "ANTHROPIC_AUTH_TOKEN";

fn load(path: &std::path::Path) -> AppResult<(Value, Option<Vec<u8>>)> {
    let original = read_bytes(path)?;
    let root = match &original {
        Some(bytes) => serde_json::from_slice::<Value>(bytes).map_err(|error| {
            AppError::BadRequest(format!("Cannot parse {}: {error}", path.display()))
        })?,
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

fn env_object(root: &Value) -> AppResult<Option<&Map<String, Value>>> {
    match root.get("env") {
        None => Ok(None),
        Some(Value::Object(env)) => Ok(Some(env)),
        Some(_) => Err(AppError::BadRequest(
            "settings.json env must be an object".to_string(),
        )),
    }
}

fn ensure_env(root: &mut Value) -> AppResult<&mut Map<String, Value>> {
    if !root.get("env").is_some_and(Value::is_object) {
        if root.get("env").is_some() {
            return Err(AppError::BadRequest(
                "settings.json env must be an object".to_string(),
            ));
        }
        root.as_object_mut()
            .expect("root checked by load")
            .insert("env".to_string(), Value::Object(Map::new()));
    }
    Ok(root
        .get_mut("env")
        .and_then(Value::as_object_mut)
        .expect("env just ensured"))
}

fn model_value(env: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    env.and_then(|env| env.get(key))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn token_from_env(env: Option<&Map<String, Value>>) -> Option<String> {
    model_value(env, TOKEN_KEY).filter(|token| !token.trim().is_empty())
}

impl Adapter for ClaudeCodeAdapter {
    fn id(&self) -> ClientId {
        ClientId::ClaudeCode
    }

    fn path(&self, paths: &ClientPaths) -> PathBuf {
        paths.claude_settings.clone()
    }

    fn status(&self, paths: &ClientPaths, _managed: Option<&Value>) -> AppResult<ClientStatus> {
        let path = self.path(paths);
        let Some(text) = read_text(&path)? else {
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
        let root: Value = serde_json::from_str(&text).map_err(|error| {
            AppError::BadRequest(format!("Cannot parse {}: {error}", path.display()))
        })?;
        let env = env_object(&root)?;
        let mut warnings = vec![];
        if env.is_some_and(|env| env.contains_key("ANTHROPIC_API_KEY")) {
            warnings.push(
                "ANTHROPIC_API_KEY is also set; Claude Code may prefer it over ANTHROPIC_AUTH_TOKEN."
                    .to_string(),
            );
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
                base_url: model_value(env, BASE_KEY),
                model: model_value(env, "ANTHROPIC_MODEL"),
                key_present: token_from_env(env).is_some(),
                provider: None,
            },
            error: None,
            warnings,
        })
    }

    fn extra_specs(&self) -> Vec<ExtraSpec> {
        claude_extra_specs()
    }

    fn read_extras(
        &self,
        paths: &ClientPaths,
    ) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let empty = (BTreeMap::new(), BTreeMap::new());
        let Ok(text) = read_text(&self.path(paths)) else {
            return empty;
        };
        let Some(text) = text else {
            return empty;
        };
        let Ok(root) = serde_json::from_str::<Value>(&text) else {
            return empty;
        };
        let Ok(Some(env)) = env_object(&root) else {
            return empty;
        };
        let read = |key: &str| {
            env.get(key).and_then(|value| match value {
                Value::String(text) => Some(text.clone()),
                // 数值键在文件里是不带引号的数字
                Value::Number(number) => Some(number.to_string()),
                Value::Bool(flag) => Some(if *flag { "1".to_string() } else { "0".to_string() }),
                _ => None,
            })
            .filter(|value| !value.trim().is_empty())
        };
        let extras = CLAUDE_ENV_KEYS
            .iter()
            .filter_map(|key| read(key).map(|value| ((*key).to_string(), value)))
            .collect();
        let names = [
            ("sonnet", "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME"),
            ("opus", "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME"),
            ("haiku", "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME"),
            ("fable", "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME"),
        ]
        .into_iter()
        .filter_map(|(tier, key)| {
            // 显示名等于实际模型时不算"自定义过"，表单里留空即可
            read(key).map(|value| (tier.to_string(), value))
        })
        .collect();
        (extras, names)
    }

    fn current_token(&self, paths: &ClientPaths) -> AppResult<Option<String>> {
        let path = self.path(paths);
        let Some(text) = read_text(&path)? else {
            return Ok(None);
        };
        let root: Value = serde_json::from_str(&text).map_err(|error| {
            AppError::BadRequest(format!("Cannot parse {}: {error}", path.display()))
        })?;
        Ok(token_from_env(env_object(&root)?))
    }

    fn plan(
        &self,
        paths: &ClientPaths,
        models: &ResolvedModels,
        base_url: &str,
        token: &str,
        _allow_existing: bool,
        extras: &ClientExtras,
    ) -> AppResult<ClientPlan> {
        let path = self.path(paths);
        let (mut root, original) = load(&path)?;
        let env_existed = root.get("env").is_some();
        let before_token = env_object(&root).ok().and_then(token_from_env);
        let env = ensure_env(&mut root)?;

        let mut desired: Vec<(String, String, bool)> = vec![
            (BASE_KEY.to_string(), base_url.to_string(), false),
            (TOKEN_KEY.to_string(), token.to_string(), true),
            ("ANTHROPIC_MODEL".to_string(), models.default.clone(), false),
        ];
        for (tier, actual) in [
            ("sonnet", &models.sonnet),
            ("opus", &models.opus),
            ("haiku", &models.haiku),
            ("fable", &models.fable),
        ] {
            let prefix = format!("ANTHROPIC_DEFAULT_{}", tier.to_uppercase());
            // 显示名与实际请求模型可以不同：菜单里读起来友好，发出去的仍是虚拟模型名
            let name = extras
                .tier_names
                .get(tier)
                .cloned()
                .unwrap_or_else(|| actual.clone());
            desired.push((format!("{prefix}_MODEL"), actual.clone(), false));
            desired.push((format!("{prefix}_MODEL_NAME"), name, false));
        }
        desired.push((
            "CLAUDE_CODE_SUBAGENT_MODEL".to_string(),
            models.subagent.clone(),
            false,
        ));
        for (key, value) in &extras.env {
            desired.push((key.clone(), value.clone(), false));
        }

        // 上次代写、这次不再要的附加键要撤掉，否则关掉一个开关却留着旧值
        let wanted: std::collections::BTreeSet<&str> = desired
            .iter()
            .map(|(key, _, _)| key.as_str())
            .collect();
        let previous = extras
            .managed
            .as_ref()
            .and_then(|managed| managed.get("env"))
            .and_then(Value::as_object);
        let removals: Vec<String> = previous
            .map(|entries| {
                entries
                    .keys()
                    .filter(|key| CLAUDE_ENV_KEYS.contains(&key.as_str()) && !wanted.contains(key.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let mut changes = Vec::new();
        let mut managed_env = Map::new();
        for (key, value, secret) in &desired {
            let current = env.get(key.as_str()).cloned();
            // 同一个键被反复代写时，托管记录里要一直是用户最初的那个值
            let before = previous
                .and_then(|entries| entries.get(key.as_str()))
                .and_then(|entry| entry.get("before"))
                .cloned()
                .filter(|value| !value.is_null())
                .or(current);
            changes.push(change(
                format!("env.{key}"),
                before.clone(),
                if is_numeric_env_key(key) {
                    value.parse::<i64>().map(|number| json!(number)).unwrap_or_else(|_| json!(value))
                } else {
                    json!(value)
                },
                *secret,
            ));
            // 数值键写数字：写成字符串的话 Claude Code 当作没设
            let typed = if is_numeric_env_key(key) {
                value
                    .parse::<i64>()
                    .map(|number| json!(number))
                    .unwrap_or_else(|_| json!(value))
            } else {
                json!(value)
            };
            if !secret {
                managed_env.insert(
                    key.clone(),
                    json!({
                        "before": before.clone().unwrap_or(Value::Null),
                        "after": typed,
                    }),
                );
            }
            env.insert(key.clone(), typed);
        }
        for key in &removals {
            let current = env.remove(key.as_str());
            // 记住的是"用户原来的值"，不是上一次代写留下的值——否则撤销会把我们的值当原件还回去
            let before = previous
                .and_then(|entries| entries.get(key.as_str()))
                .and_then(|entry| entry.get("before"))
                .cloned()
                .filter(|value| !value.is_null())
                .or(current);
            changes.push(ClientChange {
                path: format!("env.{key}"),
                before: before.as_ref().and_then(Value::as_str).map(str::to_string),
                after: String::new(),
                secret: false,
            });
            managed_env.insert(
                key.clone(),
                json!({ "before": before.unwrap_or(Value::Null), "after": null }),
            );
        }

        let mut warnings = Vec::new();
        if env.contains_key("ANTHROPIC_API_KEY") {
            warnings.push(
                "ANTHROPIC_API_KEY is also set; Claude Code may prefer it over ANTHROPIC_AUTH_TOKEN."
                    .to_string(),
            );
        }
        if before_token.as_deref().is_some_and(|previous| previous != token) {
            warnings.push(
                "An existing ANTHROPIC_AUTH_TOKEN was replaced; the previous value is only in the backup."
                    .to_string(),
            );
        }

        let managed = json!({
            "created_file": original.is_none(),
            "created_env": !env_existed,
            "token_key": TOKEN_KEY,
            "token_before_present": before_token.is_some(),
            "env": managed_env,
        });
        let content = json_pretty(&root)?;

        Ok(ClientPlan {
            path,
            original,
            content,
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

        let managed_env = managed
            .get("env")
            .and_then(Value::as_object)
            .ok_or_else(|| AppError::BadRequest("Managed Claude state is invalid".to_string()))?;
        let token_key = managed
            .get("token_key")
            .and_then(Value::as_str)
            .unwrap_or(TOKEN_KEY);
        let token_hash = managed.get("token_hash").and_then(Value::as_str);
        let token_before_present = managed
            .get("token_before_present")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let mut restored = Vec::new();
        let mut kept = Vec::new();
        let mut warnings = Vec::new();
        {
            let env = root
                .get_mut("env")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "settings.json env is missing or not an object; restore the backup instead."
                            .to_string(),
                    )
                })?;

            for (key, entry) in managed_env {
                // after 为 null 表示这次接入把键删了：撤销时"键不在"才算没被动过
                let untouched = match entry.get("after") {
                    None => continue,
                    Some(Value::Null) => !env.contains_key(key),
                    Some(after) => env.get(key) == Some(after),
                };
                if !untouched {
                    kept.push(KeptChange {
                        path: format!("env.{key}"),
                        reason: "Value changed after this client was applied.".to_string(),
                    });
                    continue;
                }
                let before = entry.get("before").cloned().unwrap_or(Value::Null);
                if before.is_null() {
                    env.remove(key);
                } else {
                    env.insert(key.clone(), before);
                }
                restored.push(format!("env.{key}"));
            }

            if let Some(current) = env.get(token_key).and_then(Value::as_str) {
                let current = current.trim().to_string();
                if token_hash.is_some_and(|hash| sha256_hex(&current) == hash) {
                    env.remove(token_key);
                    restored.push(format!("env.{token_key}"));
                    if token_before_present {
                        warnings.push(
                            "The previous ANTHROPIC_AUTH_TOKEN cannot be restored in place; use the pre-apply backup if you need the original credential."
                                .to_string(),
                        );
                    }
                } else if !current.is_empty() {
                    kept.push(KeptChange {
                        path: format!("env.{token_key}"),
                        reason: "Token changed after this client was applied.".to_string(),
                    });
                }
            }
        }

        let created_env = managed
            .get("created_env")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if created_env && root.get("env").is_some_and(|env| env.as_object().is_some_and(Map::is_empty)) {
            root.as_object_mut().expect("root object").remove("env");
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
