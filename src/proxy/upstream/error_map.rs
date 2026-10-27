//! 上游错误码 → 网关语义。
//!
//! 这个模块的价值全在「区分」上：把「模型不在目录」当成可重试，会让一个必然失败的
//! 请求在每个账号上轮流撞一遍（白等 N × 超时）；把「风控」当成配额耗尽去换号，
//! 会让单账号封禁升级成 IP 段封禁。所以每种码都必须落到正确的桶里。

use serde::{Deserialize, Serialize};

/// 上游失败的语义分类。**决定是否重试、是否换号、是否冷却。**
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UpstreamFailure {
    /// 请求级错误：模型不在目录、参数非法、权益不足。
    /// **不换号、不冷却、不重试**——换号也一样失败。
    ClientError { status: u16, message: String },
    /// 账号配额耗尽：换号 + 冷却 + 计入额度台账。
    QuotaExhausted { retry_after_secs: u64, message: String },
    /// 设备 / IP 级风控：换号通常无效，长冷却 + 告警。
    /// **继续换号会放大封禁**（同出口多账号连撞 → IP 被封）。
    RiskControl { cooldown_secs: u64, message: String },
    /// 凭据失效：触发刷新；刷新失败则禁用平台。
    Unauthorized { message: String },
    /// 连接级抖动（ECONNRESET 等）：可重试。
    Transport { message: String },
    /// 渠道容量满载（排队/限流）：不罚号，短暂退避。
    UpstreamBusy { retry_after_secs: u64, message: String },
}

impl UpstreamFailure {
    /// 是否可以换一个后端重试。
    ///
    /// `ClientError` 与 `RiskControl` 都返回 false：
    /// - ClientError 换号也没用（模型不在目录是请求本身的问题）
    /// - RiskControl 换号更糟（会连坐同出口的其它账号）
    pub fn should_try_another_backend(&self) -> bool {
        matches!(
            self,
            Self::QuotaExhausted { .. } | Self::Unauthorized { .. }
        )
    }

    /// 是否应该把该平台短暂移出候选（冷却）。
    pub fn should_cooldown(&self) -> bool {
        matches!(
            self,
            Self::QuotaExhausted { .. } | Self::RiskControl { .. } | Self::Unauthorized { .. }
        )
    }

    /// 冷却秒数。ClientError / Transport / UpstreamBusy 不冷却账号。
    pub fn cooldown_secs(&self) -> Option<u64> {
        match self {
            Self::QuotaExhausted { retry_after_secs, .. } => Some(*retry_after_secs),
            Self::RiskControl { cooldown_secs, .. } => Some(*cooldown_secs),
            Self::Unauthorized { .. } => Some(300),
            Self::UpstreamBusy { retry_after_secs, .. } => Some(*retry_after_secs),
            _ => None,
        }
    }

    /// 对客户端返回的 HTTP 状态码。
    ///
    /// **Agnes 的 402 必须转成 429**：OpenAI 生态的客户端只把 429 认成「限流/额度」，
    /// 402 会被当成未知错误、不做退避。
    pub fn client_status(&self) -> u16 {
        match self {
            Self::ClientError { status, .. } => *status,
            Self::QuotaExhausted { .. } | Self::UpstreamBusy { .. } => 429,
            Self::RiskControl { .. } => 429,
            Self::Unauthorized { .. } => 401,
            Self::Transport { .. } => 502,
        }
    }

    /// 是否需要告警（进通知中心）。
    pub fn should_alert(&self) -> bool {
        matches!(self, Self::RiskControl { .. })
    }
}

