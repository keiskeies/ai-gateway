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
    /// 命中上游提示词缓存的 token 数（无缓存数据为 None）
    pub cache_read_tokens: Option<i64>,
    /// 缓存写入 token 数（仅 Anthropic cache_creation 有此概念；无数据为 None）
    pub cache_write_tokens: Option<i64>,
    /// 是否流式请求
    pub is_stream: bool,
    /// 发起请求的网关 API Key（未鉴权或旧数据为 None）
    pub api_key_id: Option<String>,
    /// 成本估算：单价齐备（入与出都按每 100 万 token 定价）且 token 数齐备时才算得出，
    /// 否则为 None（未定价模型、只填一侧、上游没回 usage、失败请求、V15 之前的旧行）
    pub cost: Option<f64>,
    /// 缓存命中相对未命中价的价差（V26，请求时冻结）：正数=节省，负数=命中反而更贵。
    /// 上游没回缓存数据 / 模型未定价 → None；有定价但零命中 → Some(0)。
    pub cache_savings: Option<f64>,
    /// 网关侧请求 ID（V16）：同一次请求的所有尝试共享，用于关联 `request_attempts`；
    /// 迁移前写入的旧行为 None，前端据此显示"轨迹不可用"而不是伪造链路
    pub request_id: Option<String>,
    /// 这条请求是不是重放出来的：存被重放那条 `request_stats.id`（V21）。
    /// 重放会产生费用与副作用，所以新行必须能看出它是重放，而不是又一条普通流量。
    pub replayed_from: Option<i64>,
    /// 入站路径（V23）：/v1/chat/completions、/v1/completions 或 /v1/messages。
    /// 只记录路径不记录 query，避免把查询串里的凭据带进统计。
    pub path: Option<String>,
    /// 本条请求计入上游平台的积分（V24）；未启用积分/失败请求为 None。
    pub credits: Option<f64>,
}

/// 入站请求正文，仅在 `defaults.capture_request_bodies` 开启时落库（V21）。
/// 存的是调用方送来的原始 JSON，不是转换后的上游请求——重放要还原的是"客户端发了什么"，
/// 上游那一份每次重放都会按当时的路由与后端重新生成。
/// 超过采集上限时只存前缀（V27，`truncated = true`）：详情仍可读，但这份正文不能重放。
#[derive(Debug, Clone)]
pub struct RequestBody {
    pub request_id: String,
    /// 入站协议（`openai` / `anthropic`）：决定重放时用哪个解析器
    pub protocol: String,
    pub body: String,
    /// 原始正文的字节数（截断时大于 `body.len()`）
    pub bytes: i64,
    pub truncated: bool,
    pub created_at: String,
}

/// 一次请求里"发往某个后端"的单条尝试。重试与 fallback 各占一行，
/// 因此最终失败的请求也能看出依次试过哪些后端。
/// 不落库：请求/响应正文、首字时长（ttft 只在最终那条 `request_stats` 上）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestAttempt {
    pub id: i64,
    pub request_id: String,
    /// 第几次尝试，从 1 开始
    pub attempt_no: i64,
    pub backend_id: Option<String>,
    pub platform_id: Option<String>,
    /// 实际发往上游的模型 ID（与虚拟模型名可能不同）
    pub model_id: Option<String>,
    /// HTTP 状态；连接失败为 0，尝试还没拿到响应时为 None
    pub status_code: Option<i64>,
    pub error_type: Option<String>,
    /// 截断后的错误摘要（上限 300 字符），不含密钥与请求正文
    pub error_message: Option<String>,
    pub latency_ms: Option<i64>,
    pub started_at: String,
    /// 本次尝试注入的前置随机串（路由开关关闭时为 None）
    pub nonce: Option<String>,
    /// 本次尝试实际请求的上游完整地址（V23，已把 query 里的凭据打码）。
    /// 旧行为 None——旧数据没有这个字段，不是“没发请求”。
    pub forward_url: Option<String>,
    /// JOIN platforms 得到的展示名；平台已删除时为 None
    pub platform_name: Option<String>,
}

