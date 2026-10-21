use crate::db::{get_conn, DbPool};
use crate::error::AppResult;
use crate::models::auth::{AuthUser, Session, User};

pub fn user_count(pool: &DbPool) -> AppResult<i64> {
    let conn = get_conn(pool)?;
    let count = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    Ok(count)
}

/// 仅在还没有任何用户时创建首个管理员。
/// 返回 None 表示已经初始化过，避免并发 setup 覆盖凭据。
pub fn create_initial_user(pool: &DbPool, username: &str, password_hash: &str) -> AppResult<Option<User>> {
    let mut conn = get_conn(pool)?;
    let tx = conn.transaction()?;
    let count: i64 = tx.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    if count > 0 {
        return Ok(None);
    }
    let user = User {
        id: uuid::Uuid::new_v4().to_string(),
        username: username.to_string(),
        password_hash: password_hash.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    tx.execute(
        "INSERT INTO users (id, username, password_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![user.id, user.username, user.password_hash, user.created_at],
    )?;
    tx.commit()?;
    Ok(Some(user))
}

pub fn find_user_by_username(pool: &DbPool, username: &str) -> AppResult<Option<User>> {
    let conn = get_conn(pool)?;
    let result = conn.query_row(
        "SELECT id, username, password_hash, created_at FROM users WHERE username = ?1",
        [username],
        map_user,
    );
    match result {
        Ok(user) => Ok(Some(user)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn find_user_by_id(pool: &DbPool, id: &str) -> AppResult<Option<User>> {
    let conn = get_conn(pool)?;
    let result = conn.query_row(
        "SELECT id, username, password_hash, created_at FROM users WHERE id = ?1",
        [id],
        map_user,
    );
    match result {
        Ok(user) => Ok(Some(user)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn map_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?,
        username: row.get(1)?,
        password_hash: row.get(2)?,
        created_at: row.get(3)?,
    })
}

pub fn update_password(pool: &DbPool, user_id: &str, password_hash: &str) -> AppResult<()> {
    let conn = get_conn(pool)?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![password_hash, now, user_id],
    )?;
    Ok(())
}

pub fn create_session(pool: &DbPool, session: &Session) -> AppResult<()> {
    let conn = get_conn(pool)?;
    conn.execute(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![session.token_hash, session.user_id, session.created_at, session.expires_at],
    )?;
    Ok(())
}

/// 查询会话并校验过期时间；过期会话会被顺手删除。
pub fn find_session(pool: &DbPool, token_hash: &str) -> AppResult<Option<(Session, AuthUser)>> {
    let conn = get_conn(pool)?;
    let result = conn.query_row(
        "SELECT s.token_hash, s.user_id, s.created_at, s.expires_at, u.username
         FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ?1",
        [token_hash],
        |row| {
            Ok((
                Session {
                    token_hash: row.get(0)?,
                    user_id: row.get(1)?,
                    created_at: row.get(2)?,
                    expires_at: row.get(3)?,
                },
                AuthUser {
                    id: row.get(1)?,
                    username: row.get(4)?,
                },
            ))
        },
    );
    match result {
        Ok((session, user)) => {
            let expired = chrono::DateTime::parse_from_rfc3339(&session.expires_at)
                .map(|t| t.with_timezone(&chrono::Utc) <= chrono::Utc::now())
                .unwrap_or(true);
            if expired {
                let _ = conn.execute("DELETE FROM sessions WHERE token_hash = ?1", [token_hash]);
                Ok(None)
            } else {
                Ok(Some((session, user)))
            }
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete_session(pool: &DbPool, token_hash: &str) -> AppResult<()> {
    let conn = get_conn(pool)?;
    conn.execute("DELETE FROM sessions WHERE token_hash = ?1", [token_hash])?;
    Ok(())
}

pub fn delete_user_sessions(pool: &DbPool, user_id: &str) -> AppResult<()> {
    let conn = get_conn(pool)?;
    conn.execute("DELETE FROM sessions WHERE user_id = ?1", [user_id])?;
    Ok(())
}

pub fn purge_expired_sessions(pool: &DbPool, now: &str) -> AppResult<()> {
    let conn = get_conn(pool)?;
    conn.execute("DELETE FROM sessions WHERE expires_at <= ?1", [now])?;
    Ok(())
}