/// 流内业务错误 → 语义。**不依赖 HTTP 状态码**。
///
/// 为什么需要单独一个入口：Trae / WorkBuddy 把业务失败放在 `200 OK` 的 SSE 里，
/// 此时没有可用的 HTTP 状态码。若沿用 `classify_workbuddy(None, 200, ...)`，
/// 所有未知码都会落到 `ClientError { status: 200 }`——风控码（4010/4015/4017）
/// 会被当成「请求级错误」，既不合警也不冷却，等于把封禁信号丢了。
///
/// 这里先按 adapter 查各自的码表，查不到才退化成「不可重试的客户端错误」。
pub fn classify_business_error(
    adapter: crate::models::adapter::PlatformAdapter,
    code: Option<i64>,
    message: &str,
) -> UpstreamFailure {
    use crate::models::adapter::PlatformAdapter;
    let Some(code) = code else {
        return UpstreamFailure::ClientError {
            status: 502,
            message: message.to_string(),
        };
    };
    match adapter {
        PlatformAdapter::Trae => classify_trae(code, message),
        PlatformAdapter::WorkBuddy => {
            // 用 200 作为「没有 HTTP 状态码」的占位：classify_workbuddy 的
            // 码表分支会先命中，只有码表完全不认识时才会看 status。
            let failure = classify_workbuddy(Some(code), 200, message);
            // 未知码 + 占位 200 会落成 `ClientError { status: 200 }`——
            // 对客户端回 200 是错的（那是「成功」）。业务错误没有真实状态码时，
            // 统一按 502 处理：让调用方知道是上游协议层面出了问题。
            match failure {
                UpstreamFailure::ClientError { status, message } if status < 400 => {
                    UpstreamFailure::ClientError { status: 502, message }
                }
                other => other,
            }
        }
        // 标准协议不该走到这里（它们的错误是 HTTP 状态码），保守按请求级处理
        _ => UpstreamFailure::ClientError {
            status: 502,
            message: message.to_string(),
        },
    }
}

/// Trae 业务码 → 语义。来源：社区实测 `docs/PROTOCOL.md`。
pub fn classify_trae(code: i64, message: &str) -> UpstreamFailure {
    let msg = if message.is_empty() {
        format!("Trae 上游错误 {code}")
    } else {
        format!("Trae 上游错误 {code}: {message}")
    };
    match code {
        // 模型名不在当前区域目录。官方文案是「请稍后重试」，极具误导性——
        // 它不可重试，换号也没用。
        4001 => UpstreamFailure::ClientError { status: 400, message: msg },
        // 套餐权益不足（模型在订阅档位外）。模型级，勿换号。
        1005 => UpstreamFailure::ClientError { status: 400, message: msg },
        // 账号配额耗尽。账号级，换号 + 冷却。
        4008 => UpstreamFailure::QuotaExhausted { retry_after_secs: 60, message: msg },
        // 检测到风险账号，已自动登出。立即禁用平台。
        4010 => UpstreamFailure::RiskControl { cooldown_secs: 3600, message: msg },
        // 帐户/IP 存在风险。长冷却，换号无效。
        4015 => UpstreamFailure::RiskControl { cooldown_secs: 3600, message: msg },
        // 设备/IP 级风控。
        4017 => UpstreamFailure::RiskControl { cooldown_secs: 3600, message: msg },
        // 模型未知（BYOK 槽位未配 Key）。
        4023 => UpstreamFailure::ClientError { status: 400, message: msg },
        // 限流。
        3004 | 4007 => UpstreamFailure::UpstreamBusy { retry_after_secs: 30, message: msg },
        // 用量上限。注意：UA 不对时也会误报这个码，所以不能单独当限流处理。
        4011 => UpstreamFailure::UpstreamBusy { retry_after_secs: 60, message: msg },
        _ => UpstreamFailure::ClientError { status: 502, message: msg },
    }
}

/// WorkBuddy 业务码 → 语义。来源：社区逆向（官方错误码页没有「积分耗尽」专属码）。
pub fn classify_workbuddy(code: Option<i64>, status: u16, message: &str) -> UpstreamFailure {
    let msg = match code {
        Some(code) => format!("WorkBuddy 上游错误 {code}: {message}"),
        None => format!("WorkBuddy 上游错误 HTTP {status}: {message}"),
    };
    // 402 = 计费余额耗尽，最严，必须先判
    if status == 402 {
        return UpstreamFailure::QuotaExhausted { retry_after_secs: 3600, message: msg };
    }
    match code {
        // 账号积分耗尽。**必须先于通用 429 判定**，否则会误冷却 12 小时。
        Some(14018) => UpstreamFailure::QuotaExhausted { retry_after_secs: 60, message: msg },
        // 模型级限流
        Some(6004) => UpstreamFailure::UpstreamBusy { retry_after_secs: 60, message: msg },
        // 渠道容量满载：不罚号
        Some(6021) => UpstreamFailure::UpstreamBusy { retry_after_secs: 30, message: msg },
        // 内容审核 / 通道校验失败：不降级
        Some(11128) | Some(11140) => {
            UpstreamFailure::ClientError { status: 403, message: msg }
        }
        // 参数非法
        Some(11133) | Some(11134) => {
            UpstreamFailure::ClientError { status: 400, message: msg }
        }
        // session 失效（并发刷新反重放）
        Some(12153) => UpstreamFailure::Unauthorized { message: msg },
        _ => match status {
            401 | 403 => UpstreamFailure::Unauthorized { message: msg },
            429 => UpstreamFailure::UpstreamBusy { retry_after_secs: 60, message: msg },
            500..=599 => UpstreamFailure::Transport { message: msg },
            _ => UpstreamFailure::ClientError { status, message: msg },
        },
    }
}

