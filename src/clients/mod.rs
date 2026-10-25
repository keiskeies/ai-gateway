//! 本地 AI 编程客户端的接入配置管理。
//!
//! 设计对齐 CC Switch 的“地板”模型：只写自己拥有的字段，其他配置（permissions、
//! hooks、MCP、projects、其他 provider）原样保留；应用前备份，撤销时只回滚值仍等于
//! 自己写入内容的字段，用户改过的字段保留并报告。
//!
//! 六家客户端各有格式（JSON / JSONC / JSON5 / TOML / YAML），适配器共享同一个
//! `Adapter` 契约；服务层统一处理网关 Key、备份、状态落库和文件原子替换。

pub mod claude;
pub mod codex;
pub mod hermes;
pub mod openclaw;
pub mod opencode;
pub mod pi;
pub mod service;

#[cfg(test)]
mod adapter_tests;
#[cfg(test)]
mod service_tests;

use crate::error::{AppError, AppResult};
use std::collections::{BTreeMap, HashMap};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientId {
    ClaudeCode,
    Codex,
    Opencode,
    Openclaw,
    Pi,
    Hermes,
}

impl ClientId {
    pub const ALL: [ClientId; 6] = [
        ClientId::ClaudeCode,
        ClientId::Codex,
        ClientId::Opencode,
        ClientId::Openclaw,
        ClientId::Pi,
        ClientId::Hermes,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ClientId::ClaudeCode => "claude_code",
            ClientId::Codex => "codex",
            ClientId::Opencode => "opencode",
            ClientId::Openclaw => "openclaw",
            ClientId::Pi => "pi",
            ClientId::Hermes => "hermes",
        }
    }

    pub fn from_str(value: &str) -> AppResult<Self> {
        match value {
            "claude_code" => Ok(ClientId::ClaudeCode),
            "codex" => Ok(ClientId::Codex),
            "opencode" => Ok(ClientId::Opencode),
            "openclaw" => Ok(ClientId::Openclaw),
            "pi" => Ok(ClientId::Pi),
            "hermes" => Ok(ClientId::Hermes),
            other => Err(AppError::BadRequest(format!("Unknown client '{other}'"))),
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            ClientId::ClaudeCode => "Claude Code",
            ClientId::Codex => "Codex",
            ClientId::Opencode => "OpenCode",
            ClientId::Openclaw => "OpenClaw",
            ClientId::Pi => "Pi",
            ClientId::Hermes => "Hermes",
        }
    }
}

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 解析后的路径。`for_home` 完全不读环境变量，单测用它把写入锁在临时目录里。
#[derive(Debug, Clone)]
pub struct ClientPaths {
    pub claude_settings: PathBuf,
    pub codex_config: PathBuf,
    pub opencode_config: PathBuf,
    pub openclaw_config: PathBuf,
    pub pi_models: PathBuf,
    pub hermes_config: PathBuf,
}

impl ClientPaths {
    pub fn from_env() -> AppResult<Self> {
        // 仅调试构建接受测试 home；发布版绝不因为一个环境变量把写入重定向到别处。
        let test_home = if cfg!(debug_assertions) { env_dir("AIGW_CLIENT_HOME") } else { None };
        let home = match &test_home {
            Some(path) => path.clone(),
            None => platform_home()?,
        };
        let local_app_data = if test_home.is_some() {
            home.join("AppData").join("Local")
        } else {
            env_dir("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData").join("Local"))
        };
        let xdg_config = if test_home.is_some() {
            home.join(".config")
        } else {
            env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"))
        };
        // AIGW_CLIENT_HOME 是调试/E2E 的隔离根：此时忽略各客户端自己的覆盖变量，
        // 否则测试里一次 Apply 仍可能写到真实的 ~/.hermes 等目录。
        let override_dir = |name: &str| {
            if test_home.is_some() { None } else {
                env_dir(name)
            }
        };
        let claude_dir = override_dir("CLAUDE_CONFIG_DIR").unwrap_or_else(|| home.join(".claude"));
        let codex_dir = override_dir("CODEX_HOME").unwrap_or_else(|| home.join(".codex"));
        let openclaw_dir = override_dir("OPENCLAW_HOME").unwrap_or_else(|| home.join(".openclaw"));
        let pi_dir = override_dir("PI_CODING_AGENT_DIR").unwrap_or_else(|| home.join(".pi").join("agent"));
        let hermes_dir = override_dir("HERMES_HOME").unwrap_or_else(|| {
            if cfg!(windows) { local_app_data.join("hermes") } else { home.join(".hermes") }
        });
        Ok(Self::build(
            claude_dir,
            codex_dir,
            xdg_config.join("opencode"),
            openclaw_dir,
            pi_dir,
            hermes_dir,
        ))
    }

    /// 测试用：所有客户端都挂在同一个临时 home 下，不读任何环境变量。
    pub fn for_home(home: impl Into<PathBuf>) -> Self {
        let home = home.into();
        let local = home.join("AppData").join("Local");
        Self::build(
            home.join(".claude"),
            home.join(".codex"),
            home.join(".config").join("opencode"),
            home.join(".openclaw"),
            home.join(".pi").join("agent"),
            if cfg!(windows) { local.join("hermes") } else { home.join(".hermes") },
        )
    }

    fn build(
        claude_dir: PathBuf,
        codex_dir: PathBuf,
        opencode_dir: PathBuf,
        openclaw_dir: PathBuf,
        pi_dir: PathBuf,
        hermes_dir: PathBuf,
    ) -> Self {
        // OpenCode 同时支持 .jsonc 与 .json；已有 .jsonc 时优先它（与 CC Switch 一致）
        let opencode_jsonc = opencode_dir.join("opencode.jsonc");
        let opencode_json = opencode_dir.join("opencode.json");
        Self {
            claude_settings: claude_dir.join("settings.json"),
            codex_config: codex_dir.join("config.toml"),
            opencode_config: if opencode_jsonc.exists() { opencode_jsonc } else { opencode_json },
            openclaw_config: openclaw_dir.join("openclaw.json"),
            pi_models: pi_dir.join("models.json"),
            hermes_config: hermes_dir.join("config.yaml"),
        }
    }
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

