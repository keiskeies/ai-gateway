use serde::{Deserialize, Serialize};

/// 一个「Key 申请快捷入口」。
///
/// 放在后端而不是前端硬编码：改一个 URL 不用重新构建前端，且能被
/// `GET /api/platforms/presets` 复用。
///
/// `verified` 必须诚实——外链会改版、会失效，标 `unverified` 的条目在 UI 上
/// 不显示「已验证」。2026-10-27 的实测结果见 docs/research/api-key-links.md。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyLink {
    /// 品牌名。专有名词，不翻译（中英共用）。
    pub brand: String,
    /// 与 `PlatformPreset.name` 匹配时优先使用
    pub preset: Option<String>,
    /// 与平台 base_url 前缀匹配（preset 匹配失败时兜底）
    pub base_url_prefix: Option<String>,
    /// API Key 申请页
    pub apply_url: String,
    /// 控制台 / 用量页
    pub console_url: Option<String>,
    /// 文档
    pub docs_url: Option<String>,
    /// 该上游是否 OpenAI 兼容
    pub openai_compatible: bool,
    /// 可达性：ok / auth-required / blocked / unverified
    pub verified: String,
    /// 一句话备注（如「国内站与海外站是两套账号」）
    pub note: Option<String>,
}

impl KeyLink {
    fn new(brand: &str, apply_url: &str) -> Self {
        Self {
            brand: brand.to_string(),
            preset: None,
            base_url_prefix: None,
            apply_url: apply_url.to_string(),
            console_url: None,
            docs_url: None,
            openai_compatible: true,
            verified: "unverified".to_string(),
            note: None,
        }
    }

    fn preset(mut self, preset: &str) -> Self {
        self.preset = Some(preset.to_string());
        self
    }

    fn prefix(mut self, prefix: &str) -> Self {
        self.base_url_prefix = Some(prefix.to_string());
        self
    }

    fn console(mut self, url: &str) -> Self {
        self.console_url = Some(url.to_string());
        self
    }

    fn docs(mut self, url: &str) -> Self {
        self.docs_url = Some(url.to_string());
        self
    }

    fn verified(mut self, state: &str) -> Self {
        self.verified = state.to_string();
        self
    }

    fn note(mut self, note: &str) -> Self {
        self.note = Some(note.to_string());
        self
    }

    fn native_protocol(mut self) -> Self {
        self.openai_compatible = false;
        self
    }

