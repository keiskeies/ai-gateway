use serde::{Deserialize, Serialize};

use super::model::{Capability, ModelStatus};
use super::platform::{PlatformStatus, PlatformType};
use super::route::{
    BackendStatus, LBStrategy, ReasoningEcho, ReasoningStyle, RetryPolicy, UnknownParams,
};

/// 导出文件的自我描述：靠这两字段认出"这是本程序的配置备份"，
/// 而不是拿到任意 JSON 就往库里插。版本号变化时导入必须显式拒绝，
/// 让新程序先告诉用户它读不懂，而不是按老口径插一半。
pub const CONFIG_FORMAT: &str = "aigw-config";
/// v2 增加平台积分预算与模型积分单价；v1 旧文档仍然可导入（缺失字段按未启用处理）。
pub const CONFIG_VERSION: u32 = 2;

/// 配置文档：虚拟模型 + 平台 + 模型 + 路由 + 后端 + 网关 Key。
/// 不含统计、健康历史、账号与会话——那些是运行数据，不是"配置"，
/// 备份一份网关设置不应该顺带把历史流量和登录口令搬到另一台机器。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigDocument {
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub exported_at: String,
    #[serde(default)]
    pub gateway_version: String,
    /// 导出时的 config.toml 快照。只作参考：端口与监听地址会在导入瞬间改变正在服务的进程，
    /// 所以 apply 不碰它，计划里也会说明这一点。
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
    #[serde(default)]
    pub platforms: Vec<PlatformRow>,
    #[serde(default)]
    pub models: Vec<ModelRow>,
    #[serde(default)]
    pub proxies: Vec<ProxyRow>,
    #[serde(default)]
    pub routes: Vec<RouteRow>,
    #[serde(default)]
    pub backends: Vec<BackendRow>,
    #[serde(default)]
    pub api_keys: Vec<ApiKeyRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformRow {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub platform_type: PlatformType,
    pub base_url: String,
    /// `None`＝这次导出没带密钥（默认脱敏）。用 Option 区分"没带"与"本来就是空密钥"，
    /// 否则导入端无法知道该提示用户重新填哪几个。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default)]
    pub organization: Option<String>,
    #[serde(default = "empty_object")]
    pub custom_headers: serde_json::Value,
    #[serde(default = "default_platform_status")]
    pub status: PlatformStatus,
    #[serde(default)]
    pub rate_limit: Option<serde_json::Value>,
    /// 上游积分预算与自然窗口；NULL = 不启用。
    #[serde(default)]
    pub credit_budget: Option<f64>,
    #[serde(default)]
    pub credit_window: Option<String>,
    /// 上游目录键；旧配置没有时按 type + base_url 重算。
    #[serde(default)]
    pub catalog_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRow {
    pub id: String,
    pub platform_id: String,
    pub model_id: String,
    pub display_name: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    #[serde(default)]
    pub input_price: Option<f64>,
    #[serde(default)]
    pub output_price: Option<f64>,
    #[serde(default)]
    pub cache_read_price: Option<f64>,
    #[serde(default)]
    pub cache_write_price: Option<f64>,
    #[serde(default)]
    pub peak_pricing: Option<serde_json::Value>,
    /// 每次成功请求的积分单价；NULL = 未定价。
    #[serde(default)]
    pub credit_price: Option<f64>,
    /// 共享目录键；旧配置没有时从 platform 推导。
    #[serde(default)]
    pub catalog_key: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default = "default_model_status")]
    pub status: ModelStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRow {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRow {
    pub id: String,
    pub proxy_id: String,
    #[serde(default = "default_lb_strategy")]
    pub lb_strategy: LBStrategy,
    #[serde(default)]
    pub retry_policy: RetryPolicy,
    /// 目前只有存储与缓存搬运，代理链路还没有消费方；原样带走但不做引用校验。
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub default_max_tokens: Option<u32>,
    /// 每次上游尝试前置随机串；旧导出文档没有这个字段时按关闭处理。
    #[serde(default)]
    pub request_nonce: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendRow {
    pub id: String,
    pub route_id: String,
    pub platform_id: String,
    pub model_id: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub max_concurrent: Option<u32>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_backend_status")]
    pub status: BackendStatus,
    #[serde(default = "empty_object")]
    pub param_overrides: serde_json::Value,
    #[serde(default)]
    pub unknown_params: UnknownParams,
    #[serde(default)]
    pub reasoning_style: ReasoningStyle,
    #[serde(default)]
    pub reasoning_echo: ReasoningEcho,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRow {
    pub id: String,
    pub name: String,
    /// 摘要而非明文：库里本来就只有 sha256，导出带走的是"这一把 Key 仍然可用"这件事，
    /// 拿到文件的人换不来明文。缺这一行的 Key 不会被导出成可用凭据。
    pub key_hash: String,
    #[serde(default)]
    pub key_mask: String,
    #[serde(default)]
    pub proxy_id: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub quota_requests: Option<i64>,
    #[serde(default)]
    pub quota_tokens: Option<i64>,
    #[serde(default)]
    pub quota_cost: Option<f64>,
    #[serde(default)]
    pub quota_window: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub last_used: Option<String>,
}

/// 每张表的处理计数。merge 用 add/skip，replace 用 add/overwrite/drop，
/// 分开报是为了让人在读计划时就知道"历史归属会不会断"。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TablePlan {
    pub add: usize,
    pub overwrite: usize,
    pub skip: usize,
    pub drop: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    /// 只补没有的，已有 id 一律不动
    #[default]
    Merge,
    /// 六张配置表清空后按文档重建
    Replace,
}

impl ImportMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Replace => "replace",
        }
    }
}