fn platform_home() -> AppResult<PathBuf> {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    home.map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| AppError::Internal("Cannot resolve the user home directory".to_string()))
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ClientCurrent {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub key_present: bool,
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientStatus {
    pub id: String,
    pub name: String,
    pub path: String,
    pub exists: bool,
    pub detected: bool,
    pub managed: bool,
    pub backup_path: Option<String>,
    pub current: ClientCurrent,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientChange {
    /// 客户端配置里的逻辑路径，例如 `env.ANTHROPIC_BASE_URL`。
    pub path: String,
    pub before: Option<String>,
    /// 密钥类字段只给占位符（`<new key on apply>` / `<managed key>`），绝不回传明文。
    pub after: String,
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeptChange {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientPreview {
    pub id: String,
    pub path: String,
    pub exists: bool,
    pub managed: bool,
    pub backup_path: Option<String>,
    /// `create` 或 `reuse`；前端只做文案映射。
    pub key_action: String,
    pub changes: Vec<ClientChange>,
    pub warnings: Vec<String>,
    pub current: ClientCurrent,
    /// 附加项的当前值，用于表单回填：关掉一个开关必须等于"文件里那个键没了"，
    /// 而不是"表单默认没勾"。只有 Claude Code 会返回内容。
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub tier_names: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientApplyResult {
    pub id: String,
    pub path: String,
    pub backup_path: Option<String>,
    pub key_id: String,
    pub key_name: String,
    pub key_mask: String,
    pub created_key: bool,
    pub changes: Vec<ClientChange>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientRestoreResult {
    pub id: String,
    pub path: String,
    pub backup_path: Option<String>,
    pub restored: Vec<String>,
    pub kept: Vec<KeptChange>,
    pub disabled_key: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ClientApplyRequest {
    /// 网关根地址；为空时服务层按当前 admin_port 生成 `http://127.0.0.1:<port>`。
    #[serde(default)]
    pub base_url: Option<String>,
    pub default_model: String,
    #[serde(default)]
    pub sonnet_model: Option<String>,
    #[serde(default)]
    pub opus_model: Option<String>,
    #[serde(default)]
    pub haiku_model: Option<String>,
    #[serde(default)]
    pub fable_model: Option<String>,
    #[serde(default)]
    pub subagent_model: Option<String>,
    /// 档位显示名（Claude Code 的 `/model` 菜单里显示什么，实际请求仍是 `_MODEL`）。
    #[serde(default)]
    pub tier_names: Option<HashMap<String, String>>,
    /// 允许清单内的附加环境变量；没列在这里的键一律拒绝，避免把 settings.json 写成任意键值编辑器。
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone)]
pub struct ResolvedModels {
    pub default: String,
    pub sonnet: String,
    pub opus: String,
    pub haiku: String,
    pub fable: String,
    pub subagent: String,
}

/// 除「地址 + Key + 模型」之外，一次写入还带着什么。
#[derive(Debug, Clone, Default)]
pub struct ClientExtras {
    /// 档位 → 显示名，键是 sonnet / opus / haiku / fable
    pub tier_names: BTreeMap<String, String>,
    /// 允许清单内的附加环境变量
    pub env: BTreeMap<String, String>,
    /// 上一次接入写过的托管状态；这次不再要的可选键据此撤掉
    pub managed: Option<Value>,
}

/// 客户端配置里允许由 AI Gateway 代写的附加键。
///
/// 只放 Claude Code 自己认的环境变量：数值型（超时、输出/上下文上限）与开关型
/// （Tool Search、自动升级、Artifact、auto mode 服务端检查、AI 署名）。
/// 想加新键先想清楚它是不是"客户端配置"的事——上游协议、UA、Body 覆盖属于网关侧。
pub const CLAUDE_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_SMALL_FAST_MODEL",
    "ANTHROPIC_CUSTOM_HEADERS",
    "API_TIMEOUT_MS",
    "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
    "CLAUDE_CODE_MAX_CONTEXT_TOKENS",
    "ENABLE_TOOL_SEARCH",
    "DISABLE_AUTOUPDATER",
    "CLAUDE_CODE_DISABLE_ARTIFACT",
    "CLAUDE_CODE_AUTO_MODE_SERVER",
    "CLAUDE_CODE_ATTRIBUTION_HEADER",
];

const NUMERIC_CLAUDE_ENV_KEYS: &[&str] = &[
    "API_TIMEOUT_MS",
    "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
    "CLAUDE_CODE_MAX_CONTEXT_TOKENS",
];

const TIER_KEYS: &[&str] = &["sonnet", "opus", "haiku", "fable"];

fn validate_env(env: &HashMap<String, String>) -> AppResult<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for (key, value) in env {
        if !CLAUDE_ENV_KEYS.contains(&key.as_str()) {
            return Err(AppError::BadRequest(format!(
                "env key {key} is not managed by AI Gateway; allowed: {}",
                CLAUDE_ENV_KEYS.join(", ")
            )));
        }
        let value = value.trim();
        if value.is_empty() || value.len() > 2000 {
            return Err(AppError::BadRequest(format!("env {key} must be 1–2000 characters")));
        }
        if key != "ANTHROPIC_CUSTOM_HEADERS" && value.chars().any(|c| c == '\n' || c == '\r') {
            return Err(AppError::BadRequest(format!("env {key} must be a single line")));
        }
        if NUMERIC_CLAUDE_ENV_KEYS.contains(&key.as_str()) && value.parse::<u64>().is_err() {
            return Err(AppError::BadRequest(format!("env {key} must be a number")));
        }
        out.insert(key.clone(), value.to_string());
    }
    Ok(out)
}

fn validate_tier_names(names: &HashMap<String, String>) -> AppResult<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for (tier, name) in names {
        if !TIER_KEYS.contains(&tier.as_str()) {
            return Err(AppError::BadRequest(format!("unknown model tier {tier}")));
        }
        let name = name.trim();
        if name.is_empty() || name.len() > 120 || name.chars().any(|c| c == '\n' || c == '\r') {
            return Err(AppError::BadRequest(format!(
                "{tier} display name must be 1–120 characters without newlines"
            )));
        }
        out.insert(tier.clone(), name.to_string());
    }
    Ok(out)
}

impl ClientApplyRequest {
    pub fn extras(&self, managed: Option<Value>) -> AppResult<ClientExtras> {
        Ok(ClientExtras {
            tier_names: validate_tier_names(self.tier_names.as_ref().unwrap_or(&HashMap::new()))?,
            env: validate_env(self.env.as_ref().unwrap_or(&HashMap::new()))?,
            managed,
        })
    }

    pub fn resolved(&self) -> AppResult<ResolvedModels> {
        let default = self.default_model.trim();
        if default.is_empty() {
            return Err(AppError::BadRequest("default_model is required".to_string()));
        }
        let pick = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(default)
                .to_string()
        };
        Ok(ResolvedModels {
            default: default.to_string(),
            sonnet: pick(&self.sonnet_model),
            opus: pick(&self.opus_model),
            haiku: pick(&self.haiku_model),
            fable: pick(&self.fable_model),
            subagent: pick(&self.subagent_model),
        })
    }

    pub fn models(&self) -> Vec<&str> {
        let mut models = vec![self.default_model.trim()];
        for value in [
            self.sonnet_model.as_deref(),
            self.opus_model.as_deref(),
            self.haiku_model.as_deref(),
            self.fable_model.as_deref(),
            self.subagent_model.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let value = value.trim();
            if !value.is_empty() {
                models.push(value);
            }
        }
        models
    }
}

pub struct ClientPlan {
    pub path: PathBuf,
    pub original: Option<Vec<u8>>,
    pub content: Vec<u8>,
    pub changes: Vec<ClientChange>,
    /// 适配器自己的托管状态；服务层会把 token_hash 补进去再落库。
    pub managed: Value,
    pub warnings: Vec<String>,
}

pub struct RestorePlan {
    pub path: PathBuf,
    pub original: Option<Vec<u8>>,
    pub content: Vec<u8>,
    pub restored: Vec<String>,
    pub kept: Vec<KeptChange>,
    pub warnings: Vec<String>,
    pub remove_file: bool,
    /// false 表示配置已经不需要改动（例如文件已不存在）；服务层不要写空文件。
    pub changed: bool,
}

pub trait Adapter: Send + Sync {
    fn id(&self) -> ClientId;
    fn path(&self, paths: &ClientPaths) -> PathBuf;
    fn status(&self, paths: &ClientPaths, managed: Option<&Value>) -> AppResult<ClientStatus>;
    fn current_token(&self, paths: &ClientPaths) -> AppResult<Option<String>>;
    fn plan(
        &self,
        paths: &ClientPaths,
        models: &ResolvedModels,
        base_url: &str,
        token: &str,
        allow_existing: bool,
        extras: &ClientExtras,
    ) -> AppResult<ClientPlan>;
    fn restore(&self, paths: &ClientPaths, managed: &Value) -> AppResult<RestorePlan>;

    /// 读回附加项当前值（附加环境变量、档位显示名）；除 Claude Code 外都是空的。
    fn read_extras(
        &self,
        _paths: &ClientPaths,
    ) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        (BTreeMap::new(), BTreeMap::new())
    }
}

pub fn adapter(id: ClientId) -> &'static dyn Adapter {
    match id {
        ClientId::ClaudeCode => &claude::ClaudeCodeAdapter,
        ClientId::Codex => &codex::CodexAdapter,
        ClientId::Opencode => &opencode::OpenCodeAdapter,
        ClientId::Openclaw => &openclaw::OpenClawAdapter,
        ClientId::Pi => &pi::PiAdapter,
        ClientId::Hermes => &hermes::HermesAdapter,
    }
}

