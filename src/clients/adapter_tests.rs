use super::*;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn home() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-clients")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&path).expect("create test home");
    path
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create config dir");
    }
    fs::write(path, text).expect("write config");
}

fn models() -> ResolvedModels {
    ResolvedModels {
        default: "agnes".into(),
        sonnet: "agnes".into(),
        opus: "agnes".into(),
        haiku: "agnes".into(),
        fable: "agnes".into(),
        subagent: "agnes".into(),
    }
}

fn with_hash(managed: &Value, token: &str) -> Value {
    let mut value = managed.clone();
    value
        .as_object_mut()
        .expect("managed object")
        .insert("token_hash".to_string(), json!(sha256_hex(token)));
    value
}

fn apply_plan(adapter: &dyn Adapter, paths: &ClientPaths, token: &str) -> ClientPlan {
    apply_plan_with(adapter, paths, token, &ClientExtras::default())
}

fn apply_plan_with(
    adapter: &dyn Adapter,
    paths: &ClientPaths,
    token: &str,
    extras: &ClientExtras,
) -> ClientPlan {
    let plan = adapter
        .plan(paths, &models(), "http://127.0.0.1:1994", token, false, extras)
        .expect("plan");
    assert!(plan.original.is_some(), "test config must exist");
    fs::write(adapter.path(paths), &plan.content).expect("apply plan");
    plan
}

fn restore_and_parse_json(adapter: &dyn Adapter, paths: &ClientPaths, plan: &ClientPlan, token: &str) -> Value {
    let restored = adapter
        .restore(paths, &with_hash(&plan.managed, token))
        .expect("restore");
    assert!(restored.changed, "restore should change the file");
    fs::write(adapter.path(paths), &restored.content).expect("write restored");
    serde_json::from_slice(&restored.content).expect("restored json")
}

fn parse_json5(text: &str) -> Value {
    json5::from_str(text).expect("json5")
}

fn parse_yaml(text: &str) -> Value {
    serde_yaml::from_str(text).expect("yaml")
}

