use serde::Serialize;

/// 一条通知中心里的告警。行代表"某个条件当前的状态 + 用户读到没有"，
/// 不是事件流水：同一条件反复扫到只移动 `last_seen`。
#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    pub id: i64,
    /// 去重键，例如 `platform_streak:<platform_id>`；前端按它标记已读
    pub dedupe_key: String,
    pub kind: String,
    /// `warn` / `error`
    pub severity: String,
    /// 用于深链的引用类型：`platform` / `proxy` / `api_key`
    pub ref_type: Option<String>,
    pub ref_id: Option<String>,
    /// 前端 i18n 键；消息正文不存成品句子，切语言才不会读到旧语言
    pub code: String,
    /// 渲染参数（已转成字符串）
    pub args: Vec<String>,
    pub first_seen: String,
    pub last_seen: String,
    pub read_at: Option<String>,
    pub resolved_at: Option<String>,
}

/// 扫描出来的当前状态，尚未与库里已有的行合并
#[derive(Debug, Clone, PartialEq)]
pub struct AlertCandidate {
    pub dedupe_key: String,
    pub kind: &'static str,
    pub severity: &'static str,
    pub ref_type: Option<String>,
    pub ref_id: Option<String>,
    pub code: &'static str,
    pub args: Vec<String>,
}

/// 一次扫描的结果：新出现（或解决后重新出现）了几条、解决了几个旧条件、当前未读几条
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AlertSweep {
    pub opened: usize,
    pub resolved: usize,
    pub unread: usize,
}

/// `GET /api/alerts` 的响应
#[derive(Debug, Clone, Serialize)]
pub struct AlertList {
    pub items: Vec<Alert>,
    pub unread: usize,
}
