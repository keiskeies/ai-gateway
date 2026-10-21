//! 整库快照与原地还原
//!
//! 为什么不直接复制 `.db` 文件：库跑在 WAL 模式下，最后几笔事务可能还留在 `-wal` 里，
//! 裸拷主文件会得到一份"少了最近请求"的快照，而且拷的过程中正在写入还可能读出撕裂页。
//! `VACUUM INTO` 走正常事务读，出来的文件天然一致，顺手把碎片压实；
//! 还原用 SQLite 的在线备份接口（`rusqlite::Connection::restore`）把快照灌回当前库，
//! 进程不用停——Windows 上正在打开的 `.db` 根本换不掉文件，只能走这条路。
//!
//! 快照只是文件，不带任何"我保证能还原"的承诺，所以还原前先 `PRAGMA quick_check`，
//! 并在动手之前对**当前**库补一份保险快照：还原失败或还原错了还能倒回去。

use std::path::{Path, PathBuf};

use rusqlite::{Connection, DatabaseName, OpenFlags};

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::backup::{BackupInfo, BackupList, RestoreResult};

const PREFIX: &str = "aigw-";
const SUFFIX: &str = ".db";
/// 还原时等锁的上限：本地图形界面只有一两个人在点，5 秒还拿不到写锁就是真有人在跑请求
const RESTORE_BUSY_MS: u64 = 5_000;

/// 快照目录：数据库文件同级的 `backups/`。放在同一目录里，用户搬库时不会漏掉备份。
pub fn backups_dir(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .join("backups")
}

/// 文件名同时是排序键，所以用 UTC 时间戳：跨时区的机器之间比较才有意义
fn snapshot_name(now: chrono::DateTime<chrono::Utc>, seq: u32) -> String {
    let stamp = now.format("%Y%m%d-%H%M%S");
    if seq == 0 {
        format!("{PREFIX}{stamp}{SUFFIX}")
    } else {
        format!("{PREFIX}{stamp}-{seq}{SUFFIX}")
    }
}

/// 只认 `aigw-<字母数字-_>.db`：路径分隔符和多余的点一律拒绝，
/// 这样 `{name}` 这个路径参数拼进 fs 之前就已经挡掉了目录穿越。
fn valid_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(SUFFIX) else {
        return false;
    };
    let Some(rest) = stem.strip_prefix(PREFIX) else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn resolve(db_path: &Path, name: &str) -> AppResult<PathBuf> {
    if !valid_name(name) {
        return Err(AppError::BadRequest(format!(
            "Invalid backup name; expected {PREFIX}<timestamp>{SUFFIX}"
        )));
    }
    Ok(backups_dir(db_path).join(name))
}

fn file_time(ts: std::time::SystemTime) -> String {
    let at = chrono::DateTime::<chrono::Utc>::from(ts);
    at.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// 读快照里的表结构版本。打不开返回 None（坏文件或不是 SQLite）；
/// 打得开但查不到 `_schema_version` 是 0，等于"这份快照早于迁移体系"，同样只能重启后才知道能不能用。
fn schema_version_of(path: &Path) -> Option<i64> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    Some(crate::db::schema::version(&conn))
}

/// 一致性检查：`PRAGMA quick_check` 只回一行，"ok" 才算过。
/// 用只读连接，免得一个坏文件在检查过程中被我们的句柄改掉。
pub fn quick_check(path: &Path) -> AppResult<Result<(), String>> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| AppError::BadRequest(format!("Cannot open backup: {e}")))?;
    let report = conn
        .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
        .unwrap_or_else(|_| "no result".to_string());
    if report == "ok" {
        Ok(Ok(()))
    } else {
        Ok(Err(report))
    }
}

fn info_of(path: &Path) -> AppResult<BackupInfo> {
    let meta = std::fs::metadata(path)?;
    let created_at = meta
        .modified()
        .map(file_time)
        .unwrap_or_else(|_| "unknown".to_string());
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(BackupInfo {
        size_bytes: meta.len(),
        created_at,
        schema_version: schema_version_of(path),
        name,
    })
}