pub fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn read_bytes(path: &Path) -> AppResult<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn read_text(path: &Path) -> AppResult<Option<String>> {
    match read_bytes(path)? {
        Some(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| AppError::BadRequest(format!("Config is not valid UTF-8: {}", path.display()))),
        None => Ok(None),
    }
}

/// 前端展示用：字符串原样，其他 JSON 值序列化；缺失返回 None。
pub fn display_value(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(other) => Some(other.to_string()),
    }
}

pub fn json_pretty(value: &Value) -> AppResult<Vec<u8>> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    Ok(text.into_bytes())
}

/// OpenAI 兼容客户端使用 `.../v1` 作为 base URL；Claude Code 用的是根地址。
pub fn openai_base(root: &str) -> String {
    if root.ends_with("/v1") {
        root.to_string()
    } else {
        format!("{root}/v1")
    }
}

/// 把“当前值 → 新值”的变化转成前端表格；密钥只回占位符。
pub fn change(path: impl Into<String>, before: Option<Value>, after: Value, secret: bool) -> ClientChange {
    ClientChange {
        path: path.into(),
        before: if secret { None } else { display_value(before.as_ref()) },
        after: if secret {
            "<new key on apply>".to_string()
        } else {
            display_value(Some(&after)).unwrap_or_default()
        },
        secret,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ids_round_trip() {
        for id in ClientId::ALL {
            assert_eq!(ClientId::from_str(id.as_str()).unwrap(), id);
        }
        assert!(ClientId::from_str("nope").is_err());
    }

    #[test]
    fn models_fall_back_to_default_and_keep_distinct_tiers() {
        let req = ClientApplyRequest {
            default_model: "main".into(),
            sonnet_model: Some("sonnet".into()),
            opus_model: Some("  ".into()),
            ..Default::default()
        };
        let models = req.resolved().unwrap();
        assert_eq!(models.default, "main");
        assert_eq!(models.sonnet, "sonnet");
        assert_eq!(models.opus, "main");
        assert_eq!(models.subagent, "main");
    }

    #[test]
    fn openai_base_appends_v1_once() {
        assert_eq!(openai_base("http://127.0.0.1:1994"), "http://127.0.0.1:1994/v1");
        assert_eq!(openai_base("https://x.example/v1"), "https://x.example/v1");
    }

    #[test]
    fn token_hash_is_stable() {
        assert_eq!(sha256_hex("abc"), sha256_hex("abc"));
        assert_ne!(sha256_hex("abc"), sha256_hex("abd"));
    }
}