/// 导入前的影响预演：先告诉用户会新增/跳过/删除什么，确认后才真的写库。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportPlan {
    pub ok: bool,
    pub mode: ImportMode,
    pub source_version: String,
    pub source_exported_at: String,
    pub platforms: TablePlan,
    pub models: TablePlan,
    pub proxies: TablePlan,
    pub routes: TablePlan,
    pub backends: TablePlan,
    pub api_keys: TablePlan,
    pub warnings: Vec<Issue>,
    pub errors: Vec<Issue>,
}

/// 真正写入后的结果（计数由落库那趟自己统计，与计划同源但不复算）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportResult {
    pub mode: ImportMode,
    pub platforms: TablePlan,
    pub models: TablePlan,
    pub proxies: TablePlan,
    pub routes: TablePlan,
    pub backends: TablePlan,
    pub api_keys: TablePlan,
    pub warnings: Vec<Issue>,
}

/// 计划里的一句话。后端只给 code + 参数，文案在前端字典里——
/// 导入预演是界面正文而不是偶发报错，中英必须同时齐全。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub code: String,
    pub args: Vec<String>,
}

impl Issue {
    pub fn new(code: &str, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self { code: code.to_string(), args: args.into_iter().map(|a| a.into()).collect() }
    }
}

impl ConfigDocument {
    pub fn is_empty(&self) -> bool {
        self.platforms.is_empty()
            && self.models.is_empty()
            && self.proxies.is_empty()
            && self.routes.is_empty()
            && self.backends.is_empty()
            && self.api_keys.is_empty()
    }