/// 拍一份快照。同一秒内重复点会撞名（`VACUUM INTO` 遇到已存在的目标直接报错），
/// 所以往后加序号，而不是把用户刚拍的那份覆盖掉。
pub fn create(pool: &DbPool, db_path: &Path) -> AppResult<BackupInfo> {
    let dir = backups_dir(db_path);
    std::fs::create_dir_all(&dir)?;
    let now = chrono::Utc::now();
    let mut seq = 0u32;
    let path = loop {
        let candidate = dir.join(snapshot_name(now, seq));
        if !candidate.exists() {
            break candidate;
        }
        seq += 1;
        if seq > 999 {
            return Err(AppError::Internal(
                "Too many backups taken in the same second".into(),
            ));
        }
    };
    let conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
    // VACUUM INTO 的目标必须是尚不存在的文件，参数走绑定变量而不是拼字符串
    conn.execute("VACUUM INTO ?1", rusqlite::params![path.to_string_lossy()])?;
    info_of(&path)
}

/// 列出快照，新的在前。目录不存在=还没备份过，是正常状态而不是错误。
pub fn list(db_path: &Path) -> AppResult<BackupList> {
    let dir = backups_dir(db_path);
    let mut items: Vec<BackupInfo> = Vec::new();
    if dir.is_dir() {
        let entries = std::fs::read_dir(&dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !path.is_file() || !valid_name(&name) {
                continue;
            }
            if let Ok(info) = info_of(&path) {
                items.push(info);
            }
        }
    }
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let total_bytes = items.iter().map(|i| i.size_bytes).sum();
    Ok(BackupList {
        items,
        dir: dir.to_string_lossy().to_string(),
        total_bytes,
    })
}

/// 最近一份快照的时间（unix 秒）。"备份过期"告警用它，所以坏文件也算数——
/// 用户需要知道"最后一次动备份是什么时候"，至于那份备份好不好，quick_check 在还原时拦。
pub fn latest_snapshot_time(db_path: &Path) -> Option<i64> {
    let dir = backups_dir(db_path);
    let entries = std::fs::read_dir(dir).ok()?;
    let mut latest: Option<i64> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !valid_name(&name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let Ok(ts) = meta.modified() else { continue };
        let secs = ts
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        if latest.map(|cur| secs > cur).unwrap_or(true) {
            latest = Some(secs);
        }
    }
    latest
}

pub fn delete(db_path: &Path, name: &str) -> AppResult<bool> {
    let path = resolve(db_path, name)?;
    if !path.is_file() {
        return Ok(false);
    }
    std::fs::remove_file(&path)?;
    Ok(true)
}

/// 快照文件路径（下载用），同样过名字校验
pub fn path_for(db_path: &Path, name: &str) -> AppResult<PathBuf> {
    let path = resolve(db_path, name)?;
    if !path.is_file() {
        return Err(AppError::NotFound(format!("backup {name}")));
    }
    Ok(path)
}