/// 入站请求头（V23）。仅在 `capture_request_bodies` 开启时落库，
/// 且只收白名单头；Authorization / x-api-key / Cookie 等敏感头永远不会出现在这里。
#[derive(Debug, Clone)]
pub struct RequestHeaders {
    pub request_id: String,
    pub headers: String,
    pub bytes: i64,
    pub truncated: bool,
    pub created_at: String,
}

/// 上游原始响应正文（V23）。流式请求存的是原始 SSE 字节（截断到上限），
/// 不是客户端最终收到的转换结果。
#[derive(Debug, Clone)]
pub struct ResponseBody {
    pub request_id: String,
    pub protocol: String,
    pub status_code: i64,
    pub content_type: Option<String>,
    pub body: String,
    /// 上游响应的原始字节数（截断时大于实际存下的长度）
    pub bytes: i64,
    pub truncated: bool,
    pub created_at: String,
}

/// 详情接口里“请求侧”的读数：请求头与请求正文分别判断，
/// 长请求可以只有头、没有正文。
#[derive(Debug, Clone, Serialize)]
pub struct RequestPayloadRequest {
    pub headers_captured: bool,
    pub body_captured: bool,
    pub protocol: Option<String>,
    pub headers: Option<serde_json::Value>,
    pub body: Option<String>,
    /// 请求正文原始字节数（截断时大于实际存下的长度）
    pub bytes: i64,
    pub headers_truncated: bool,
    pub body_truncated: bool,
}

/// 详情接口里“响应侧”的读数；未采集时 body 为 None。
#[derive(Debug, Clone, Serialize)]
pub struct RequestPayloadResponse {
    pub captured: bool,
    pub protocol: Option<String>,
    pub status_code: Option<i64>,
    pub content_type: Option<String>,
    pub body: Option<String>,
    pub bytes: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestPayload {
    pub request: RequestPayloadRequest,
    pub response: RequestPayloadResponse,
    /// 这条请求最后一次上游尝试注入的前置随机串；开关没开时为 None。
    /// Traffic 里展示的是入站正文，随机串只存在于转发出去的那一份，所以单独带出来。
    pub nonce: Option<String>,
    /// 当前采集开关：用来区分“这次没记”和“开关没开”，前端才能给出正确提示
    pub request_capture_enabled: bool,
    pub response_capture_enabled: bool,
}

/// 一次请求的尝试轨迹：某条 `request_stats` 行对应的全部尝试
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestTrace {
    /// 明细行 ID
    pub request_stat_id: i64,
    /// V16 之前或未经网关的请求为 None，此时 attempts 为空
    pub request_id: Option<String>,
    pub attempts: Vec<RequestAttempt>,
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
    /// 缓存命中 token 总量（范围内 SUM(cache_read_tokens)）
    pub total_cache_read_tokens: i64,
    /// 新增输入 token（总输入 − 缓存命中 − 缓存写入，即真正新鲜计费的 prompt）
    pub fresh_input_tokens: i64,
    /// 缓存命中率 %：cache_read / (cache_read + fresh_input)，与 CC Switch 口径一致；
    /// 分母（可缓存 prompt）为 0 时为 0
    pub cache_hit_rate: f64,
    /// 成本估算合计：范围内所有已定价请求的 cost 之和；一条都没有时为 None（区别于 0）
    pub total_cost: Option<f64>,
    /// 缓存价差合计：命中相对未命中价省下的钱（负数=命中反而更贵）；
    /// 范围内没有可比较的请求（未定价/上游没回缓存）时为 None
    pub total_cache_savings: Option<f64>,
    /// 参与成本估算的请求数（cost 非 NULL 的行数）；覆盖率 = priced_requests / total_requests
    pub priced_requests: i64,
    /// 配了高峰价（且启用）的模型数；用于大屏说明"高峰价覆盖了几个模型"
    pub peak_priced_models: i64,
    /// 当前正落在高峰窗口里的模型数（0 = 全部按基础价）
    pub peak_active_models: i64,
    /// 下一次计费档位切换的本地时刻 HH:MM；没有任何高峰配置时为 None
    pub peak_next_at: Option<String>,
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
    /// 成本估算合计（无已定价请求时为 None）
    pub total_cost: Option<f64>,
    /// 参与成本估算的请求数
    pub priced_requests: i64,
}

/// 删除虚拟模型会被一并销毁的历史数据量（不受时间范围影响，只用于删除确认的披露）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyHistoryImpact {
    /// 该模型名下的请求统计行数
    pub requests: i64,
    /// 这些请求的重试轨迹行数
    pub attempts: i64,
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
    /// 成本估算合计（无已定价请求时为 None）
    pub total_cost: Option<f64>,
    /// 参与成本估算的请求数
    pub priced_requests: i64,
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
    /// 该周期该模型的缓存命中 token 总量
    pub cache_read_tokens: i64,
    /// 该周期该模型的缓存写入 token 总量（仅 Anthropic 等支持 cache_creation 的上游）
    pub cache_write_tokens: i64,
    /// 新增输入 token = 总输入 − 缓存命中 − 缓存写入（即真正新鲜计费的 prompt）
    pub fresh_input_tokens: i64,
    /// 成本估算合计：该周期该模型所有已定价请求的 cost 之和；一条都没有时为 None（区别于 0）
    pub total_cost: Option<f64>,
    /// 参与成本估算的请求数（cost 非 NULL 的行数），用于逐桶成本覆盖率
    pub priced_requests: i64,
    /// 有首字样本的请求数（ttft_ms 非 NULL）。0 表示"没有流式样本"，与"首字为 0ms"是两件事
    pub ttft_samples: i64,
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
    /// 缓存命中 token 总量
    pub cache_read_tokens: i64,
    /// 新增输入 token（总输入 − 缓存命中 − 缓存写入）
    pub fresh_input_tokens: i64,
    /// 缓存命中率 %：cache_read / (cache_read + fresh_input)；分母为 0 时为 0
    pub cache_hit_rate: f64,
    /// 成本估算合计（该模型的行全部未定价时为 None）
    pub total_cost: Option<f64>,
    /// 缓存价差合计（正数=节省，负数=加价）；该模型没有可比较的请求时为 None
    pub cache_savings: Option<f64>,
    /// 参与成本估算的请求数
    pub priced_requests: i64,
}

