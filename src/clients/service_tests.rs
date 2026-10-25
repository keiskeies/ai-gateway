use super::*;
use crate::db::{self, DbPool};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn test_db() -> (DbPool, PathBuf) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-clients");
    fs::create_dir_all(&dir).expect("create test dir");
    let path = dir.join(format!("service-{}.db", uuid::Uuid::new_v4()));
    let pool = db::init_pool(&path).expect("init db");
    let conn = pool.get().expect("conn");
    conn.execute(
        "INSERT INTO proxies (id, name, created_at) VALUES ('p1', 'agnes', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("seed proxy");
    (pool, path)
}

fn temp_home() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-clients")
        .join(format!("home-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&path).expect("create home");
    path
}

fn write_settings(paths: &ClientPaths, text: &str) {
    fs::create_dir_all(paths.claude_settings.parent().unwrap()).expect("claude dir");
    fs::write(&paths.claude_settings, text).expect("settings");
}

fn request() -> ClientApplyRequest {
    ClientApplyRequest {
        default_model: "agnes".into(),
        base_url: None,
        sonnet_model: None,
        opus_model: None,
        haiku_model: None,
        fable_model: None,
        subagent_model: None,
        tier_names: None,
        env: None,
}
}

const ORIGINAL: &str = r#"{
  "permissions": { "allow": ["Bash"] },
  "env": {
    "ANTHROPIC_BASE_URL": "http://old",
    "ANTHROPIC_AUTH_TOKEN": "old-token",
    "ANTHROPIC_MODEL": "old-model"
  }
}"#;

#[test]
fn apply_creates_key_backup_state_and_restore_disables_the_key() {
    let (pool, db_path) = test_db();
    let home = temp_home();
    let paths = ClientPaths::for_home(&home);
    write_settings(&paths, ORIGINAL);
    let backup_root = home.join("backups");

    let outcome = service::apply(
        &pool,
        &paths,
        ClientId::ClaudeCode,
        &request(),
        1994,
        &backup_root,
    )
    .expect("apply");
    assert!(outcome.result.created_key);
    assert!(outcome.cache_refresh);
    assert!(outcome.result.backup_path.as_deref().is_some_and(|path| Path::new(path).exists()));
    let backup = fs::read_to_string(outcome.result.backup_path.as_deref().unwrap()).unwrap();
    assert_eq!(backup, ORIGINAL, "backup is the exact pre-apply file");

    let applied: Value = serde_json::from_str(&fs::read_to_string(&paths.claude_settings).unwrap()).unwrap();
    assert_eq!(applied["permissions"]["allow"][0], "Bash");
    assert_eq!(applied["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:1994");
    assert_eq!(applied["env"]["ANTHROPIC_MODEL"], "agnes");
    let token = applied["env"]["ANTHROPIC_AUTH_TOKEN"].as_str().unwrap().to_string();
    assert!(token.starts_with("sk-aigw-"));
    let stored = db::api_key::get_by_key(&pool, &token).expect("lookup key").expect("key exists");
    assert!(stored.enabled);
    assert_eq!(stored.id, outcome.result.key_id);

    let state = db::client_config::get(&pool, "claude_code").expect("state").expect("row");
    assert_eq!(state.token_hash.as_deref(), Some(sha256_hex(&token).as_str()));
    assert_eq!(state.key_id.as_deref(), Some(outcome.result.key_id.as_str()));

    let statuses = service::list(&pool, &paths).expect("list");
    let claude = statuses.iter().find(|status| status.id == "claude_code").unwrap();
    assert!(claude.managed);
    assert!(claude.current.base_url.as_deref() == Some("http://127.0.0.1:1994"));

    let restored = service::restore(&pool, &paths, ClientId::ClaudeCode, &backup_root).expect("restore");
    assert_eq!(restored.result.disabled_key.as_deref(), Some(outcome.result.key_name.as_str()));
    assert!(restored.cache_refresh);
    let after: Value = serde_json::from_str(&fs::read_to_string(&paths.claude_settings).unwrap()).unwrap();
    assert_eq!(after["permissions"]["allow"][0], "Bash");
    assert_eq!(after["env"]["ANTHROPIC_BASE_URL"], "http://old");
    assert_eq!(after["env"]["ANTHROPIC_MODEL"], "old-model");
    assert!(after["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
    assert!(db::client_config::get(&pool, "claude_code").expect("state").is_none());
    assert!(!db::api_key::get(&pool, &outcome.result.key_id).expect("key").expect("row").enabled);

    let _ = fs::remove_dir_all(&home);
    for suffix in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{}", db_path.display(), suffix));
    }
}

#[test]
fn preview_does_not_write_or_create_a_key() {
    let (pool, db_path) = test_db();
    let home = temp_home();
    let paths = ClientPaths::for_home(&home);
    write_settings(&paths, ORIGINAL);
    let before = fs::read_to_string(&paths.claude_settings).unwrap();
    let preview = service::preview(&pool, &paths, ClientId::ClaudeCode, &request(), 1994).expect("preview");
    assert_eq!(preview.key_action, "create");
    assert_eq!(preview.changes.iter().filter(|change| change.secret).count(), 1);
    assert!(preview.changes.iter().any(|change| change.path == "env.ANTHROPIC_MODEL"));
    assert_eq!(fs::read_to_string(&paths.claude_settings).unwrap(), before);
    assert!(db::api_key::list(&pool).expect("keys").is_empty());
    assert!(db::client_config::get(&pool, "claude_code").expect("state").is_none());
    let _ = fs::remove_dir_all(&home);
    for suffix in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{}", db_path.display(), suffix));
    }
}

#[test]
fn restore_backup_restores_the_original_bytes_and_disables_the_key() {
    let (pool, db_path) = test_db();
    let home = temp_home();
    let paths = ClientPaths::for_home(&home);
    write_settings(&paths, ORIGINAL);
    let backup_root = home.join("backups");
    let applied = service::apply(
        &pool,
        &paths,
        ClientId::ClaudeCode,
        &request(),
        1994,
        &backup_root,
    )
    .expect("apply");
    assert!(applied.result.created_key);
    let restored = service::restore_backup(&pool, &paths, ClientId::ClaudeCode, &backup_root)
        .expect("restore backup");
    assert_eq!(fs::read_to_string(&paths.claude_settings).unwrap(), ORIGINAL);
    assert!(restored.result.disabled_key.is_some());
    assert!(restored.cache_refresh);
    assert!(db::client_config::get(&pool, "claude_code").expect("state").is_none());
    assert!(!db::api_key::get(&pool, &applied.result.key_id).expect("key").expect("row").enabled);
    let _ = fs::remove_dir_all(&home);
    for suffix in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{}", db_path.display(), suffix));
    }
}
