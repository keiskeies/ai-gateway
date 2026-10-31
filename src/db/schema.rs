use rusqlite::Connection;
use crate::error::AppResult;

/// 本程序编译进来的表结构版本（= `run_migrations` 会跑到的最后一步）。
/// 有单测把两者钉在一起，加迁移时不改常量就编译不过测试，不会悄悄漂移。
/// 备份还原用的是它，而不是当前库的版本：见 `db::backup::restore`。
pub const CURRENT_SCHEMA_VERSION: i64 = 30;

/// 库当前的表结构版本。`_schema_version` 还没建（全新库）按 0 处理。
/// 备份还原也要读它：版本不一致的快照不能就地假定本机代码读得懂。
pub fn version(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT value FROM _schema_version WHERE id = 1",
        [],
        |row| row.get(0),
    ).unwrap_or(0)
}

pub fn run_migrations(conn: &Connection) -> AppResult<()> {
    // Check current version
    let current_version: i32 = version(conn) as i32;

    if current_version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.execute("CREATE TABLE IF NOT EXISTS _schema_version (id INTEGER PRIMARY KEY, value INTEGER NOT NULL)", [])?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 4)", [])?;
        tracing::info!("Database schema V1 initialized");
    }

    if current_version < 2 {
        conn.execute_batch(SCHEMA_V2_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 2)", [])?;
        tracing::info!("Database migrated to V2 (removed listen_port and status from proxies)");
    }

    if current_version < 3 {
        conn.execute_batch(SCHEMA_V3_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 3)", [])?;
        tracing::info!("Database migrated to V3 (added api_keys table)");
    }

    if current_version < 4 {
        conn.execute_batch(SCHEMA_V4_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 4)", [])?;
        tracing::info!("Database migrated to V4 (simplified proxies, removed virtual_model from routes)");
    }

    if current_version < 5 {
        conn.execute_batch(SCHEMA_V5_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 5)", [])?;
        tracing::info!("Database migrated to V5 (backends.model_id now stores model ID string directly)");
    }

    if current_version < 6 {
        conn.execute_batch(SCHEMA_V6_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 6)", [])?;
        tracing::info!("Database migrated to V6 (backend param_overrides/unknown_params, route default_max_tokens)");
    }

    if current_version < 7 {
        conn.execute_batch(SCHEMA_V7_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 7)", [])?;
        tracing::info!("Database migrated to V7 (request_stats ttft_ms / stream flags)");
    }

    if current_version < 8 {
        conn.execute_batch(SCHEMA_V8_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 8)", [])?;
        tracing::info!("Database migrated to V8 (request_stats cache_read_tokens / cache_write_tokens)");
    }

    if current_version < 9 {
        conn.execute_batch(SCHEMA_V9_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 9)", [])?;
        tracing::info!("Database migrated to V9 (backend reasoning_style / reasoning_echo)");
    }

    if current_version < 10 {
        conn.execute_batch(SCHEMA_V10_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 10)", [])?;
        tracing::info!("Database migrated to V10 (users / sessions for admin login)");
    }

    if current_version < 11 {
        conn.execute_batch(SCHEMA_V11_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 11)", [])?;
        tracing::info!("Database migrated to V11 (api_keys.enabled)");
    }

    if current_version < 12 {
        conn.execute_batch(SCHEMA_V12_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 12)", [])?;
        tracing::info!("Database migrated to V12 (request_stats.api_key_id for per-key usage)");
    }

    if current_version < 13 {
        conn.execute_batch(SCHEMA_V13_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 13)", [])?;
        tracing::info!("Database migrated to V13 (platform_health connectivity history)");
    }

    if current_version < 14 {
        conn.execute_batch(SCHEMA_V14_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 14)", [])?;
        tracing::info!("Database migrated to V14 (orphaned bound API keys disabled)");
    }

    if current_version < 15 {
        conn.execute_batch(SCHEMA_V15_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 15)", [])?;
        tracing::info!("Database migrated to V15 (request_stats.cost per-request cost estimate)");
    }

    if current_version < 16 {
        conn.execute_batch(SCHEMA_V16_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 16)", [])?;
        tracing::info!("Database migrated to V16 (request_attempts retry trace)");
    }

    if current_version < 17 {
        migrate_api_keys_to_hashed(conn)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 17)", [])?;
        tracing::info!("Database migrated to V17 (api_keys keeps sha256 hash + mask, never plaintext)");
    }

    if current_version < 18 {
        conn.execute_batch(SCHEMA_V18_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 18)", [])?;
        tracing::info!("Database migrated to V18 (api_keys expiry + quota)");
    }

    if current_version < 19 {
        conn.execute_batch(SCHEMA_V19_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 19)", [])?;
        tracing::info!("Database migrated to V19 (alerts + request_stats.api_key_id index)");
    }

    if current_version < 20 {
        conn.execute_batch(SCHEMA_V20_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 20)", [])?;
        tracing::info!("Database migrated to V20 (api_keys cost quota + rolling quota window)");
    }

    if current_version < 21 {
        conn.execute_batch(SCHEMA_V21_MIGRATION)?;
        // 加列单独走、带守卫：`execute_batch` 逐语句自动提交，建表成功而加列失败时版本号仍停在 20，
        // 重跑就会撞上"列已存在"——SQLite 没有 ADD COLUMN IF NOT EXISTS，只能自己查。
        let has_col: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('request_stats') WHERE name = 'replayed_from'",
            [],
            |r| r.get(0),
        )?;
        if has_col == 0 {
            conn.execute("ALTER TABLE request_stats ADD COLUMN replayed_from INTEGER", [])?;
        }
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 21)", [])?;
        tracing::info!("Database migrated to V21 (opt-in request_bodies + request_stats.replayed_from)");
    }

    if current_version < 22 {
        conn.execute_batch(SCHEMA_V22_MIGRATION)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 22)", [])?;
        tracing::info!("Database migrated to V22 (client_configs for local coding client integration)");
    }

    if current_version < 23 {
        // `execute_batch` 逐语句自动提交：版本号可能停在 22，而列已经加过一半。
        // SQLite 没有 ADD COLUMN IF NOT EXISTS，所以逐列查 pragma 再决定。
        add_column_if_missing(conn, "request_stats", "path",
            "ALTER TABLE request_stats ADD COLUMN path TEXT")?;
        add_column_if_missing(conn, "request_attempts", "forward_url",
            "ALTER TABLE request_attempts ADD COLUMN forward_url TEXT")?;
        conn.execute_batch(SCHEMA_V23_TABLES)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 23)", [])?;
        tracing::info!("Database migrated to V23 (request path / forward URL / headers / response bodies)");
    }

    if current_version < 24 {
        add_column_if_missing(conn, "platforms", "credit_budget",
            "ALTER TABLE platforms ADD COLUMN credit_budget REAL")?;
        add_column_if_missing(conn, "platforms", "credit_window",
            "ALTER TABLE platforms ADD COLUMN credit_window TEXT")?;
        add_column_if_missing(conn, "models", "credit_price",
            "ALTER TABLE models ADD COLUMN credit_price REAL")?;
        add_column_if_missing(conn, "request_stats", "credits",
            "ALTER TABLE request_stats ADD COLUMN credits REAL")?;
        conn.execute_batch(SCHEMA_V24_TABLES)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 24)", [])?;
        tracing::info!("Database migrated to V24 (platform credits / model credit price / credit ledger)");
    }

    if current_version < 25 {
        add_column_if_missing(conn, "platforms", "catalog_key",
            "ALTER TABLE platforms ADD COLUMN catalog_key TEXT")?;
        add_column_if_missing(conn, "models", "catalog_key",
            "ALTER TABLE models ADD COLUMN catalog_key TEXT")?;
        add_column_if_missing(conn, "models", "cache_read_price",
            "ALTER TABLE models ADD COLUMN cache_read_price REAL")?;
        add_column_if_missing(conn, "models", "cache_write_price",
            "ALTER TABLE models ADD COLUMN cache_write_price REAL")?;
        add_column_if_missing(conn, "models", "peak_pricing",
            "ALTER TABLE models ADD COLUMN peak_pricing TEXT")?;
        conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_platforms_catalog ON platforms(catalog_key);")?;
        backfill_catalog_keys(conn)?;
        dedupe_models_by_catalog(conn)?;
        // 唯一索引必须在去重之后建：旧库可能已经有同 URL 多账号的重复模型行。
        conn.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_models_catalog_model ON models(catalog_key, model_id);",
        )?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 25)", [])?;
        tracing::info!("Database migrated to V25 (shared model catalog + cache/peak pricing)");
    }

    if current_version < 26 {
        // 缓存价差必须按请求发生时的单价冻结：用今天的价格回算历史行会改写已发生的成本。
        add_column_if_missing(conn, "request_stats", "cache_savings",
            "ALTER TABLE request_stats ADD COLUMN cache_savings REAL")?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 26)", [])?;
        tracing::info!("Database migrated to V26 (request_stats.cache_savings frozen cache price delta)");
    }

    if current_version < 27 {
        // 正文超限时改存前缀并标记，详情页仍能看到内容；重放只认完整正文。
        add_column_if_missing(conn, "request_bodies", "truncated",
            "ALTER TABLE request_bodies ADD COLUMN truncated INTEGER NOT NULL DEFAULT 0")?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 27)", [])?;
        tracing::info!("Database migrated to V27 (request_bodies.truncated for partial capture)");
    }

    if current_version < 28 {
        // 私有协议上游（Trae / WorkBuddy）：adapter 决定请求怎么构造，NULL = 由 type 推出。
        // credit_unit 决定额度怎么记账，NULL = 不计量；与 credit_budget 正交：
        // 同一个 credit_budget 在 credits 下走台账、在 currency 下走 cost、在 requests 下只限流。
        add_column_if_missing(conn, "platforms", "adapter",
            "ALTER TABLE platforms ADD COLUMN adapter TEXT")?;
        add_column_if_missing(conn, "platforms", "credit_unit",
            "ALTER TABLE platforms ADD COLUMN credit_unit TEXT")?;
        // WorkBuddy 的模型目录直接给积分倍率（"x0.79 credits"），比“每次 1 分”精确；
        // NULL 视为 1.0，旧数据行为不变。
        add_column_if_missing(conn, "models", "credit_multiplier",
            "ALTER TABLE models ADD COLUMN credit_multiplier REAL")?;
        conn.execute_batch(SCHEMA_V28_TABLES)?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 28)", [])?;
        tracing::info!("Database migrated to V28 (platform adapter / credit unit / credential vault)");
    }

    if current_version < 29 {
        // 路由级开关：每次上游尝试在整段 prompt 最前面插入 32~128 位随机数字串，
        // 让上游看到的前缀每次都不同。默认 0 = 关，老库行为不变。
        add_column_if_missing(conn, "routes", "request_nonce",
            "ALTER TABLE routes ADD COLUMN request_nonce INTEGER NOT NULL DEFAULT 0")?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 29)", [])?;
        tracing::info!("Database migrated to V29 (routes.request_nonce)");
    }

    if current_version < 30 {
        // 记录本次尝试实际注入的前置随机串，Traffic 详情里能看见「到底加了什么」。
        add_column_if_missing(conn, "request_attempts", "nonce",
            "ALTER TABLE request_attempts ADD COLUMN nonce TEXT")?;
        conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 30)", [])?;
        tracing::info!("Database migrated to V30 (request_attempts.nonce)");
    }

    Ok(())
}

