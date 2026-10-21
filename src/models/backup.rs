use serde::Serialize;

/// 一个整库快照文件。只描述磁盘上的文件，不代表它一定可以还原——
/// `schema_version` 读不出来通常说明文件坏了或不是 SQLite。
#[derive(Debug, Clone, Serialize)]
pub struct BackupInfo {
    pub name: String,
    pub size_bytes: u64,
    /// 文件写入时间（UTC，秒精度 RFC3339）
    pub created_at: String,
    /// 快照里的 `_schema_version`
    pub schema_version: Option<i64>,
}

/// `GET /api/backups`
#[derive(Debug, Clone, Serialize)]
pub struct BackupList {
    pub items: Vec<BackupInfo>,
    /// 快照目录绝对路径：本机用户直接去拿文件比走浏览器下载快
    pub dir: String,
    pub total_bytes: u64,
}

/// `POST /api/backups/{name}/restore` 的结果
#[derive(Debug, Clone, Serialize)]
pub struct RestoreResult {
    pub restored_from: String,
    /// 还原前对当前库自动补的那份保险快照，restore 失败时还能倒回去
    pub safety_backup: String,
    /// 还原后必须重启：快照的结构与本机程序不一致，迁移只在启动时跑
    pub needs_restart: bool,
    /// 本机程序支持到的结构版本
    pub current_version: i64,
    pub backup_version: Option<i64>,
}