/// 请求明细筛选：`/api/stats/requests` 的查询串直接反序列化，同时作为 db 层入参。
/// 空值（None / 空串）一律表示"不筛"。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RequestFilter {
    pub from: Option<String>,
    pub to: Option<String>,
    /// 后端模型 id，精确匹配
    pub model: Option<String>,
    /// success / fail，其余值视为全部
    pub status: Option<String>,
    /// 平台 id
    pub platform: Option<String>,
    /// 虚拟模型（proxy）id
    pub proxy: Option<String>,
    /// API Key id
    pub api_key: Option<String>,
    /// 错误类型，精确匹配
    pub error_type: Option<String>,
    /// 跨模型 / 平台 / 虚拟模型 / Key 名称的模糊搜索
    pub q: Option<String>,
    /// 延迟下限（毫秒），含
    pub latency_min: Option<i64>,
    /// 延迟上限（毫秒），含
    pub latency_max: Option<i64>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    /// 排序列（前端列 id）。只有 `db::stats::requests_order` 白名单里的键会被翻译成 SQL，
    /// 其余值一律回落到默认的时间倒序——ORDER BY 不能参数化，所以这里靠白名单而不是绑定。
    pub sort: Option<String>,
    /// `asc` 才是升序，其余（含缺省）都是降序
    pub dir: Option<String>,
    /// 导出行数上限，只有 `/api/stats/requests/export` 读它；放在同一个结构里是为了让
    /// 列表和导出共用同一份查询串定义，否则两边筛选迟早长得不一样。
    pub limit: Option<i64>,
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
    /// 命中缓存的 token 数（无缓存数据为 None）
    pub cache_read_tokens: Option<i64>,
    /// 缓存写入 token 数（仅 Anthropic；无数据为 None）
    pub cache_write_tokens: Option<i64>,
    /// 新增输入 token = 总输入 − 缓存命中 − 缓存写入（无输入 token 时为 None）
    pub fresh_input_tokens: Option<i64>,
    pub is_stream: bool,
    pub error_type: Option<String>,
    /// 发起请求的 API Key 名称（未鉴权或旧数据为 None）
    pub api_key_name: Option<String>,
    /// 本条请求的成本估算（模型未定价、只填一侧、上游无 usage 或旧数据时为 None）
    pub cost: Option<f64>,
    /// 本条请求计入上游平台的积分（V24）；未启用积分、失败请求或旧数据为 None
    pub credits: Option<f64>,
    /// 入站路径（V23）；旧数据为 None
    pub path: Option<String>,
    /// 网关侧请求 ID；None 表示这条记录早于 V16，重试轨迹不可查
    pub request_id: Option<String>,
    /// 网关为这一笔请求发出的尝试次数（含重试与 fallback），至少为 1；
    /// None 表示这条记录早于 V16，次数无从查起——不是 0 次，0 次尝试的请求不存在
    pub attempts: Option<i64>,
    /// 这条请求的正文是否存着（正文采集开着时为 true）——重放按钮据此决定可用与否，
    /// 而不是点了之后才告诉用户"没存正文"
    pub has_body: bool,
    /// 由哪条请求重放而来（V21）；None 表示不是重放
    pub replayed_from: Option<i64>,
}