/// V25 第一阶段：按 `type + normalized base_url` 回填平台与模型的 catalog_key。
/// 模型行如果找不到平台（悬挂行）保持 NULL；唯一索引允许 NULL，不阻塞启动。
fn backfill_catalog_keys(conn: &Connection) -> AppResult<()> {
    use crate::models::platform::PlatformType;
    let rows: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare("SELECT id, type, base_url FROM platforms")?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };
    for (id, type_json, base_url) in rows {
        let platform_type: PlatformType =
            serde_json::from_str(&type_json).unwrap_or(PlatformType::OpenAI);
        let key = crate::catalog::catalog_key(&platform_type, &base_url);
        conn.execute(
            "UPDATE platforms SET catalog_key = ?2 WHERE id = ?1",
            rusqlite::params![id, key],
        )?;
    }
    conn.execute(
        "UPDATE models SET catalog_key = (
             SELECT p.catalog_key FROM platforms p WHERE p.id = models.platform_id
         ) WHERE catalog_key IS NULL",
        [],
    )?;
    Ok(())
}

/// V25 第二阶段：同一 `(catalog_key, model_id)` 只留一行。
/// 保留“价格字段最多”的那行（旧库常见：一个账号配了价，另一个没配），
/// 平手保留最早平台的行；删掉的 id 不被任何表引用（backends 存的是模型名字符串）。
fn dedupe_models_by_catalog(conn: &Connection) -> AppResult<()> {
    struct Candidate {
        id: String,
        key: String,
        model_id: String,
    }
    let rows: Vec<Candidate> = {
        let mut stmt = conn.prepare(
            "SELECT m.id, m.catalog_key, m.model_id,
                    ((m.input_price IS NOT NULL) + (m.output_price IS NOT NULL)
                     + (m.cache_read_price IS NOT NULL) + (m.cache_write_price IS NOT NULL)
                     + (m.credit_price IS NOT NULL) + (m.peak_pricing IS NOT NULL)) AS score,
                    p.created_at
             FROM models m
             JOIN platforms p ON p.id = m.platform_id
             WHERE m.catalog_key IS NOT NULL
             ORDER BY m.catalog_key, m.model_id, score DESC, p.created_at ASC, m.id ASC",
        )?;
        let mapped = stmt.query_map([], |r| {
            Ok(Candidate { id: r.get(0)?, key: r.get(1)?, model_id: r.get(2)? })
        })?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        if !seen.insert((row.key, row.model_id)) {
            conn.execute("DELETE FROM models WHERE id = ?1", [row.id])?;
        }
    }
    Ok(())
}