    /// 文档内自查：格式标识、id 唯一、引用完整。一次报全部而不是第一个就退出——
    /// 手改过的文件往往不止一处错，逐条挤牙膏最难查。
    pub fn check(&self) -> Vec<Issue> {
        let mut errors = vec![];

        match self.format.as_deref() {
            Some(CONFIG_FORMAT) => {}
            Some(other) => errors.push(Issue::new("cfgFormatMismatch", [other])),
            None => errors.push(Issue::new("cfgFormatMissing", [] as [String; 0])),
        }
        match self.version {
            // v1 文档没有积分字段：按默认值导入，等同于“未启用积分”。
            Some(v) if v == 1 || v == CONFIG_VERSION => {}
            Some(v) => errors.push(Issue::new("cfgVersionMismatch", [v.to_string(), CONFIG_VERSION.to_string()])),
            None => errors.push(Issue::new("cfgVersionMissing", [] as [String; 0])),
        }

        fn dup<T>(
            label: &str,
            key: impl Fn(&T) -> &str,
            rows: &[T],
            errors: &mut Vec<Issue>,
        ) {
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for row in rows {
                let id = key(row);
                if !id.is_empty() && !seen.insert(id) {
                    errors.push(Issue::new("cfgDuplicateId", [label.to_string(), id.to_string()]));
                }
            }
        }
        dup("platforms", |r: &PlatformRow| r.id.as_str(), &self.platforms, &mut errors);
        dup("models", |r: &ModelRow| r.id.as_str(), &self.models, &mut errors);
        dup("proxies", |r: &ProxyRow| r.id.as_str(), &self.proxies, &mut errors);
        dup("routes", |r: &RouteRow| r.id.as_str(), &self.routes, &mut errors);
        dup("backends", |r: &BackendRow| r.id.as_str(), &self.backends, &mut errors);
        dup("api_keys", |r: &ApiKeyRow| r.id.as_str(), &self.api_keys, &mut errors);

        // (platform_id, model_id) 在库里是 UNIQUE：文档内重复会在插入时炸掉整个事务，
        // 提前报出来比让 SQLite 抱怨更好读。
        let mut pairs: std::collections::HashSet<(&str, &str)> = std::collections::HashSet::new();
        for row in &self.models {
            if !pairs.insert((row.platform_id.as_str(), row.model_id.as_str())) {
                errors.push(Issue::new("cfgDuplicateModel", [row.model_id.clone(), row.platform_id.clone()]));
            }
        }

        let platform_ids: std::collections::HashSet<&str> =
            self.platforms.iter().map(|p| p.id.as_str()).collect();
        let proxy_ids: std::collections::HashSet<&str> =
            self.proxies.iter().map(|p| p.id.as_str()).collect();
        let route_ids: std::collections::HashSet<&str> =
            self.routes.iter().map(|r| r.id.as_str()).collect();
        let mut route_owner: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        for route in &self.routes {
            if !proxy_ids.contains(route.proxy_id.as_str()) {
                errors.push(Issue::new("cfgDanglingRouteProxy", [route.id.clone(), route.proxy_id.clone()]));
            }
            // routes.proxy_id 是 UNIQUE：一个虚拟模型只有一条路由
            if let Some(prev) = route_owner.insert(route.proxy_id.as_str(), route.id.as_str()) {
                errors.push(Issue::new("cfgMultipleRoutes", [route.proxy_id.clone(), prev.to_string(), route.id.clone()]));
            }
        }
        for row in &self.models {
            if !platform_ids.contains(row.platform_id.as_str()) {
                errors.push(Issue::new("cfgDanglingModelPlatform", [row.model_id.clone(), row.platform_id.clone()]));
            }
        }
        for row in &self.backends {
            if !route_ids.contains(row.route_id.as_str()) {
                errors.push(Issue::new("cfgDanglingBackendRoute", [row.id.clone(), row.route_id.clone()]));
            }
            if !platform_ids.contains(row.platform_id.as_str()) {
                errors.push(Issue::new("cfgDanglingBackendPlatform", [row.id.clone(), row.platform_id.clone()]));
            }
        }
        for row in &self.api_keys {
            if !row.key_hash.is_empty() && !is_hash(&row.key_hash) {
                errors.push(Issue::new("cfgBadKeyHash", [row.name.clone(), row.key_hash.chars().count().to_string()]));
            }
            if let Some(proxy_id) = &row.proxy_id {
                if !proxy_ids.contains(proxy_id.as_str()) {
                    errors.push(Issue::new("cfgDanglingKeyProxy", [row.name.clone(), proxy_id.clone()]));
                }
            }
        }
        let named: Vec<(&str, Vec<(&str, &str)>)> = vec![
            ("platforms", self.platforms.iter().map(|p| (p.id.as_str(), p.name.as_str())).collect()),
            ("models", self.models.iter().map(|m| (m.id.as_str(), m.display_name.as_str())).collect()),
            ("proxies", self.proxies.iter().map(|p| (p.id.as_str(), p.name.as_str())).collect()),
            ("api_keys", self.api_keys.iter().map(|k| (k.id.as_str(), k.name.as_str())).collect()),
        ];
        for (table, rows) in named {
            for (id, name) in rows {
                if name.trim().is_empty() {
                    errors.push(Issue::new("cfgEmptyName", [table.to_string(), id.to_string()]));
                }
            }
        }

        errors
    }

