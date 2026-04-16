use rusqlite::Connection;
use crate::error::AppResult;

pub fn run_migrations(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(SCHEMA_V1)?;
    tracing::info!("Database schema initialized");
    Ok(())
}

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS platforms (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    type            TEXT NOT NULL,
    base_url        TEXT NOT NULL,
    api_key         TEXT NOT NULL DEFAULT '',
    organization    TEXT,
    custom_headers  TEXT NOT NULL DEFAULT '{}',
    status          TEXT NOT NULL DEFAULT 'Active',
    rate_limit      TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS models (
    id              TEXT PRIMARY KEY,
    platform_id     TEXT NOT NULL REFERENCES platforms(id) ON DELETE CASCADE,
    model_id        TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    max_tokens      INTEGER NOT NULL DEFAULT 4096,
    context_window  INTEGER NOT NULL DEFAULT 8192,
    input_price     REAL,
    output_price    REAL,
    capabilities    TEXT NOT NULL DEFAULT '[]',
    status          TEXT NOT NULL DEFAULT 'Active',
    UNIQUE(platform_id, model_id)
);

CREATE TABLE IF NOT EXISTS proxies (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    listen_port INTEGER NOT NULL UNIQUE,
    protocols   TEXT NOT NULL DEFAULT '["OpenAI"]',
    auth_token  TEXT,
    status      TEXT NOT NULL DEFAULT 'Stopped',
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS routes (
    id              TEXT PRIMARY KEY,
    proxy_id        TEXT NOT NULL REFERENCES proxies(id) ON DELETE CASCADE,
    virtual_model   TEXT NOT NULL,
    lb_strategy     TEXT NOT NULL DEFAULT 'RoundRobin',
    retry_policy    TEXT NOT NULL DEFAULT '{"max_retries":2,"retry_on_error":["RateLimit","ServerError","Timeout"],"backoff_ms":500}',
    fallback        TEXT,
    UNIQUE(proxy_id, virtual_model)
);

CREATE TABLE IF NOT EXISTS backends (
    id              TEXT PRIMARY KEY,
    route_id        TEXT NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    platform_id     TEXT NOT NULL REFERENCES platforms(id),
    model_id        TEXT NOT NULL REFERENCES models(id),
    weight          INTEGER NOT NULL DEFAULT 1,
    priority        INTEGER NOT NULL DEFAULT 0,
    max_concurrent  INTEGER,
    status          TEXT NOT NULL DEFAULT 'Active'
);

CREATE TABLE IF NOT EXISTS request_stats (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    proxy_id    TEXT NOT NULL,
    route_id    TEXT NOT NULL,
    backend_id  TEXT NOT NULL,
    status_code INTEGER NOT NULL,
    latency_ms  INTEGER NOT NULL,
    token_input INTEGER,
    token_output INTEGER,
    error_type  TEXT,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_stats_proxy ON request_stats(proxy_id);
CREATE INDEX IF NOT EXISTS idx_stats_route ON request_stats(route_id);
CREATE INDEX IF NOT EXISTS idx_stats_backend ON request_stats(backend_id);
CREATE INDEX IF NOT EXISTS idx_stats_created ON request_stats(created_at);
"#;
