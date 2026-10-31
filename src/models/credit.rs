use serde::{Deserialize, Serialize};

/// 平台积分读数（管理接口与平台列表共用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformCreditUsage {
    pub platform_id: String,
    pub platform_name: String,
    /// 配置的预算（平台编辑页里那条数字），仅作参考与告警阈值
    pub budget: f64,
    /// 当前窗口实际可用的总额：领取同步到上游余额时是余额，否则等于 budget
    pub budget_effective: f64,
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
    /// 领取时同步到的站点余额（目前只有魔搭）；None = 没有实时读数
    pub live_balance: Option<f64>,
    /// 余额的同步时间（UTC RFC3339）；与 live_balance 成对出现
    pub balance_at: Option<String>,
    pub exhausted: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreditAdjustRequest {
    /// 正数补记、负数冲减；0 和超过 ±1e6 的幅度会被拒绝
    pub delta: f64,
    #[serde(default)]
    pub note: Option<String>,
}