    /// 不阻断导入、但用户应该知道的事
    pub fn notes(&self) -> Vec<Issue> {
        let mut notes = vec![];
        let redacted = self.platforms.iter().filter(|p| p.api_key.is_none()).count();
        if redacted > 0 {
            notes.push(Issue::new("cfgRedactedKeys", [redacted.to_string()]));
        }
        let model_keys: std::collections::HashSet<(&str, &str)> = self
            .models
            .iter()
            .map(|m| (m.platform_id.as_str(), m.model_id.as_str()))
            .collect();
        let orphan = self
            .backends
            .iter()
            .filter(|b| !model_keys.contains(&(b.platform_id.as_str(), b.model_id.as_str())))
            .count();
        if orphan > 0 {
            notes.push(Issue::new("cfgOrphanBackends", [orphan.to_string()]));
        }
        if self.settings.is_some() {
            notes.push(Issue::new("cfgSettingsIgnored", [] as [String; 0]));
        }
        let current = env!("CARGO_PKG_VERSION");
        if !self.gateway_version.is_empty() && self.gateway_version != current {
            notes.push(Issue::new("cfgVersionDiffers", [self.gateway_version.clone(), current.to_string()]));
        }
        notes
    }
}

/// sha256 摘要一定是 64 个十六进制字符；用它挡掉"把明文 Key 粘进 key_hash"这种文件。
fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn empty_object() -> serde_json::Value {
    serde_json::json!({})
}
fn default_platform_status() -> PlatformStatus {
    PlatformStatus::Active
}
fn default_model_status() -> ModelStatus {
    ModelStatus::Active
}
fn default_backend_status() -> BackendStatus {
    BackendStatus::Active
}
fn default_lb_strategy() -> LBStrategy {
    LBStrategy::RoundRobin
}
fn default_weight() -> u32 {
    1
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_context_window() -> u32 {
    8192
}
fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(issues: &[Issue]) -> Vec<&str> {
        issues.iter().map(|i| i.code.as_str()).collect()
    }

    fn doc() -> ConfigDocument {
        serde_json::from_str(
            r#"{
              "format": "aigw-config",
              "version": 1,
              "platforms": [{"id":"p1","name":"本地","type":"OpenAI","base_url":"http://127.0.0.1:8080/v1",
                             "api_key":"sk-upstream","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}],
              "models": [{"id":"m1","platform_id":"p1","model_id":"llama","display_name":"llama"}],
              "proxies": [{"id":"x1","name":"chat","created_at":"2026-01-01T00:00:00Z"}],
              "routes": [{"id":"r1","proxy_id":"x1"}],
              "backends": [{"id":"b1","route_id":"r1","platform_id":"p1","model_id":"llama"}],
              "api_keys": [{"id":"k1","name":"k","key_hash":"0000000000000000000000000000000000000000000000000000000000000000","created_at":"2026-01-01T00:00:00Z"}]
            }"#,
        )
        .expect("minimal document parses with defaults")
    }

    /// 手写的最小文档必须能解析：可空字段全有默认值，否则导入等于要求用户复制整份导出。
    #[test]
    fn minimal_document_uses_column_defaults() {
        let d = doc();
        assert_eq!(codes(&d.check()), Vec::<&str>::new(), "{:?}", d.check());
        assert_eq!(d.models[0].max_tokens, 4096);
        assert_eq!(d.backends[0].weight, 1);
        assert_eq!(d.routes[0].lb_strategy, LBStrategy::RoundRobin);
        assert!(d.api_keys[0].enabled);
        assert_eq!(d.platforms[0].api_key.as_deref(), Some("sk-upstream"));
    }

    #[test]
    fn foreign_shaped_json_is_rejected_before_any_write() {
        let mut d = doc();
        d.format = Some("openv1".to_string());
        d.version = Some(9);
        assert_eq!(codes(&d.check()), vec!["cfgFormatMismatch", "cfgVersionMismatch"]);

        let mut missing = ConfigDocument::default();
        missing.platforms = d.platforms.clone();
        assert_eq!(codes(&missing.check()), vec!["cfgFormatMissing", "cfgVersionMissing"]);
    }

    /// 悬挂引用必须挡住：绑到不存在模型的 Key 正是 V14 之前"静默变全局"那一类事故的入口。
    #[test]
    fn dangling_references_are_errors() {
        let mut d = doc();
        d.api_keys[0].proxy_id = Some("nope".to_string());
        d.backends[0].platform_id = "ghost".to_string();
        let issues = d.check();
        assert!(issues.iter().any(|i| i.code == "cfgDanglingKeyProxy" && i.args == vec!["k".to_string(), "nope".to_string()]), "{issues:?}");
        assert!(issues.iter().any(|i| i.code == "cfgDanglingBackendPlatform" && i.args[1] == "ghost"));

        let mut plain = doc();
        plain.api_keys[0].key_hash = "sk-aigw-plaintext-not-a-hash".to_string();
        assert!(codes(&plain.check()).contains(&"cfgBadKeyHash"));
    }

    #[test]
    fn one_route_per_proxy_and_unique_model_pair_are_enforced() {
        let mut d = doc();
        d.routes.push(RouteRow {
            id: "r2".to_string(),
            proxy_id: "x1".to_string(),
            lb_strategy: LBStrategy::RoundRobin,
            retry_policy: RetryPolicy::default(),
            fallback: None,
            default_max_tokens: None,
            request_nonce: false,
        });
        assert!(codes(&d.check()).contains(&"cfgMultipleRoutes"));

        let mut dup = doc();
        dup.models.push(ModelRow {
            id: "m2".to_string(),
            platform_id: "p1".to_string(),
            model_id: "llama".to_string(),
            display_name: "重复".to_string(),
            max_tokens: 4096,
            context_window: 8192,
            input_price: None,
            output_price: None,
            cache_read_price: None,
            cache_write_price: None,
            peak_pricing: None,
            credit_price: None,
            catalog_key: None,
            capabilities: vec![],
            status: ModelStatus::Active,
        });
        assert!(codes(&dup.check()).contains(&"cfgDuplicateModel"));
    }

    /// 脱敏导出（api_key 字段缺席）要能在计划里说出来：
    /// 否则导入完才发现所有平台都没密钥，是最难查的"配置明明导入了却全红"。
    #[test]
    fn redacted_keys_and_missing_model_rows_are_called_out() {
        let mut d = doc();
        d.platforms[0].api_key = None;
        assert!(d.check().is_empty());
        assert!(d.notes().iter().any(|n| n.code == "cfgRedactedKeys" && n.args == vec!["1".to_string()]));

        let mut orphan = doc();
        orphan.backends[0].model_id = "untracked".to_string();
        assert!(codes(&orphan.notes()).contains(&"cfgOrphanBackends"));

        let mut with_settings = doc();
        with_settings.settings = Some(serde_json::json!({"admin_port": 1994}));
        assert!(codes(&with_settings.notes()).contains(&"cfgSettingsIgnored"));
    }

    #[test]
    fn round_trips_through_json() {
        let text = serde_json::to_string(&doc()).expect("serialize");
        let again: ConfigDocument = serde_json::from_str(&text).expect("deserialize");
        assert!(again.check().is_empty());
        // 平台密钥字段只在带明文导出时出现
        assert_eq!(again.platforms[0].api_key.as_deref(), Some("sk-upstream"));
    }

    /// 字段缺席 vs 空串必须可分：这是 V18 三态列的同一个教训
    #[test]
    fn absent_api_key_is_not_an_empty_key() {
        let redacted: ConfigDocument = serde_json::from_str(
            r#"{"format":"aigw-config","version":1,
               "platforms":[{"id":"p1","name":"n","type":"OpenAI","base_url":"http://x",
                             "created_at":"c","updated_at":"u"}]}"#,
        )
        .unwrap();
        assert!(redacted.platforms[0].api_key.is_none());
        let blank: ConfigDocument = serde_json::from_str(
            r#"{"format":"aigw-config","version":1,
               "platforms":[{"id":"p1","name":"n","type":"OpenAI","base_url":"http://x","api_key":"",
                             "created_at":"c","updated_at":"u"}]}"#,
        )
        .unwrap();
        assert_eq!(blank.platforms[0].api_key.as_deref(), Some(""));
        // 序列化时 None 字段整个消失，不会写出 "api_key": null（"api_keys" 是集合名，不算）
        assert!(!serde_json::to_string(&redacted).unwrap().contains("\"api_key\":"));
    }
}