/// 密钥维度用量汇总：时间范围内每个 API Key 的请求/成功/失败/token/延迟
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyUsage {
    pub api_key_id: String,
    pub requests: i64,
    pub success_requests: i64,
    pub failed_requests: i64,
    pub token_input: i64,
    pub token_output: i64,
    pub avg_latency_ms: f64,
    pub last_used: Option<String>,
    /// 成本估算合计（该 Key 的请求全部未定价时为 None）
    pub total_cost: Option<f64>,
    /// 参与成本估算的请求数
    pub priced_requests: i64,
}

/// 单个密钥的模型维度拆分
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyModelUsage {
    pub model_id: String,
    pub requests: i64,
    pub failed_requests: i64,
    pub token_input: i64,
    pub token_output: i64,
    pub avg_latency_ms: f64,
    pub last_used: Option<String>,
    /// 成本估算合计（该 Key 在这个模型上的请求全部未定价时为 None）
    pub total_cost: Option<f64>,
    /// 参与成本估算的请求数，小于 requests 说明窗口内改过价或模型未定价
    pub priced_requests: i64,
}

/// 请求明细分页结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLogPage {
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub items: Vec<RequestLogItem>,
    /// 整个筛选集（不止当前页）的成本估算合计；一条已定价的都没有时为 None
    pub total_cost: Option<f64>,
    /// 筛选集中参与成本估算的请求数；覆盖率 = priced_requests / total
    pub priced_requests: i64,
    /// 整个筛选集的积分合计；一条计过积分的都没有时为 None
    pub total_credits: Option<f64>,
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

/// 平台连通性检测的单次记录（`POST /api/platforms/test` 对已保存平台的落库行）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformHealth {
    pub id: i64,
    pub platform_id: String,
    pub ok: bool,
    pub latency_ms: i64,
    pub model_count: i64,
    /// 失败原因（成功时为 None）
    pub error: Option<String>,
    pub created_at: String,
}

/// 平台健康汇总：时间范围内每平台一行，列表页 CHECK 列直接读这份
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformHealthSummary {
    pub platform_id: String,
    /// 范围内检测次数
    pub checks: i64,
    /// 范围内失败次数
    pub failures: i64,
    /// 范围内最后一次检测的结果；无检测时为 None（UI 显示"未检测"）
    pub last_ok: Option<bool>,
    pub last_latency_ms: Option<i64>,
    /// 最后一次检测拉到的模型数（Anthropic 这类无列表接口时为 0）
    pub last_model_count: Option<i64>,
    pub last_error: Option<String>,
    pub last_checked_at: Option<String>,
}
