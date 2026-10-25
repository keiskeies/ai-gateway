use serde::{Deserialize, Serialize};

/// 平台积分读数（管理接口与平台列表共用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformCreditUsage {
    pub platform_id: String,
    pub platform_name: String,
    pub budget: f64,
    /// day / week / month
    pub window: String,
    /// 当前自然窗口内已用
    pub spent: f64,
    /// 剩余（不小于 0）
    pub remaining: f64,
    /// 当前窗口起点（UTC RFC3339）
    pub period_start: String,
    /// 下一个窗口起点（UTC RFC3339），429 与界面据此告诉用户何时恢复
    pub reset_at: String,
    /// 启用积分但未填单价的模型数：这些模型按 0 计，不会被拦截
    pub unpriced_models: i64,
    pub exhausted: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreditAdjustRequest {
    /// 正数补记、负数冲减；0 和超过 ±1e6 的幅度会被拒绝
    pub delta: f64,
    #[serde(default)]
    pub note: Option<String>,
}