/// 通用 HTTP 状态码 → 语义（标准 OpenAI / Anthropic 上游，以及未识别的私有协议响应）。
///
/// 这里必须处理 **402**：Agnes 额度耗尽返回 402 而不是 429。
pub fn classify_http(status: u16, message: &str) -> UpstreamFailure {
    match status {
        401 => UpstreamFailure::Unauthorized { message: message.to_string() },
        // 402 Payment Required：Agnes 等上游用它表示额度耗尽
        402 => UpstreamFailure::QuotaExhausted {
            retry_after_secs: 60,
            message: message.to_string(),
        },
        403 => UpstreamFailure::ClientError { status: 403, message: message.to_string() },
        429 => UpstreamFailure::UpstreamBusy {
            retry_after_secs: 60,
            message: message.to_string(),
        },
        500..=599 => UpstreamFailure::Transport { message: message.to_string() },
        _ => UpstreamFailure::ClientError { status, message: message.to_string() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trae_client_errors_never_rotate_accounts() {
        // 4001/1005/4023 换号也不会成功，必须不重试
        for code in [4001, 1005, 4023] {
            let failure = classify_trae(code, "");
            assert!(
                !failure.should_try_another_backend(),
                "{code} 不该换后端"
            );
            assert!(!failure.should_cooldown(), "{code} 不该冷却账号");
        }
    }

    #[test]
    fn trae_quota_rotates_and_cools() {
        let failure = classify_trae(4008, "quota");
        assert!(failure.should_try_another_backend());
        assert!(failure.should_cooldown());
        assert_eq!(failure.client_status(), 429);
    }

    #[test]
    fn trae_risk_control_does_not_rotate() {
        // 关键：4010/4015/4017 换号会放大封禁（同出口连撞 → IP 被封）
        for code in [4010, 4015, 4017] {
            let failure = classify_trae(code, "risk");
            assert!(
                !failure.should_try_another_backend(),
                "{code} 换号会放大封禁，不该换"
            );
            assert!(failure.should_cooldown(), "{code} 必须冷却");
            assert!(failure.should_alert(), "{code} 必须告警");
            assert_eq!(failure.client_status(), 429);
        }
    }

    #[test]
    fn workbuddy_402_is_quota_not_server_error() {
        let failure = classify_workbuddy(None, 402, "insufficient balance");
        assert!(matches!(failure, UpstreamFailure::QuotaExhausted { .. }));
        assert_eq!(failure.client_status(), 429);
    }

    #[test]
    fn workbuddy_14018_checked_before_generic_429() {
        // 14018 是「账号积分耗尽」，不能被当成普通 429 限流（那会误冷却 12 小时）
        let failure = classify_workbuddy(Some(14018), 429, "credits exhausted");
        assert!(matches!(failure, UpstreamFailure::QuotaExhausted { .. }));
        assert_eq!(failure.cooldown_secs(), Some(60));

        // 普通 429 没有 14018 时才是 UpstreamBusy
        let busy = classify_workbuddy(None, 429, "rate limited");
        assert!(matches!(busy, UpstreamFailure::UpstreamBusy { .. }));
    }

    #[test]
    fn workbuddy_content_audit_does_not_degrade() {
        // 11128 / 11140 都是 "request illegal"，但性质是内容审核/通道校验，不降级
        for code in [11128, 11140] {
            let failure = classify_workbuddy(Some(code), 403, "request illegal");
            assert!(matches!(failure, UpstreamFailure::ClientError { .. }));
            assert!(!failure.should_try_another_backend());
        }
    }

    #[test]
    fn agnes_402_maps_to_quota_and_returns_429() {
        // Agnes 用 402 表示额度耗尽；对客户端必须回 429，否则客户端不识别
        let failure = classify_http(402, "insufficient quota");
        assert!(matches!(failure, UpstreamFailure::QuotaExhausted { .. }));
        assert_eq!(failure.client_status(), 429);
    }

    #[test]
    fn standard_http_statuses_map_correctly() {
        assert!(matches!(classify_http(401, ""), UpstreamFailure::Unauthorized { .. }));
        assert!(matches!(classify_http(403, ""), UpstreamFailure::ClientError { .. }));
        assert!(matches!(classify_http(429, ""), UpstreamFailure::UpstreamBusy { .. }));
        assert!(matches!(classify_http(500, ""), UpstreamFailure::Transport { .. }));
        assert!(matches!(classify_http(503, ""), UpstreamFailure::Transport { .. }));
        assert!(matches!(classify_http(400, ""), UpstreamFailure::ClientError { .. }));
    }

    #[test]
    fn transport_and_busy_do_not_cooldown_accounts() {
        assert!(!UpstreamFailure::Transport { message: String::new() }.should_cooldown());
        assert!(!UpstreamFailure::UpstreamBusy {
            retry_after_secs: 30,
            message: String::new()
        }
        .should_cooldown());
        assert!(!UpstreamFailure::ClientError {
            status: 400,
            message: String::new()
        }
        .should_cooldown());
    }

    #[test]
    fn in_stream_errors_use_per_adapter_code_tables() {
        use crate::models::adapter::PlatformAdapter;
        // 同一个码在两家语义不同：4008 是 Trae 的配额耗尽；
        // 对 WorkBuddy 它是未知码（其码表里没有 4008）。
        let trae = classify_business_error(PlatformAdapter::Trae, Some(4008), "quota");
        assert!(matches!(trae, UpstreamFailure::QuotaExhausted { .. }));
        assert_eq!(trae.client_status(), 429);

        let wb = classify_business_error(PlatformAdapter::WorkBuddy, Some(4008), "unknown");
        assert!(matches!(wb, UpstreamFailure::ClientError { .. }));
        // 未知码必须回 5xx，不能回 200（那是「成功」）
        assert!(
            wb.client_status() >= 500,
            "未知业务码不能回 2xx，实际 {}",
            wb.client_status()
        );
    }

    #[test]
    fn trae_risk_codes_stay_risk_when_classified_in_stream() {
        use crate::models::adapter::PlatformAdapter;
        // 关键回归：4010/4015/4017 是 Trae 的风控码，流内出现时必须仍然是
        // RiskControl（长冷却 + 告警 + 不换号），不能被降级成普通 ClientError。
        for code in [4010, 4015, 4017] {
            let failure = classify_business_error(PlatformAdapter::Trae, Some(code), "risk");
            assert!(
                matches!(failure, UpstreamFailure::RiskControl { .. }),
                "{code} 必须是 RiskControl，实际 {failure:?}"
            );
            assert!(!failure.should_try_another_backend(), "{code} 不该换号");
            assert!(failure.should_alert(), "{code} 必须告警");
        }
    }

    #[test]
    fn workbuddy_quota_code_stays_quota_in_stream() {
        use crate::models::adapter::PlatformAdapter;
        let failure = classify_business_error(PlatformAdapter::WorkBuddy, Some(14018), "credits exhausted");
        assert!(matches!(failure, UpstreamFailure::QuotaExhausted { .. }));
        assert_eq!(failure.client_status(), 429);
        assert!(failure.should_try_another_backend());
    }

    #[test]
    fn missing_code_in_stream_is_502_not_200() {
        use crate::models::adapter::PlatformAdapter;
        for adapter in [PlatformAdapter::Trae, PlatformAdapter::WorkBuddy] {
            let failure = classify_business_error(adapter, None, "no code");
            assert_eq!(
                failure.client_status(),
                502,
                "{adapter:?} 缺码时必须回 502 而不是 2xx"
            );
        }
    }

    #[test]
    fn standard_adapter_business_error_is_conservative() {
        use crate::models::adapter::PlatformAdapter;
        // 标准协议不该有流内业务码；万一有，按请求级 502 处理，不能假装成功
        let failure = classify_business_error(PlatformAdapter::OpenAI, Some(123), "odd");
        assert_eq!(failure.client_status(), 502);
        assert!(!failure.should_try_another_backend());
    }
}