#[test]
fn claude_code_only_patches_env_and_restores_before_values() {
    let home = home();
    let paths = ClientPaths::for_home(&home);
    let adapter = claude::ClaudeCodeAdapter;
    let original = json!({
        "permissions": {"allow": ["Bash"]},
        "mcpServers": {"fs": {"command": "npx"}},
        "env": {
            "ANTHROPIC_BASE_URL": "http://old",
            "ANTHROPIC_AUTH_TOKEN": "old-token",
            "ANTHROPIC_MODEL": "old-model",
            "KEEP": "yes"
        }
    });
    write(&adapter.path(&paths), &serde_json::to_string_pretty(&original).unwrap());
    let token = "sk-aigw-unit-test-token-0123456789";
    let plan = apply_plan(&adapter, &paths, token);
    let applied: Value = serde_json::from_slice(&plan.content).unwrap();
    assert_eq!(applied["permissions"], original["permissions"]);
    assert_eq!(applied["mcpServers"], original["mcpServers"]);
    assert_eq!(applied["env"]["KEEP"], "yes");
    assert_eq!(applied["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:1994");
    assert_eq!(applied["env"]["ANTHROPIC_AUTH_TOKEN"], token);
    assert_eq!(applied["env"]["ANTHROPIC_MODEL"], "agnes");
    assert_eq!(applied["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"], "agnes");

    let restored = adapter
        .restore(&paths, &with_hash(&plan.managed, token))
        .expect("restore");
    let restored_json: Value = serde_json::from_slice(&restored.content).unwrap();
    assert_eq!(restored_json["permissions"], original["permissions"]);
    assert_eq!(restored_json["env"]["KEEP"], "yes");
    assert_eq!(restored_json["env"]["ANTHROPIC_BASE_URL"], "http://old");
    assert_eq!(restored_json["env"]["ANTHROPIC_MODEL"], "old-model");
    assert!(restored_json["env"].get("ANTHROPIC_AUTH_TOKEN").is_none(), "old token is only in the backup");
    assert!(restored_json["env"].get("ANTHROPIC_DEFAULT_SONNET_MODEL").is_none());
    assert!(
        restored.warnings.iter().any(|warning| warning.contains("previous ANTHROPIC_AUTH_TOKEN")),
        "{:?}",
        restored.warnings
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn claude_code_created_file_is_removed_on_restore() {
    let home = home();
    let paths = ClientPaths::for_home(&home);
    let adapter = claude::ClaudeCodeAdapter;
    let path = adapter.path(&paths);
    let token = "sk-aigw-unit-test-token-0123456789";
    let plan = adapter
        .plan(
            &paths,
            &models(),
            "http://127.0.0.1:1994",
            token,
            false,
            &ClientExtras::default(),
        )
        .expect("plan");
    assert_eq!(plan.managed["created_file"], true);
    write(&path, std::str::from_utf8(&plan.content).unwrap());
    let restored = adapter
        .restore(&paths, &with_hash(&plan.managed, token))
        .expect("restore");
    assert!(restored.remove_file);
    fs::remove_file(&path).expect("remove created file");
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn codex_preserves_other_tables_and_restores_top_level() {
    let home = home();
    let paths = ClientPaths::for_home(&home);
    let adapter = codex::CodexAdapter;
    let original = r#"model = "old-model"
model_provider = "openai"

[projects.demo]
trust_level = "trusted"

[model_providers.other]
name = "Other"
base_url = "https://other.example/v1"
wire_api = "responses"
"#;
    write(&adapter.path(&paths), original);
    let token = "sk-aigw-unit-test-token-0123456789";
    let plan = apply_plan(&adapter, &paths, token);
    let applied: toml::Value = std::str::from_utf8(&plan.content).unwrap().parse().unwrap();
    assert_eq!(applied.get("model").and_then(toml::Value::as_str), Some("agnes"));
    assert_eq!(applied.get("model_provider").and_then(toml::Value::as_str), Some("ai-gateway"));
    assert_eq!(
        applied
            .get("projects")
            .and_then(|projects| projects.get("demo"))
            .and_then(|demo| demo.get("trust_level"))
            .and_then(toml::Value::as_str),
        Some("trusted")
    );
    let gateway = applied
        .get("model_providers")
        .and_then(|providers| providers.get("ai-gateway"))
        .expect("gateway table");
    assert_eq!(gateway.get("base_url").and_then(toml::Value::as_str), Some("http://127.0.0.1:1994/v1"));
    assert_eq!(gateway.get("wire_api").and_then(toml::Value::as_str), Some("chat"));
    assert_eq!(gateway.get("experimental_bearer_token").and_then(toml::Value::as_str), Some(token));
    assert!(applied
        .get("model_providers")
        .and_then(|providers| providers.get("other"))
        .is_some());

    let restored = adapter
        .restore(&paths, &with_hash(&plan.managed, token))
        .expect("restore");
    let restored_toml: toml::Value = std::str::from_utf8(&restored.content).unwrap().parse().unwrap();
    assert_eq!(restored_toml.get("model").and_then(toml::Value::as_str), Some("old-model"));
    assert_eq!(restored_toml.get("model_provider").and_then(toml::Value::as_str), Some("openai"));
    assert!(restored_toml
        .get("model_providers")
        .and_then(|providers| providers.get("ai-gateway"))
        .is_none());
    assert!(restored_toml
        .get("model_providers")
        .and_then(|providers| providers.get("other"))
        .is_some());
    assert!(restored_toml.get("projects").is_some());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn opencode_jsonc_preserves_other_providers() {
    let home = home();
    let path = home.join(".config").join("opencode").join("opencode.jsonc");
    write(
        &path,
        r#"{
  // existing
  "provider": {
    "open-webui": {
      "npm": "@ai-sdk/openai-compatible",
      "options": { "baseURL": "http://old", "apiKey": "other" }
    },
  },
}
"#,
    );
    let paths = ClientPaths::for_home(&home);
    assert_eq!(paths.opencode_config, path);
    let adapter = opencode::OpenCodeAdapter;
    let token = "sk-aigw-unit-test-token-0123456789";
    let plan = apply_plan(&adapter, &paths, token);
    let applied: Value = serde_json::from_slice(&plan.content).unwrap();
    assert_eq!(applied["provider"]["open-webui"]["options"]["baseURL"], "http://old");
    assert_eq!(
        applied["provider"]["ai-gateway"]["options"]["baseURL"],
        "http://127.0.0.1:1994/v1"
    );
    assert_eq!(applied["provider"]["ai-gateway"]["options"]["apiKey"], token);
    assert!(applied["provider"]["ai-gateway"]["models"]["agnes"].is_object());
    let restored = restore_and_parse_json(&adapter, &paths, &plan, token);
    assert!(restored["provider"].get("ai-gateway").is_none());
    assert_eq!(restored["provider"]["open-webui"]["options"]["baseURL"], "http://old");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn openclaw_preserves_providers_and_primary() {
    let home = home();
    let paths = ClientPaths::for_home(&home);
    let adapter = openclaw::OpenClawAdapter;
    write(
        &adapter.path(&paths),
        r#"{
  models: {
    mode: 'merge',
    providers: {
      other: { baseUrl: 'https://other/v1', apiKey: 'x', api: 'openai-completions', models: [{ id: 'm' }] }
    }
  },
  agents: { defaults: { model: { primary: 'other/m', fallbacks: ['other/n'] } } }
}
"#,
    );
    let token = "sk-aigw-unit-test-token-0123456789";
    let plan = apply_plan(&adapter, &paths, token);
    let applied = parse_json5(std::str::from_utf8(&plan.content).unwrap());
    assert_eq!(applied["agents"]["defaults"]["model"]["primary"], "ai-gateway/agnes");
    assert_eq!(applied["agents"]["defaults"]["model"]["fallbacks"][0], "other/n");
    assert_eq!(applied["models"]["providers"]["other"]["baseUrl"], "https://other/v1");
    assert_eq!(applied["models"]["providers"]["ai-gateway"]["baseUrl"], "http://127.0.0.1:1994/v1");
    let restored = restore_and_parse_json(&adapter, &paths, &plan, token);
    assert_eq!(restored["agents"]["defaults"]["model"]["primary"], "other/m");
    assert!(restored["models"]["providers"].get("ai-gateway").is_none());
    assert_eq!(restored["models"]["providers"]["other"]["baseUrl"], "https://other/v1");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn pi_preserves_providers_and_does_not_touch_settings() {
    let home = home();
    let paths = ClientPaths::for_home(&home);
    let adapter = pi::PiAdapter;
    write(
        &adapter.path(&paths),
        r#"{"providers":{"other":{"name":"Other","baseUrl":"https://other/v1","api":"openai-completions","apiKey":"x","models":[{"id":"m"}]}}}"#,
    );
    let token = "sk-aigw-unit-test-token-0123456789";
    let plan = apply_plan(&adapter, &paths, token);
    let applied: Value = serde_json::from_slice(&plan.content).unwrap();
    assert_eq!(applied["providers"]["other"]["baseUrl"], "https://other/v1");
    assert_eq!(applied["providers"]["ai-gateway"]["baseUrl"], "http://127.0.0.1:1994/v1");
    assert_eq!(applied["providers"]["ai-gateway"]["apiKey"], token);
    let restored = restore_and_parse_json(&adapter, &paths, &plan, token);
    assert!(restored["providers"].get("ai-gateway").is_none());
    assert_eq!(restored["providers"]["other"]["baseUrl"], "https://other/v1");
    assert!(!home.join(".pi").join("agent").join("settings.json").exists());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn hermes_preserves_other_providers_and_mcp() {
    let home = home();
    let paths = ClientPaths::for_home(&home);
    let adapter = hermes::HermesAdapter;
    write(
        &adapter.path(&paths),
        r#"model:
  default: old
  provider: other
custom_providers:
  - name: other
    base_url: https://other/v1
    api_key: other-key
    model: old
mcp_servers:
  fs:
    command: npx
"#,
    );
    let token = "sk-aigw-unit-test-token-0123456789";
    let plan = apply_plan(&adapter, &paths, token);
    let applied = parse_yaml(std::str::from_utf8(&plan.content).unwrap());
    assert_eq!(applied["model"]["default"], "agnes");
    assert_eq!(applied["model"]["provider"], "ai-gateway");
    assert_eq!(applied["mcp_servers"]["fs"]["command"], "npx");
    let providers = applied["custom_providers"].as_array().unwrap();
    assert!(providers.iter().any(|provider| provider["name"] == "other"));
    assert!(providers.iter().any(|provider| provider["name"] == "ai-gateway"));
    let restored = adapter
        .restore(&paths, &with_hash(&plan.managed, token))
        .expect("restore");
    let restored_yaml = parse_yaml(std::str::from_utf8(&restored.content).unwrap());
    assert_eq!(restored_yaml["model"]["default"], "old");
    assert_eq!(restored_yaml["model"]["provider"], "other");
    assert_eq!(restored_yaml["mcp_servers"]["fs"]["command"], "npx");
    let providers = restored_yaml["custom_providers"].as_array().unwrap();
    assert!(providers.iter().any(|provider| provider["name"] == "other"));
    assert!(!providers.iter().any(|provider| provider["name"] == "ai-gateway"));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn additive_adapters_refuse_an_unmanaged_ai_gateway_provider() {
    let cases: [(&dyn Adapter, &str); 5] = [
        (&codex::CodexAdapter, "[model_providers.ai-gateway]\nname = \"x\"\n"),
        (&opencode::OpenCodeAdapter, "{\"provider\":{\"ai-gateway\":{}}}"),
        (&openclaw::OpenClawAdapter, "{\"models\":{\"providers\":{\"ai-gateway\":{}}}}"),
        (&pi::PiAdapter, "{\"providers\":{\"ai-gateway\":{}}}"),
        (&hermes::HermesAdapter, "custom_providers:\n  - name: ai-gateway\n"),
    ];
    for (adapter, text) in cases {
        let home = home();
        let paths = ClientPaths::for_home(&home);
        write(&adapter.path(&paths), text);
        let result = adapter.plan(
            &paths,
            &models(),
            "http://127.0.0.1:1994",
            "sk-aigw-unit-test-token-0123456789",
            false,
            &ClientExtras::default(),
        );
        assert!(
            matches!(result, Err(AppError::Conflict(_))),
            "{} should refuse an unmanaged provider",
            adapter.id().as_str()
        );
        let _ = fs::remove_dir_all(&home);
    }
}


fn claude_env(root: &Value) -> &serde_json::Map<String, Value> {
    root.get("env").and_then(Value::as_object).expect("env object")
}

#[test]
fn claude_code_writes_display_names_separately_from_requested_models() {
    let home = home();
    let paths = ClientPaths::for_home(&home);
    let adapter = &claude::ClaudeCodeAdapter;
    write(&adapter.path(&paths), "{\"env\":{}}");

    let mut names = std::collections::BTreeMap::new();
    names.insert("opus".to_string(), "Opus 4.5".to_string());
    let mut env = std::collections::BTreeMap::new();
    env.insert("DISABLE_AUTOUPDATER".to_string(), "1".to_string());
    env.insert("API_TIMEOUT_MS".to_string(), "300000".to_string());
    let extras = ClientExtras {
        tier_names: names,
        env,
        managed: None,
    };

    let plan = apply_plan_with(adapter, &paths, "sk-aigw-unit-test-token-0123456789", &extras);
    let root: Value = serde_json::from_slice(&plan.content).expect("written json");
    let env = claude_env(&root);
    // 菜单里读起来友好，发出去的仍是虚拟模型名
    assert_eq!(env["ANTHROPIC_DEFAULT_OPUS_MODEL"], json!("agnes"));
    assert_eq!(env["ANTHROPIC_DEFAULT_OPUS_MODEL_NAME"], json!("Opus 4.5"));
    assert_eq!(env["ANTHROPIC_DEFAULT_SONNET_MODEL_NAME"], json!("agnes"));
    assert_eq!(env["DISABLE_AUTOUPDATER"], json!("1"));
    // 数值键写数字：写成字符串 Claude Code 会当成没设
    assert_eq!(env["API_TIMEOUT_MS"], json!(300000));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn claude_code_drops_optional_env_when_the_switch_goes_off() {
    let home = home();
    let paths = ClientPaths::for_home(&home);
    let adapter = &claude::ClaudeCodeAdapter;
    write(&adapter.path(&paths), "{\"env\":{\"API_TIMEOUT_MS\":\"60000\"}}");

    let mut env = std::collections::BTreeMap::new();
    env.insert("API_TIMEOUT_MS".to_string(), "300000".to_string());
    let first = apply_plan_with(
        adapter,
        &paths,
        "sk-aigw-unit-test-token-0123456789",
        &ClientExtras {
            tier_names: Default::default(),
            env: env.clone(),
            managed: None,
        },
    );

    // 第二次把开关关掉：附加键要消失，且变更里能看到"删掉"这一条
    let second = adapter
        .plan(
            &paths,
            &models(),
            "http://127.0.0.1:1994",
            "sk-aigw-unit-test-token-0123456789",
            true,
            &ClientExtras {
                tier_names: Default::default(),
                env: Default::default(),
                managed: Some(first.managed.clone()),
            },
        )
        .expect("second plan");
    assert!(second
        .changes
        .iter()
        .any(|change| change.path == "env.API_TIMEOUT_MS" && change.after.is_empty()));
    fs::write(adapter.path(&paths), &second.content).expect("apply second");
    let root: Value = serde_json::from_slice(&second.content).expect("written json");
    assert!(
        !claude_env(&root).contains_key("API_TIMEOUT_MS"),
        "a switch turned off must not leave its env key behind"
    );

    // 撤销要把它恢复成用户原来的值，而不是删掉
    let restored = adapter
        .restore(&paths, &with_hash(&second.managed, "sk-aigw-unit-test-token-0123456789"))
        .expect("restore");
    let root: Value = serde_json::from_slice(&restored.content).expect("restored json");
    assert_eq!(claude_env(&root)["API_TIMEOUT_MS"], json!("60000"));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn only_allowlisted_env_keys_are_accepted() {
    let request = ClientApplyRequest {
        default_model: "agnes".to_string(),
        env: Some(std::collections::HashMap::from([(
            "ANTHROPIC_API_KEY".to_string(),
            "sk-ant-paste".to_string(),
        )])),
        ..Default::default()
    };
    let error = request.extras(None).expect_err("unknown key must be refused");
    assert!(matches!(error, AppError::BadRequest(_)), "{error}");

    let numeric = ClientApplyRequest {
        default_model: "agnes".to_string(),
        env: Some(std::collections::HashMap::from([(
            "API_TIMEOUT_MS".to_string(),
            "soon".to_string(),
        )])),
        ..Default::default()
    };
    assert!(numeric.extras(None).is_err(), "a numeric knob must be a number");
}

#[test]
fn raw_config_editing_validates_backs_up_and_preserves_unmanaged_fields() {
    let home = home();
    let backups = home.join("backups");
    let paths = ClientPaths::for_home(&home);
    let adapter = &claude::ClaudeCodeAdapter;
    write(
        &adapter.path(&paths),
        "{\"env\":{\"ANTHROPIC_MODEL\":\"keep-me\"},\"includeCoAuthoredBy\":false}",
    );

    // 语法不过就一行都不写
    assert!(service::write_config_file(&paths, &backups, ClientId::ClaudeCode, "{\"env\":")
        .unwrap_err()
        .to_string()
        .contains("not valid JSON"));
    let untouched: Value =
        serde_json::from_slice(&fs::read(adapter.path(&paths)).expect("read")).expect("json");
    assert_eq!(untouched["includeCoAuthoredBy"], json!(false));

    let edited = "{\"env\":{\"ANTHROPIC_MODEL\":\"keep-me\",\"API_TIMEOUT_MS\":120000},\"includeCoAuthoredBy\":true,\"maxTokens\":200000}";
    let written = service::write_config_file(&paths, &backups, ClientId::ClaudeCode, edited).expect("write");
    assert!(written.replaced, "an existing file must report as replaced");
    let backup = PathBuf::from(written.backup_path.expect("backup recorded"));
    assert!(backup.exists(), "raw editing must back the file up first");

    let on_disk: Value =
        serde_json::from_slice(&fs::read(adapter.path(&paths)).expect("read")).expect("json");
    assert_eq!(on_disk["env"]["API_TIMEOUT_MS"], json!(120000));
    assert_eq!(on_disk["maxTokens"], json!(200000));
    assert_eq!(on_disk["includeCoAuthoredBy"], json!(true));

    let read_back = service::read_config_file(&paths, ClientId::ClaudeCode).expect("read");
    assert_eq!(read_back.content, edited);
    assert!(read_back.exists);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn raw_config_editing_rejects_empty_and_non_object_content() {
    let home = home();
    let backups = home.join("backups");
    let paths = ClientPaths::for_home(&home);
    for content in ["", "   ", "[1, 2, 3]"] {
        assert!(
            service::write_config_file(&paths, &backups, ClientId::ClaudeCode, content).is_err(),
            "{content:?} must be refused"
        );
    }
    let _ = fs::remove_dir_all(&home);
}