/// 加列前先查 pragma：迁移重跑时列已存在就跳过，而不是把启动卡在 duplicate column。
fn add_column_if_missing(conn: &Connection, table: &str, column: &str, ddl: &str) -> AppResult<()> {
    let sql = format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = ?1", table);
    let existing: i64 = conn.query_row(&sql, [column], |row| row.get(0))?;
    if existing == 0 {
        conn.execute(ddl, [])?;
    }
    Ok(())
}

/// V23：请求详情。路径与转发地址是元数据，始终记录；请求头/请求正文/响应正文受采集开关控制。
/// 请求头单独成表：正文超限时仍保存前缀（V27 起），但请求头不该跟着正文的存储策略走。
/// 响应正文同样单独成表：`request_bodies` 是重放入口，语义只限“客户端发来的原始请求”。
const SCHEMA_V23_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS request_headers (
    request_id TEXT PRIMARY KEY,
    headers    TEXT NOT NULL,
    bytes      INTEGER NOT NULL,
    truncated  INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS response_bodies (
    request_id   TEXT PRIMARY KEY,
    protocol     TEXT NOT NULL,
    status_code  INTEGER NOT NULL,
    content_type TEXT,
    body         TEXT NOT NULL,
    bytes        INTEGER NOT NULL,
    truncated    INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);
"#;

/// V24：上游积分额度。`credit_window` 只允许 day/week/month，NULL 表示不启用；
/// V28：私有协议上游的凭据保险箱。
///
/// 为什么不复用 `platforms.api_key`：那个字段是单个字符串，而 Trae / WorkBuddy 的凭据是
/// 结构体（access_token + refresh_token + uid + enterprise_id + domain + 到期时间），
/// 还有独立生命周期（刷新、失效、多账号）。塞进字符串再用 JSON 解析，会让
/// Key 掩码、测试连接、配置导出全部要特判。
///
/// `secret_blob` 是 AES-256-GCM 加密后的 base64（nonce||ciphertext||tag），密钥来自
/// config.toml 的 `encrypt_key`。管理接口只返回元信息，明文永不出进程。
///
/// `allow_self_refresh` 默认 0：Trae 的 ExchangeToken 会轮换整个令牌族，网关一刷，
/// 同账号在官方客户端的登录就静默失效。用户显式打开才自刷。
const SCHEMA_V28_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS platform_credentials (
    platform_id        TEXT PRIMARY KEY,
    secret_blob        TEXT NOT NULL,
    kind               TEXT NOT NULL DEFAULT 'api_key',
    refreshed_at       TEXT,
    failure_count      INTEGER NOT NULL DEFAULT 0,
    last_error         TEXT,
    allow_self_refresh INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL
);
"#;

/// 预算与窗口必须同时存在，避免“有预算但没有重置口径”的半套配置。
/// `platform_credit_state` 是独立台账：清理 request_stats 不能把上游已扣的积分退回来。
const SCHEMA_V24_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS platform_credit_state (
    platform_id  TEXT PRIMARY KEY,
    period_start TEXT NOT NULL,
    spent        REAL NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS platform_credit_ledger (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    platform_id TEXT NOT NULL,
    request_id  TEXT,
    model_id    TEXT,
    delta       REAL NOT NULL,
    kind        TEXT NOT NULL,
    note        TEXT,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_credit_ledger_platform
    ON platform_credit_ledger(platform_id, created_at);
"#;

/// V22：客户端接入的托管状态。managed 是每个客户端适配器自己的 JSON：
/// 应用前/后的非敏感字段、token 摘要、创建过的 Key id、最近一次备份路径。
/// 只存 token 的 sha256，不存明文；配置里的凭据和备份文件才是原文。
const SCHEMA_V22_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS client_configs (
    client_id   TEXT PRIMARY KEY,
    config_path TEXT NOT NULL,
    managed     TEXT NOT NULL,
    token_hash  TEXT,
    key_id      TEXT,
    key_name    TEXT,
    backup_path TEXT,
    applied_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
"#;

/// 请求正文只在显式开启时落库，默认关（见 §2 原则 10"默认不存请求正文"）。
/// 单独一张表而不是给 `request_stats` 加 TEXT 列：绝大多数部署一行正文都不存，
/// 把大字段挂在每条统计行上会让默认路径的表一直变胖。
/// 无外键，与 `request_attempts` 同一口径——正文要能活得比统计行清理更自由，
/// 反过来清理统计时按 `request_id` 一起删（见 db::stats::purge_older_than）。
/// `replayed_from` 存被重放那条 `request_stats.id`：重放产生的新请求必须能看出是重放。
const SCHEMA_V21_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS request_bodies (
    request_id TEXT PRIMARY KEY,
    protocol   TEXT NOT NULL,
    body       TEXT NOT NULL,
    bytes      INTEGER NOT NULL,
    created_at TEXT NOT NULL
);
"#;

/// 预算配额与滚动窗口。两列都可空，空即"不限 / 累计"——V18 那批 Key 迁移后行为一字不变。
/// `quota_window` 存 `day`/`week`/`month`（小写），NULL 就是 V18 的累计口径；
/// 窗口只改变分母统计哪一段 `request_stats`，不需要谁去把已用量清零。
const SCHEMA_V20_MIGRATION: &str = r#"
ALTER TABLE api_keys ADD COLUMN quota_cost REAL;
ALTER TABLE api_keys ADD COLUMN quota_window TEXT;
"#;

/// 通知中心：告警行是"扫描到的当前状态 + 已读/解决状态"，不是事件流水。
/// dedupe_key 唯一（如 `platform_streak:<id>`），同一条件反复扫到只更新 last_seen；
/// 消息存 i18n code + JSON 参数数组而不是成品句子，否则切换语言就读到另一种语言的旧告警。
/// 顺带给 request_stats.api_key_id 建索引：Key 用量和配额告警都按 Key 聚合，此前只有全表扫。
const SCHEMA_V19_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS alerts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    dedupe_key  TEXT NOT NULL UNIQUE,
    kind        TEXT NOT NULL,
    severity    TEXT NOT NULL,
    ref_type    TEXT,
    ref_id      TEXT,
    code        TEXT NOT NULL,
    args        TEXT NOT NULL,
    first_seen  TEXT NOT NULL,
    last_seen   TEXT NOT NULL,
    read_at     TEXT,
    resolved_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_alerts_open ON alerts(resolved_at, last_seen);
CREATE INDEX IF NOT EXISTS idx_stats_api_key ON request_stats(api_key_id);
"#;

/// Key 的过期与配额。三列都可空，空即"不限"——
/// 已有 Key 迁移后行为完全不变，不会因为多了三列就突然被拒。
const SCHEMA_V18_MIGRATION: &str = r#"
ALTER TABLE api_keys ADD COLUMN expires_at TEXT;
ALTER TABLE api_keys ADD COLUMN quota_requests INTEGER;
ALTER TABLE api_keys ADD COLUMN quota_tokens INTEGER;
"#;

/// V17 的目标结构：`key_hash` 用来鉴权，`key_mask` 只用来在列表里认人。
/// 明文列 `key` 直接消失——留着它，"我们只存哈希"就只是一句注释。
const SCHEMA_V17_TABLE: &str = r#"
CREATE TABLE api_keys_target (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    key_hash    TEXT NOT NULL UNIQUE,
    key_mask    TEXT NOT NULL,
    proxy_id    TEXT,
    created_at  TEXT NOT NULL,
    last_used   TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1
);
"#;

const SCHEMA_V17_SWAP: &str = r#"
DROP TABLE api_keys;
ALTER TABLE api_keys_target RENAME TO api_keys;
CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_proxy ON api_keys(proxy_id);
"#;

/// 空库/手工删过表时直接建出目标结构（无数据可搬）
const SCHEMA_V17_EMPTY: &str = r#"
CREATE TABLE IF NOT EXISTS api_keys (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    key_hash    TEXT NOT NULL UNIQUE,
    key_mask    TEXT NOT NULL,
    proxy_id    TEXT,
    created_at  TEXT NOT NULL,
    last_used   TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_proxy ON api_keys(proxy_id);
"#;

/// SQLite 没有 sha256，所以回填只能在 Rust 里做：先读出旧行，算出摘要与掩码，
/// 再在同一个事务里换表。中断重启不会留下"旧表已删、新表半空"的库。
/// 已发出去的 Key 仍然能用（校验时同样对客户端传来的值取摘要），
/// 变的只是"再也读不回明文"——这正是这次迁移的目的。
fn migrate_api_keys_to_hashed(conn: &Connection) -> AppResult<()> {
    let has_plaintext: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('api_keys') WHERE name='key'",
        [],
        |row| row.get(0),
    ).unwrap_or(0);
    if has_plaintext == 0 {
        conn.execute_batch(SCHEMA_V17_EMPTY)?;
        return Ok(());
    }

    let rows: Vec<(String, String, String, Option<String>, String, Option<String>, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT id, name, key, proxy_id, created_at, last_used, enabled FROM api_keys",
        )?;
        let loaded = stmt.query_map([], |row| Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
        )))?.collect::<Result<Vec<_>, _>>()?;
        loaded
    };

    conn.execute_batch("BEGIN;")?;
    let migrated = (|| -> AppResult<()> {
        conn.execute_batch(SCHEMA_V17_TABLE)?;
        for (id, name, plain, proxy_id, created_at, last_used, enabled) in &rows {
            conn.execute(
                "INSERT INTO api_keys_target (id, name, key_hash, key_mask, proxy_id, created_at, last_used, enabled) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                rusqlite::params![
                    id,
                    name,
                    crate::db::api_key::hash_key(plain),
                    crate::db::api_key::key_mask(plain),
                    proxy_id,
                    created_at,
                    last_used,
                    enabled,
                ],
            )?;
        }
        conn.execute_batch(SCHEMA_V17_SWAP)?;
        Ok(())
    })();
    match migrated {
        Ok(()) => conn.execute_batch("COMMIT;")?,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(e);
        }
    }

    // 换表不会销毁旧页：DROP TABLE 只把页标成空闲，明文密钥的字节仍躺在库文件和 -wal 里。
    // 所以回填后做一次 VACUUM 重写库文件——否则"库里只存摘要"只在读接口成立，
    // 拿到 .db 的人还是能捞回历史明文。这是一次性代价（只在这条迁移里跑）。
    // 顺序有讲究：WAL 模式下 VACUUM 的新页先落在 -wal，所以必须再 checkpoint 把它们折回主文件；
    // 反过来先 checkpoint 等于亲手把明文页刷进 .db，再 VACUUM 也来不及了。
    conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;

    tracing::info!("api_keys 明文密钥已换成哈希，共迁移 {} 条", rows.len());
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
    name        TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS routes (
    id              TEXT PRIMARY KEY,
    proxy_id        TEXT NOT NULL UNIQUE REFERENCES proxies(id) ON DELETE CASCADE,
    lb_strategy     TEXT NOT NULL DEFAULT 'RoundRobin',
    retry_policy    TEXT NOT NULL DEFAULT '{"max_retries":2,"retry_on_error":["RateLimit","ServerError","Timeout"],"backoff_ms":500}',
    fallback        TEXT
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

CREATE TABLE IF NOT EXISTS api_keys (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    key         TEXT NOT NULL UNIQUE,
    proxy_id    TEXT REFERENCES proxies(id) ON DELETE SET NULL,
    created_at  TEXT NOT NULL,
    last_used   TEXT
);

CREATE INDEX IF NOT EXISTS idx_api_keys_key ON api_keys(key);
CREATE INDEX IF NOT EXISTS idx_api_keys_proxy ON api_keys(proxy_id);
"#;

// Migration from V1 to V2: remove listen_port and status from proxies table
const SCHEMA_V2_MIGRATION: &str = r#"
-- Migrate proxies table: remove listen_port and status columns
CREATE TABLE IF NOT EXISTS proxies_new (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    protocols   TEXT NOT NULL DEFAULT '["OpenAI"]',
    auth_token  TEXT,
    created_at  TEXT NOT NULL
);

INSERT OR IGNORE INTO proxies_new (id, name, protocols, auth_token, created_at)
    SELECT id, name, '["OpenAI"]', NULL, created_at FROM proxies;

DROP TABLE IF EXISTS proxies;
ALTER TABLE proxies_new RENAME TO proxies;
"#;

// Migration V3: add api_keys table
const SCHEMA_V3_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS api_keys (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    key         TEXT NOT NULL UNIQUE,
    proxy_id    TEXT REFERENCES proxies(id) ON DELETE SET NULL,
    created_at  TEXT NOT NULL,
    last_used   TEXT
);

CREATE INDEX IF NOT EXISTS idx_api_keys_key ON api_keys(key);
CREATE INDEX IF NOT EXISTS idx_api_keys_proxy ON api_keys(proxy_id);
"#;

// Migration V4: simplify proxies (remove protocols, auth_token), simplify routes (remove virtual_model, proxy_id UNIQUE)
const SCHEMA_V4_MIGRATION: &str = r#"
-- Simplify proxies table: remove protocols and auth_token columns, add UNIQUE on name
CREATE TABLE IF NOT EXISTS proxies_new (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL
);

INSERT OR IGNORE INTO proxies_new (id, name, created_at)
    SELECT id, name, created_at FROM proxies;

DROP TABLE IF EXISTS proxies;
ALTER TABLE proxies_new RENAME TO proxies;

-- Simplify routes table: remove virtual_model, make proxy_id UNIQUE (1:1)
CREATE TABLE IF NOT EXISTS routes_new (
    id              TEXT PRIMARY KEY,
    proxy_id        TEXT NOT NULL UNIQUE REFERENCES proxies(id) ON DELETE CASCADE,
    lb_strategy     TEXT NOT NULL DEFAULT 'RoundRobin',
    retry_policy    TEXT NOT NULL DEFAULT '{"max_retries":2,"retry_on_error":["RateLimit","ServerError","Timeout"],"backoff_ms":500}',
    fallback        TEXT
);

INSERT OR IGNORE INTO routes_new (id, proxy_id, lb_strategy, retry_policy, fallback)
    SELECT id, proxy_id, lb_strategy, retry_policy, fallback FROM routes;

DROP TABLE IF EXISTS routes;
ALTER TABLE routes_new RENAME TO routes;
"#;

// Migration V5: backends.model_id now stores the actual model ID string (e.g. "gpt-4o") instead of FK to models table
// Added capabilities column to backends
const SCHEMA_V5_MIGRATION: &str = r#"
-- Rebuild backends table: model_id stores model string directly, add capabilities column
CREATE TABLE IF NOT EXISTS backends_new (
    id              TEXT PRIMARY KEY,
    route_id        TEXT NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    platform_id     TEXT NOT NULL REFERENCES platforms(id),
    model_id        TEXT NOT NULL,
    weight          INTEGER NOT NULL DEFAULT 1,
    priority        INTEGER NOT NULL DEFAULT 0,
    max_concurrent  INTEGER,
    capabilities    TEXT NOT NULL DEFAULT '[]',
    status          TEXT NOT NULL DEFAULT 'Active'
);

-- Copy data, converting model_id from UUID reference to actual model_id string
INSERT OR IGNORE INTO backends_new (id, route_id, platform_id, model_id, weight, priority, max_concurrent, capabilities, status)
    SELECT b.id, b.route_id, b.platform_id,
           COALESCE(m.model_id, b.model_id),
           b.weight, b.priority, b.max_concurrent,
           COALESCE(m.capabilities, '[]'),
           b.status
    FROM backends b
    LEFT JOIN models m ON b.model_id = m.id;

DROP TABLE IF EXISTS backends;
ALTER TABLE backends_new RENAME TO backends;
"#;

// Migration V6: per-backend forward params (param_overrides / unknown_params) + per-route default_max_tokens
const SCHEMA_V6_MIGRATION: &str = r#"
-- routes: 客户端未传 max_tokens 时填充的默认值
ALTER TABLE routes ADD COLUMN default_max_tokens INTEGER;

-- backends: 转发前 merge 进请求体的参数覆盖（JSON object）
ALTER TABLE backends ADD COLUMN param_overrides TEXT;

-- backends: 未知参数转发策略（Auto=按平台自动 / Drop=丢弃 / Passthrough=透传）
ALTER TABLE backends ADD COLUMN unknown_params TEXT NOT NULL DEFAULT 'Auto';
"#;

// Migration V7: 性能指标采集——首字时长（TTFT）与流式标记
// ttft_ms: 流式请求从发出到收到上游第一个 chunk 的耗时；非流式为 NULL
// is_stream: 是否流式请求（流式的 latency_ms 在流结束时改写为完整耗时，与口径统一）
const SCHEMA_V7_MIGRATION: &str = r#"
ALTER TABLE request_stats ADD COLUMN ttft_ms INTEGER;
ALTER TABLE request_stats ADD COLUMN is_stream INTEGER NOT NULL DEFAULT 0;
"#;

// Migration V8: 缓存命中统计
// cache_read_tokens: 命中上游提示词缓存的 token 数（OpenAI prompt_tokens_details.cached_tokens /
//   DeepSeek prompt_cache_hit_tokens / Anthropic cache_read_input_tokens，归一为同一字段）
// cache_write_tokens: 缓存写入 token 数（目前仅 Anthropic cache_creation_input_tokens 有此概念）
const SCHEMA_V8_MIGRATION: &str = r#"
ALTER TABLE request_stats ADD COLUMN cache_read_tokens INTEGER;
ALTER TABLE request_stats ADD COLUMN cache_write_tokens INTEGER;
"#;

// Migration V9: 思考参数适配
// reasoning_style: Auto / OpenAiEffort / AnthropicThinking / OpenRouter / Qwen / Glm / VllmQwen / Disabled
// reasoning_echo: OpenAI assistant reasoning_content 回传策略（Drop / Keep / Auto）
const SCHEMA_V9_MIGRATION: &str = r#"
ALTER TABLE backends ADD COLUMN reasoning_style TEXT NOT NULL DEFAULT 'Auto';
ALTER TABLE backends ADD COLUMN reasoning_echo TEXT NOT NULL DEFAULT 'Drop';
"#;

// Migration V10: 管理端登录——用户与会话
// password_hash: Argon2id PHC 字符串
// sessions.token_hash: 会话 token 的 SHA-256（cookie 中只保存原始 token，DB 不落明文）
const SCHEMA_V10_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id              TEXT PRIMARY KEY,
    username        TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS sessions (
    token_hash      TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at      TEXT NOT NULL,
    expires_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);
"#;

// Migration V11: API Key 生命周期——启用/禁用
const SCHEMA_V11_MIGRATION: &str = r#"
ALTER TABLE api_keys ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;
"#;

// Migration V12: 密钥维度用量——请求统计记录发起方 API Key（旧行为 NULL，不参与密钥聚合）
const SCHEMA_V12_MIGRATION: &str = r#"
ALTER TABLE request_stats ADD COLUMN api_key_id TEXT;
CREATE INDEX IF NOT EXISTS idx_request_stats_api_key ON request_stats(api_key_id, created_at);
"#;

// Migration V13: 平台健康历史——每次连通性测试落一行，列表页 CHECK 列读最后一次而不是本次会话
// 不建外键：平台删除后历史行留着（与 request_stats 的悬挂行同一口径），聚合从 platforms 侧 JOIN 自然过滤
const SCHEMA_V13_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS platform_health (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    platform_id TEXT NOT NULL,
    ok          INTEGER NOT NULL,
    latency_ms  INTEGER NOT NULL,
    model_count INTEGER NOT NULL DEFAULT 0,
    error       TEXT,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_platform_health_platform ON platform_health(platform_id, created_at);
"#;

// Migration V14: 绑定关系不再随虚拟模型一起消失
// api_keys.proxy_id 原本带 ON DELETE SET NULL：删除虚拟模型会把绑定 Key 静默升级成全局凭据。
// 这里重建表去掉该外键（保留 proxy_id 作为历史绑定，与其他悬挂数据同一口径），
// 再把早已指向不存在模型的 Key 一律停用——它们本来就鉴权失败，enabled=1 只是状态造假。
const SCHEMA_V14_MIGRATION: &str = r#"
DROP TABLE IF EXISTS api_keys_v14;
CREATE TABLE api_keys_v14 (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    key         TEXT NOT NULL UNIQUE,
    proxy_id    TEXT,
    created_at  TEXT NOT NULL,
    last_used   TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1
);
INSERT INTO api_keys_v14 (id, name, key, proxy_id, created_at, last_used, enabled)
    SELECT id, name, key, proxy_id, created_at, last_used, enabled FROM api_keys;
DROP TABLE api_keys;
ALTER TABLE api_keys_v14 RENAME TO api_keys;
CREATE INDEX IF NOT EXISTS idx_api_keys_key ON api_keys(key);
CREATE INDEX IF NOT EXISTS idx_api_keys_proxy ON api_keys(proxy_id);

UPDATE api_keys SET enabled = 0
WHERE enabled = 1 AND proxy_id IS NOT NULL
  AND proxy_id NOT IN (SELECT id FROM proxies);
"#;

// Migration V15: 单请求成本估算落库
// 口径：models.input_price / output_price 按"每 100 万 token"计，两侧都有价才算得出成本，
// 只填一侧或都没填 → 整单留 NULL（不做半截估算）。写入时算一次并冻结在行里，
// 之后改价不会回头重写历史（所以旧行一律 NULL，不回填）。
const SCHEMA_V15_MIGRATION: &str = r#"
ALTER TABLE request_stats ADD COLUMN cost REAL;
"#;

// Migration V16: 重试轨迹
// 一条 request_stats 只记最终那次尝试，重试/fallback 依次试过哪些后端因此不可见。
// request_id 由网关在收到请求时生成并写进最终行，尝试明细另存 request_attempts（一次尝试一行）。
// 不建外键：后端/平台被删后轨迹仍要能读（与 platform_health、request_stats 悬挂行同一口径），
// 所以关键展示字段（model_id）直接冗余在尝试行里。
// 不存请求/响应正文；error_message 落库前截断，且绝不包含密钥。
// 旧行 request_id 为 NULL → 接口返回空轨迹，前端标注"历史数据，轨迹不可用"，不伪造。
const SCHEMA_V16_MIGRATION: &str = r#"
ALTER TABLE request_stats ADD COLUMN request_id TEXT;

CREATE INDEX IF NOT EXISTS idx_request_stats_request_id ON request_stats(request_id);

CREATE TABLE IF NOT EXISTS request_attempts (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id    TEXT NOT NULL,
    attempt_no    INTEGER NOT NULL,
    backend_id    TEXT,
    platform_id   TEXT,
    model_id      TEXT,
    status_code   INTEGER,
    error_type    TEXT,
    error_message TEXT,
    latency_ms    INTEGER,
    started_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_request_attempts_request ON request_attempts(request_id, attempt_no);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db(tag: &str) -> (crate::db::DbPool, std::path::PathBuf) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-db");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let path = dir.join(format!("schema-{}-{}.db", tag, uuid::Uuid::new_v4()));
        let pool = crate::db::init_pool(&path).expect("init test db");
        (pool, path)
    }

    fn cleanup_db(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    /// 常量必须跟着迁移终点走：加了 V20 却忘了改常量，备份还原会拿旧上限拦掉新快照。
    #[test]
    fn current_version_constant_tracks_migrations() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        run_migrations(&conn).expect("migrations run on a fresh db");
        let reached = version(&conn);
        assert_eq!(
            reached, CURRENT_SCHEMA_VERSION,
            "run_migrations 跑到 V{reached}，常量写的是 V{CURRENT_SCHEMA_VERSION}"
        );
    }

    /// V17 回填：迁移前发出去的 Key 迁移后照样能鉴权，而库里已经读不出明文。
    /// （"能读出来"从来不是问题——问题是它一旦被列表接口原样带回，就等于凭据外泄。）
    #[test]
    fn v17_backfill_keeps_existing_keys_working() {
        let (pool, path) = test_db("v17");
        let plain = "sk-legacy-plaintext-secret-value-1234";
        {
            let conn = pool.get().expect("conn");
            // 退回到迁移前的形状：明文列 + 版本 16
            conn.execute_batch(
                "DROP TABLE api_keys;
                 CREATE TABLE api_keys (
                     id TEXT PRIMARY KEY, name TEXT NOT NULL, key TEXT NOT NULL UNIQUE,
                     proxy_id TEXT, created_at TEXT NOT NULL, last_used TEXT,
                     enabled INTEGER NOT NULL DEFAULT 1
                 );
                 INSERT INTO api_keys (id, name, key, proxy_id, created_at, enabled)
                     VALUES ('k1', 'legacy', 'sk-legacy-plaintext-secret-value-1234', 'p1', '2026-01-01T00:00:00Z', 1);
                 INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 16);"
            ).expect("roll back to V16 shape");

            run_migrations(&conn).expect("migrate to V17");

            let version: i32 = conn.query_row(
                "SELECT value FROM _schema_version WHERE id = 1", [], |r| r.get(0)).unwrap();
            assert!(version >= 17, "迁移链至少要走完 V17");
            let plaintext_cols: i64 = conn.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('api_keys') WHERE name='key'",
                [], |r| r.get(0)).unwrap();
            assert_eq!(plaintext_cols, 0, "明文列必须已经消失");
            // V18 的三列跟着一起补上，回填出来的老行必须还是"不过期 + 不限"，
            // 否则单纯加列就会让迁移前发的 Key 突然被拒。
            assert_eq!(conn.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('api_keys') \
                 WHERE name IN ('expires_at','quota_requests','quota_tokens')",
                [], |r| r.get::<_, i64>(0)).unwrap(), 3);
            assert!(conn.query_row(
                "SELECT expires_at IS NULL AND quota_requests IS NULL AND quota_tokens IS NULL \
                 FROM api_keys WHERE id='k1'", [], |r| r.get::<_, i64>(0)).unwrap() != 0);
        }

        let migrated = crate::db::api_key::get_by_key(&pool, plain).unwrap()
            .expect("旧 Key 迁移后仍能按明文摘要命中");
        assert_eq!(migrated.id, "k1");
        assert_eq!(migrated.proxy_id.as_deref(), Some("p1"));
        assert!(migrated.enabled);
        assert!(crate::db::api_key::validate_any(&pool, plain).unwrap());
        assert!(!crate::db::api_key::validate_any(&pool, &migrated.key_mask).unwrap());

        // 幂等：再跑一次迁移不能报错，也不能再去找明文列
        run_migrations(&pool.get().unwrap()).expect("V17 迁移可重入");
        assert!(crate::db::api_key::validate_any(&pool, plain).unwrap());

        // 迁移不只是"读不出明文"，落盘字节也要真的消失：
        // DROP TABLE 只把页标成空闲，不 VACUUM 的话拿到 .db 的人还能捞回历史明文。
        for suffix in ["", "-wal"] {
            let bytes = std::fs::read(format!("{}{}", path.display(), suffix)).unwrap_or_default();
            assert!(!printable(&bytes).contains(plain), "{suffix} 里仍残留明文");
        }

        cleanup_db(&path);
    }

    /// V21 半途失败要能重跑：`execute_batch` 逐语句自动提交，建表成功而加列失败时版本号仍停在 20。
    /// 再启动时表已在、列也在，SQLite 又没有 ADD COLUMN IF NOT EXISTS——不加守卫就再也起不来。
    #[test]
    fn v21_reruns_after_a_partial_migration() {
        let (pool, path) = test_db("v21-partial");
        {
            let conn = pool.get().expect("conn");
            conn.execute("INSERT OR REPLACE INTO _schema_version (id, value) VALUES (1, 20)", [])
                .expect("rewind version to 20");
        }

        run_migrations(&pool.get().unwrap()).expect("V21 半途失败后可重跑");

        let conn = pool.get().unwrap();
        assert_eq!(version(&conn), CURRENT_SCHEMA_VERSION, "重跑要把版本号补到终点");
        assert_eq!(conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('request_stats') WHERE name='replayed_from'",
            [], |r| r.get::<_, i64>(0)).unwrap(), 1, "跳过 ALTER 不等于列不存在");
        assert_eq!(conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='request_bodies'",
            [], |r| r.get::<_, i64>(0)).unwrap(), 1);
        cleanup_db(&path);
    }

    /// V26 只加一列：旧行回填成 NULL（"当时没有这个读数"），不能补 0 说"没省到钱"
    #[test]
    fn v26_adds_cache_savings_as_null_for_history() {
        let (pool, path) = test_db("v26");
        {
            let conn = pool.get().expect("conn");
            conn.execute(
                "INSERT INTO request_stats (proxy_id, route_id, backend_id, status_code, latency_ms, created_at) \
                 VALUES ('p','r','b',200,1,'2026-01-01T00:00:00Z')",
                [],
            ).expect("legacy row");
            conn.execute("UPDATE _schema_version SET value = 25 WHERE id = 1", [])
                .expect("rewind to 25");
            run_migrations(&conn).expect("V26 runs alone");
            assert_eq!(version(&conn), CURRENT_SCHEMA_VERSION);
            assert_eq!(conn.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('request_stats') WHERE name='cache_savings'",
                [], |r| r.get::<_, i64>(0)).unwrap(), 1, "列必须已经加上");
            let savings: Option<f64> = conn.query_row(
                "SELECT cache_savings FROM request_stats LIMIT 1", [], |r| r.get(0)).unwrap();
            assert_eq!(savings, None, "历史行是未知，不是 0");
        }
        cleanup_db(&path);
    }

    /// V27 只加一列：已有正文都是完整写入的，迁移后必须是 truncated=0
    #[test]
    fn v27_marks_existing_bodies_as_complete() {
        let (pool, path) = test_db("v27");
        {
            let conn = pool.get().expect("conn");
            conn.execute(
                "INSERT INTO request_bodies (request_id, protocol, body, bytes, created_at) \
                 VALUES ('req-old', 'openai', '{}', 2, '2026-01-01T00:00:00Z')",
                [],
            ).expect("legacy body");
            conn.execute("UPDATE _schema_version SET value = 26 WHERE id = 1", [])
                .expect("rewind to 26");
            run_migrations(&conn).expect("V27 runs alone");
            assert_eq!(version(&conn), CURRENT_SCHEMA_VERSION);
            assert_eq!(conn.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('request_bodies') WHERE name='truncated'",
                [], |r| r.get::<_, i64>(0)).unwrap(), 1, "列必须已经加上");
            let truncated: i64 = conn.query_row(
                "SELECT truncated FROM request_bodies WHERE request_id = 'req-old'", [], |r| r.get(0)).unwrap();
            assert_eq!(truncated, 0, "老正文是完整写入的，不能标成截断");
        }
        cleanup_db(&path);
    }

    /// 把库文件字节压成"可打印字符 + NUL"的字符串，用来粗判残留明文（跨页会被打断，够用）
    fn printable(bytes: &[u8]) -> String {
        bytes.iter().map(|b| if b.is_ascii_graphic() { *b as char } else { '\0' }).collect()
    }
}