    /// 内置注册表。顺序即 UI 展示顺序：私有协议与用户点名的排前面。
    ///
    /// 状态码来自 2026-10-27 实测（见 docs/research/api-key-links.md）：
    /// - `ok`：Key 页直接 200
    /// - `auth-required`：端点存在但需鉴权（base_url 探活得 401，比 200 更能证明正确）
    /// - `blocked`：被反爬拦截，URL 正确但未实测到 200
    pub fn all() -> Vec<Self> {
        vec![
            // —— 用户点名的四家 ——
            Self::new("Agnes AI", "https://platform.agnes-ai.com/settings/apiKeys")
                .preset("Agnes AI（海外）")
                .prefix("https://apihub.agnes-ai.com")
                .console("https://platform.agnes-ai.com")
                .docs("https://wiki.agnes-ai.com/zh-Hans/docs/quickstart")
                .verified("ok")
                .note("海外站，美元计价 / Token Plan（按请求次数）"),
            Self::new("Agnes AI", "https://platform.agnes-ai.cn/settings/apiKeys")
                .preset("Agnes AI（国内）")
                .prefix("https://api.agnes-ai.cn")
                .console("https://platform.agnes-ai.cn")
                .docs("https://agnes-ai.cn/doc/overview")
                .verified("ok")
                .note("国内站，人民币计价 / Agent Plan（Credits 积分，1000 = ¥1）"),
            Self::new("ModelScope", "https://www.modelscope.cn/my/settings/token")
                .preset("ModelScope 魔搭")
                .prefix("https://api-inference.modelscope.cn")
                .console("https://www.modelscope.cn/my/settings/token")
                .docs("https://www.modelscope.cn/docs/model-service/API-Inference/intro")
                .verified("ok")
                .note("魔搭社区（用户口语称「魔塔」）。模型 id 带斜杠，如 Qwen/Qwen3-*"),
            Self::new("Trae", "https://www.trae.cn")
                .preset("Trae（国内）")
                .prefix("https://trae-api-cn.mchost.guru")
                .console("https://www.trae.cn")
                .docs("https://docs.trae.cn/ide/what-is-trae")
                .native_protocol()
                .verified("ok")
                .note("国内站。私有协议，无公开 API；积分是 31 天钱包，非每月配额"),
            Self::new("Trae", "https://www.trae.ai")
                .preset("Trae（海外）")
                .prefix("https://core-normal.traeapi.us")
                .console("https://www.trae.ai")
                .docs("https://www.trae.ai/pricing")
                .native_protocol()
                .verified("ok")
                .note("海外站。计费单位是 Usage（美元额度），不是积分"),
            Self::new("WorkBuddy", "https://www.workbuddy.cn")
                .preset("WorkBuddy（国内）")
                .prefix("https://copilot.tencent.com")
                .console("https://www.workbuddy.cn")
                .docs("https://www.workbuddy.cn/docs/workbuddy/Overview")
                .native_protocol()
                .verified("ok")
                .note("腾讯 WorkBuddy，与 CodeBuddy 积分共享；上游正在收紧非官方客户端"),
            Self::new("WorkBuddy", "https://www.workbuddy.ai")
                .preset("WorkBuddy（海外）")
                .prefix("https://www.workbuddy.ai")
                .console("https://www.workbuddy.ai")
                .docs("https://www.workbuddy.ai/docs/zh/workbuddy/Overview")
                .native_protocol()
                .verified("ok")
                .note("国际版。与国内版是两套独立账号、两份额度"),
            // —— 通用厂商 ——
            Self::new("OpenAI", "https://platform.openai.com/api-keys")
                .prefix("https://api.openai.com")
                .console("https://platform.openai.com")
                .verified("blocked")
                .note("Key 页被 Cloudflare 拦非浏览器 UA（403），地址本身有效"),
            Self::new("Anthropic", "https://console.anthropic.com/settings/keys")
                .prefix("https://api.anthropic.com")
                .console("https://console.anthropic.com")
                .native_protocol()
                .verified("ok")
                .note("原生 Messages 协议，网关走 Anthropic 适配器"),
            Self::new("DeepSeek", "https://platform.deepseek.com/api_keys")
                .prefix("https://api.deepseek.com")
                .console("https://platform.deepseek.com")
                .verified("ok"),
            Self::new("智谱 BigModel", "https://bigmodel.cn/usercenter/proj-mgmt/apikeys")
                .prefix("https://open.bigmodel.cn")
                .console("https://bigmodel.cn")
                .verified("ok"),
            Self::new("Moonshot", "https://platform.moonshot.cn/console/api-keys")
                .prefix("https://api.moonshot.cn")
                .console("https://platform.moonshot.cn")
                .verified("ok")
                .note("国内站；海外版是 platform.moonshot.ai"),
            Self::new("Moonshot", "https://platform.moonshot.ai/console/api-keys")
                .prefix("https://api.moonshot.ai")
                .console("https://platform.moonshot.ai")
                .verified("ok")
                .note("海外版"),
            Self::new("SiliconFlow", "https://cloud.siliconflow.cn/account/ak")
                .prefix("https://api.siliconflow.cn")
                .console("https://cloud.siliconflow.cn")
                .verified("ok")
                .note("国内站；海外版是 cloud.siliconflow.com"),
            Self::new("SiliconFlow", "https://cloud.siliconflow.com/account/ak")
                .prefix("https://api.siliconflow.com")
                .console("https://cloud.siliconflow.com")
                .verified("ok")
                .note("海外版"),
            Self::new("OpenRouter", "https://openrouter.ai/keys")
                .prefix("https://openrouter.ai/api")
                .console("https://openrouter.ai")
                .verified("ok")
                .note("模型列表公开可读，免鉴权"),
            Self::new("阿里百炼", "https://bailian.console.aliyun.com/?apiKey=1")
                .prefix("https://dashscope.aliyuncs.com")
                .console("https://bailian.console.aliyun.com")
                .verified("ok")
                .note("兼容模式路径必须带 /compatible-mode/v1"),
            Self::new("火山方舟", "https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey")
                .prefix("https://ark.cn-beijing.volces.com")
                .console("https://console.volcengine.com/ark")
                .verified("ok")
                .note("路径是 /api/v3，不是 /v1"),
            Self::new("Groq", "https://console.groq.com/keys")
                .prefix("https://api.groq.com")
                .console("https://console.groq.com")
                .verified("ok")
                .note("路径含 /openai"),
            Self::new("Together AI", "https://api.together.xyz/settings/api-keys")
                .prefix("https://api.together.xyz")
                .console("https://api.together.xyz")
                .verified("ok")
                .note("新域名 api.together.ai 也可用"),
            Self::new("腾讯云 LKEAP", "https://console.cloud.tencent.com/lkeap/api-key")
                .prefix("https://api.lkeap.cloud.tencent.com")
                .console("https://console.cloud.tencent.com/lkeap")
                .verified("ok"),
        ]
    }