/// 原地还原：校验 → 保险快照 → 灌回当前库。
/// 快照版本比当前程序新就直接拒绝，那种文件是留给以后的程序用的。
pub fn restore(pool: &DbPool, db_path: &Path, name: &str) -> AppResult<RestoreResult> {
    let path = path_for(db_path, name)?;
    match quick_check(&path)? {
        Ok(()) => {}
        Err(report) => {
            return Err(AppError::BadRequest(format!(
                "Backup failed quick_check: {report}"
            )))
        }
    }
    let backup_version = schema_version_of(&path);
    let supported = crate::db::schema::CURRENT_SCHEMA_VERSION;
    // 比的是"本机程序读得懂到哪个结构"，不是当前库现在的版本：刚还原完一份旧备份，
    // 当前库也一起变旧了，拿它当基准会把"倒回刚才那份安全快照"这条路同时堵死。
    if let Some(v) = backup_version {
        if v > supported {
            return Err(AppError::BadRequest(format!(
                "Backup schema version {v} is newer than this build ({supported})"
            )));
        }
    }

    let safety = create(pool, db_path)?;

    {
        let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
        conn.busy_timeout(std::time::Duration::from_millis(RESTORE_BUSY_MS))
            .map_err(|e| AppError::Internal(e.to_string()))?;
        conn.restore(DatabaseName::Main, &path, None::<fn(rusqlite::backup::Progress)>)
            .map_err(|e| {
                AppError::Internal(format!(
                    "Restore failed ({e}); the state before restoring is saved in {}",
                    safety.name
                ))
            })?;
    }

    Ok(RestoreResult {
        // 结构与本机程序不一致就要重启：迁移只在启动时跑，旧库得重启才会补到当前版本
        needs_restart: backup_version != Some(supported),
        current_version: supported,
        backup_version,
        restored_from: name.to_string(),
        safety_backup: safety.name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> (DbPool, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "aigw-backup-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("test.db");
        let pool = crate::db::init_pool(&path).expect("init pool");
        (pool, path)
    }

    fn insert_proxy(pool: &DbPool, id: &str) {
        let conn = pool.get().unwrap();
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        conn.execute(
            "INSERT INTO proxies (id, name, created_at) VALUES (?1, ?1, ?2)",
            rusqlite::params![id, now],
        )
        .unwrap();
    }

    fn count_proxies(pool: &DbPool) -> i64 {
        let conn = pool.get().unwrap();
        conn.query_row("SELECT COUNT(*) FROM proxies", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn name_validation_blocks_paths_and_stray_files() {
        assert!(valid_name("aigw-20261021-010203.db"));
        assert!(valid_name("aigw-20261021-010203-1.db"));
        assert!(!valid_name("../aigw-x.db"), "目录穿越");
        assert!(!valid_name("aigw-x.db.txt"), "伪装成快照的后缀");
        assert!(!valid_name("db.sqlite"), "别人的文件");
        assert!(!valid_name("AIGW-x.db"), "大小写也要挡");
        assert!(!valid_name("aigw-.db"), "时间戳不能为空");
        assert!(!valid_name("aigw-x\\y.db"), "反斜杠也是分隔符");
    }

    #[test]
    fn snapshot_covers_wal_writes_and_restore_brings_them_back() {
        let (pool, path) = temp_db("roundtrip");

        insert_proxy(&pool, "keep");
        let live = crate::db::schema::version(&pool.get().unwrap());
        let supported = crate::db::schema::CURRENT_SCHEMA_VERSION;
        assert_eq!(live, supported, "新建库应迁移到本机版本");
        let info = create(&pool, &path).expect("create snapshot");
        assert!(valid_name(&info.name));
        assert!(info.size_bytes > 0);
        assert_eq!(info.schema_version, Some(supported), "快照要带上当前表结构版本");

        // 快照之后再写：WAL 模式下这些行只留在 -wal 里
        insert_proxy(&pool, "gone");
        assert_eq!(count_proxies(&pool), 2);

        let files = list(&path).unwrap();
        assert_eq!(files.items.len(), 1);
        assert_eq!(files.total_bytes, info.size_bytes);
        assert!(files.dir.ends_with("backups"));

        {
            let conn = pool.get().unwrap();
            conn.execute("DELETE FROM proxies WHERE id = 'gone'", [])
                .unwrap();
            conn.execute("DELETE FROM proxies WHERE id = 'keep'", [])
                .unwrap();
        }
        assert_eq!(count_proxies(&pool), 0);

        let result = restore(&pool, &path, &info.name).expect("restore");
        assert_eq!(result.restored_from, info.name);
        assert!(!result.needs_restart, "同版本还原不该提示重启");
        assert_eq!(count_proxies(&pool), 1, "还原要回到快照那一刻");

        // 结构落后的快照：能还原，但必须说清要重启（迁移只在启动时跑）
        let older = backups_dir(&path).join("aigw-older.db");
        std::fs::copy(backups_dir(&path).join(&info.name), &older).expect("copy snapshot");
        {
            let conn = Connection::open(&older).unwrap();
            conn.execute(
                "UPDATE _schema_version SET value = ?1 WHERE id = 1",
                rusqlite::params![supported - 1],
            )
            .unwrap();
        }
        let back = restore(&pool, &path, "aigw-older.db").expect("旧结构的快照也要能还原");
        assert_eq!(back.backup_version, Some(supported - 1));
        assert!(back.needs_restart, "结构落后本机要提示重启");

        // 这才是保险快照的意义：当前库已经被灌旧了，仍要能把新版本的原始快照倒回去
        let forward = restore(&pool, &path, &info.name)
            .expect("倒回当前版本的快照不该被版本比较挡住");
        assert!(!forward.needs_restart);
        assert_eq!(count_proxies(&pool), 1, "倒回去之后数据还是那份的");

        // 原始快照 1 份 + 造出来的旧结构快照 1 份 + 三次还原各补 1 份保险快照
        let after = list(&path).unwrap();
        assert_eq!(after.items.len(), 5, "实际列表：{:?}", after.items.iter().map(|i| &i.name).collect::<Vec<_>>());
        assert!(after.items.iter().any(|i| i.name == result.safety_backup));

        drop(pool);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn restore_refuses_junk_and_newer_schemas() {
        let (pool, path) = temp_db("refuse");

        let dir = backups_dir(&path);
        std::fs::create_dir_all(&dir).unwrap();
        let junk = dir.join("aigw-junk.db");
        std::fs::write(&junk, b"this is not a database").unwrap();
        let err = restore(&pool, &path, "aigw-junk.db").unwrap_err();
        assert!(
            format!("{err:?}").contains("quick_check"),
            "坏文件要在动手之前被挡下：{err:?}"
        );
        assert_eq!(count_proxies(&pool), 0, "被挡下不该改动当前库");

        // 名字不合规矩的直接 400，连文件都不用看
        assert!(restore(&pool, &path, "../test.db").is_err());
        assert!(path_for(&path, "aigw-missing.db").is_err());

        // 比当前程序更新的结构属于以后的程序，还原进来只会让本机代码读不懂库
        let newer = dir.join("aigw-newer.db");
        let ahead = crate::db::schema::CURRENT_SCHEMA_VERSION + 1;
        {
            let conn = Connection::open(&newer).unwrap();
            conn.execute("CREATE TABLE _schema_version (id INTEGER PRIMARY KEY, value INTEGER NOT NULL)", [])
                .unwrap();
            conn.execute(
                "INSERT INTO _schema_version (id, value) VALUES (1, ?1)",
                rusqlite::params![ahead],
            )
            .unwrap();
        }
        let err = restore(&pool, &path, "aigw-newer.db").unwrap_err();
        assert!(
            format!("{err:?}").contains("newer"),
            "要说明是版本问题：{err:?}"
        );

        drop(pool);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn latest_time_tracks_only_snapshot_files() {
        let (pool, path) = temp_db("latest");
        assert!(latest_snapshot_time(&path).is_none(), "没有 backups 目录就是没备份过");

        std::fs::create_dir_all(backups_dir(&path)).unwrap();
        std::fs::write(backups_dir(&path).join("notes.txt"), b"x").unwrap();
        assert!(
            latest_snapshot_time(&path).is_none(),
            "无关文件不该被当成备份"
        );

        let info = create(&pool, &path).unwrap();
        let latest = latest_snapshot_time(&path).expect("刚拍过快照");
        let now = chrono::Utc::now().timestamp();
        assert!(
            latest <= now + 5 && latest >= now - 3600,
            "{} 取的是快照时间：{}",
            info.name,
            latest
        );

        drop(pool);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
