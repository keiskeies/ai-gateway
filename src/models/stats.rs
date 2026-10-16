use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestStat {
    pub id: i64,
    pub proxy_id: String,
    pub route_id: String,
    pub backend_id: String,
    pub status_code: i32,
    pub latency_ms: i64,
    pub token_input: Option<i64>,
    pub token_output: Option<i64>,
    pub error_type: Option<String>,
    pub created_at: String,
    /// 流式请求首字时长（发出请求 → 收到上游第一个 chunk 的毫秒数）；非流式为 None
    pub ttft_ms: Option<i64>,
    /// 是否流式请求
    pub is_stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewStats {
    pub total_requests: i64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub active_proxies: i64,
    pub total_proxies: i64,
    pub active_platforms: i64,
    pub total_platforms: i64,
    pub total_models: i64,
    pub total_token_input: i64,
    pub total_token_output: i64,
    /// 当前在途请求数（来自 selector 连接计数器，概览大屏并发指标）
    pub total_connections_active: i64,
    /// 上一同级周期（昨日）请求数，用于 KPI delta 徽章
    pub requests_prev_period: i64,
    /// 上一周期（昨日）平均延迟，用于 KPI delta 徽章
    pub avg_latency_prev_period: f64,
    /// 平均首字时长（仅流式请求；无数据时为 0）
    pub avg_ttft_ms: f64,
    /// 平均输出速度 tok/s（生成阶段口径，见 ModelTimeSeriesPoint::output_speed_tps）
    pub avg_output_speed_tps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyStats {
    pub proxy_id: String,
    pub proxy_name: String,
    pub total_requests: i64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub requests_today: i64,
    pub total_token_input: i64,
    pub total_token_output: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStats {
    pub backend_id: String,
    pub platform_name: String,
    pub model_name: String,
    pub total_requests: i64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub active_connections: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformStats {
    pub platform_id: String,
    pub platform_name: String,
    pub total_requests: i64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub total_token_input: i64,
    pub total_token_output: i64,
}

/// 模型时间序列统计单点数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTimeSeriesPoint {
    pub period: String,
    pub model_id: String,
    pub requests: i64,
    pub token_input: i64,
    pub token_output: i64,
    /// 该周期该模型的平均延迟（概览大屏延迟 sparkline 用）
    pub avg_latency_ms: f64,
    /// 失败请求数（非 2xx）
    pub failed_requests: i64,
    /// 流式请求数
    pub stream_requests: i64,
    /// 平均首字时长（仅该周期该模型的流式请求；无数据时为 0）
    pub avg_ttft_ms: f64,
    /// 平均输出速度 tok/s：SUM(token_output) / (SUM(latency) - SUM(ttft))，
    /// 即生成阶段口径；生成总时长为 0 时为 0
    pub output_speed_tps: f64,
}

/// 模型时间序列统计聚合结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTimeSeriesStats {
    pub granularity: String,
    /// 连续的时间周期列表（用于前端补零对齐 X 轴）
    pub periods: Vec<String>,
    /// 扁平数据点列表
    pub data: Vec<ModelTimeSeriesPoint>,
}

/// 模型维度汇总：选定时间范围内每个模型的全量统计（不按时段切分）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSummary {
    pub model_id: String,
    pub requests: i64,
    pub success_requests: i64,
    pub failed_requests: i64,
    pub stream_requests: i64,
    pub token_input: i64,
    pub token_output: i64,
    /// 平均总延时 ms
    pub avg_latency_ms: f64,
    /// 平均首字时长 ms（流式请求；无数据为 0）
    pub avg_ttft_ms: f64,
    /// 平均输出速度 tok/s（生成阶段口径）
    pub output_speed_tps: f64,
}

/// 请求明细单行（概览大屏请求日志表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLogItem {
    pub id: i64,
    pub created_at: String,
    pub model_id: String,
    pub platform_name: String,
    pub proxy_name: String,
    pub status_code: i32,
    pub token_input: Option<i64>,
    pub token_output: Option<i64>,
    pub latency_ms: i64,
    pub ttft_ms: Option<i64>,
    /// 单条输出速度 tok/s（生成阶段口径；无 token 或无生成时长时为 None）
    pub output_speed_tps: Option<f64>,
    pub is_stream: bool,
    pub error_type: Option<String>,
}

/// 请求明细分页结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLogPage {
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub items: Vec<RequestLogItem>,
}

/// 错误类型分布单项（概览大屏错误分析面板）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBreakdown {
    pub kind: String,
    pub count: i64,
}

/// 错误分析聚合：总量 / 成功数 / 按 error_type 的失败分布
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorStats {
    pub total: i64,
    pub success: i64,
    pub errors: Vec<ErrorBreakdown>,
}

/// 活跃热力图单格：weekday(0=周日) × hour(0-23)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatmapCell {
    pub weekday: i64,
    pub hour: i64,
    pub count: i64,
}

/// 周 × 时活跃热力图（按服务器本地时区聚合）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityHeatmap {
    pub cells: Vec<HeatmapCell>,
    pub max: i64,
}

/// 延迟分位数（均值会被流式长请求拉高，分位数更接近真实体验）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    pub p50: f64,
    pub p90: f64,
    pub p99: f64,
    pub max: f64,
}