    /// 按预设名或 base_url 前缀匹配。预设名优先（更精确）。
    pub fn find_for(preset_name: Option<&str>, base_url: Option<&str>) -> Option<Self> {
        let all = Self::all();
        if let Some(name) = preset_name {
            if let Some(hit) = all.iter().find(|l| l.preset.as_deref() == Some(name)) {
                return Some(hit.clone());
            }
        }
        if let Some(url) = base_url {
            let normalized = crate::catalog::normalize_base_url(url);
            if !normalized.is_empty() {
                if let Some(hit) = all.iter().find(|l| {
                    l.base_url_prefix
                        .as_deref()
                        .map(|p| normalized.starts_with(p))
                        .unwrap_or(false)
                }) {
                    return Some(hit.clone());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_has_https_apply_url() {
        for link in KeyLink::all() {
            assert!(
                link.apply_url.starts_with("https://"),
                "{} 的 apply_url 必须是 https：{}",
                link.brand,
                link.apply_url
            );
        }
    }

    #[test]
    fn brands_are_not_empty_and_verified_is_known() {
        for link in KeyLink::all() {
            assert!(!link.brand.trim().is_empty());
            assert!(
                ["ok", "auth-required", "blocked", "unverified"].contains(&link.verified.as_str()),
                "{} 的 verified 值非法：{}",
                link.brand,
                link.verified
            );
        }
    }

    #[test]
    fn preset_match_wins_over_prefix() {
        let hit = KeyLink::find_for(Some("Agnes AI（国内）"), None).unwrap();
        assert_eq!(hit.apply_url, "https://platform.agnes-ai.cn/settings/apiKeys");
    }

    #[test]
    fn prefix_match_works_when_preset_unknown() {
        let hit = KeyLink::find_for(None, Some("https://api-inference.modelscope.cn/v1")).unwrap();
        assert_eq!(hit.brand, "ModelScope");
    }

    #[test]
    fn prefix_match_tolerates_trailing_slash_and_case() {
        let hit = KeyLink::find_for(None, Some("HTTPS://API.DeepSeek.com/v1/")).unwrap();
        assert_eq!(hit.brand, "DeepSeek");
    }

    #[test]
    fn unknown_returns_none() {
        assert!(KeyLink::find_for(Some("不存在的预设"), Some("https://example.com/v1")).is_none());
        assert!(KeyLink::find_for(None, None).is_none());
    }

    #[test]
    fn agnes_two_sites_are_distinct_entries() {
        let all = KeyLink::all();
        let agnes: Vec<&KeyLink> = all.iter().filter(|l| l.brand == "Agnes AI").collect();
        assert_eq!(agnes.len(), 2, "Agnes 国内与海外必须是两条独立条目");
        assert_ne!(agnes[0].apply_url, agnes[1].apply_url);
        assert_ne!(agnes[0].base_url_prefix, agnes[1].base_url_prefix);
    }

    #[test]
    fn proprietary_links_are_not_marked_openai_compatible() {
        let all = KeyLink::all();
        for brand in ["Trae", "WorkBuddy"] {
            let hits: Vec<&KeyLink> = all.iter().filter(|l| l.brand == brand).collect();
            assert!(!hits.is_empty(), "{brand} 必须有条目");
            for hit in hits {
                assert!(!hit.openai_compatible, "{brand} 走私有协议，不能标 OpenAI 兼容");
            }
        }
    }
}
