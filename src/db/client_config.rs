use crate::db::DbPool;
use crate::error::AppResult;
use rusqlite::OptionalExtension;

/// 一个客户端接入的托管状态。
/// `managed` 是适配器自己的 JSON（非敏感字段的前后值、token 摘要、provider id 等）；
/// 明文凭据只存在客户端配置和备份文件里，这张表永远不存。
#[derive(Debug, Clone)]
pub struct ClientConfigRow {
    pub client_id: String,
    pub config_path: String,
    pub managed: String,
    pub token_hash: Option<String>,
    pub key_id: Option<String>,
    pub key_name: Option<String>,
    pub backup_path: Option<String>,
    pub applied_at: String,
    pub updated_at: String,
}

const COLS: &str = "client_id, config_path, managed, token_hash, key_id, key_name, backup_path, applied_at, updated_at";

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<ClientConfigRow> {
    Ok(ClientConfigRow {
        client_id: row.get(0)?,
        config_path: row.get(1)?,
        managed: row.get(2)?,
        token_hash: row.get(3)?,
        key_id: row.get(4)?,
        key_name: row.get(5)?,
        backup_path: row.get(6)?,
        applied_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub fn get(pool: &DbPool, client_id: &str) -> AppResult<Option<ClientConfigRow>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let row = conn
        .query_row(
            &format!("SELECT {COLS} FROM client_configs WHERE client_id = ?1"),
            rusqlite::params![client_id],
            map_row,
        )
        .optional()?;
    Ok(row)
}

pub fn list(pool: &DbPool) -> AppResult<Vec<ClientConfigRow>> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM client_configs ORDER BY client_id"))?;
    let rows = stmt.query_map([], map_row)?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

pub fn upsert(pool: &DbPool, row: &ClientConfigRow) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO client_configs (client_id, config_path, managed, token_hash, key_id, key_name, backup_path, applied_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(client_id) DO UPDATE SET
            config_path = excluded.config_path,
            managed = excluded.managed,
            token_hash = excluded.token_hash,
            key_id = excluded.key_id,
            key_name = excluded.key_name,
            backup_path = excluded.backup_path,
            applied_at = excluded.applied_at,
            updated_at = excluded.updated_at",
        rusqlite::params![
            row.client_id,
            row.config_path,
            row.managed,
            row.token_hash,
            row.key_id,
            row.key_name,
            row.backup_path,
            row.applied_at,
            row.updated_at,
        ],
    )?;
    Ok(())
}

pub fn delete(pool: &DbPool, client_id: &str) -> AppResult<()> {
    let conn = pool.get().map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
    conn.execute("DELETE FROM client_configs WHERE client_id = ?1", rusqlite::params![client_id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> (DbPool, std::path::PathBuf) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-db");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("client-config-test-{}.db", uuid::Uuid::new_v4()));
        (crate::db::init_pool(&path).expect("init test db"), path)
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn client_state_round_trips_and_migration_reaches_v22() {
        let (pool, path) = test_db();
        let conn = pool.get().expect("conn");
        let version: i64 = conn
            .query_row("SELECT value FROM _schema_version WHERE id = 1", [], |row| row.get(0))
            .expect("schema version");
        assert_eq!(version, 22);
        drop(conn);

        assert!(get(&pool, "claude_code").expect("empty get").is_none());
        let row = ClientConfigRow {
            client_id: "claude_code".into(),
            config_path: "C:/Users/x/.claude/settings.json".into(),
            managed: r#"{"env":{}}"#.into(),
            token_hash: Some("abc".into()),
            key_id: Some("k1".into()),
            key_name: Some("client-claude".into()),
            backup_path: Some("backup.json".into()),
            applied_at: "2026-10-25T00:00:00Z".into(),
            updated_at: "2026-10-25T00:00:00Z".into(),
        };
        upsert(&pool, &row).expect("insert");
        let loaded = get(&pool, "claude_code").expect("get").expect("row");
        assert_eq!(loaded.managed, row.managed);
        assert_eq!(loaded.token_hash.as_deref(), Some("abc"));
        assert_eq!(list(&pool).expect("list").len(), 1);

        let updated = ClientConfigRow {
            updated_at: "2026-10-25T01:00:00Z".into(),
            ..row
        };
        upsert(&pool, &updated).expect("update");
        let loaded = get(&pool, "claude_code").expect("get").expect("row");
        assert_eq!(loaded.updated_at, "2026-10-25T01:00:00Z");

        delete(&pool, "claude_code").expect("delete");
        assert!(get(&pool, "claude_code").expect("get after delete").is_none());
        cleanup_db(&path);
    }
}
