export interface OverviewStats {
  total_requests: number
  success_rate: number
  avg_latency_ms: number
  active_proxies: number
  total_proxies: number
  active_platforms: number
  total_platforms: number
  total_models: number
  total_token_input: number
  total_token_output: number
  total_connections_active?: number
  requests_prev_period?: number
  avg_latency_prev_period?: number
  avg_ttft_ms?: number
  avg_output_speed_tps?: number
  total_cache_read_tokens?: number
  fresh_input_tokens?: number
  cache_hit_rate?: number
  /** 成本估算合计（按模型定价，每 100 万 token 计价）；筛选范围内一条已定价的都没有时为 null */
  total_cost?: number | null
  /** 缓存价差合计：正数=命中省下，负数=命中反而更贵；无可比较请求时为 null */
  total_cache_savings?: number | null
  /** 参与成本估算的请求数；覆盖率 = priced_requests / total_requests */
  priced_requests?: number
  /** 配了高峰价且启用、基础价完整的模型数 */
  peak_priced_models?: number
  /** 当前正落在高峰窗口里的模型数 */
  peak_active_models?: number
  /** 下一次计费档位切换的本地 HH:MM */
  peak_next_at?: string | null
}

export interface LatencyPercentiles {
  p50: number
  p90: number
  p99: number
  max: number
}

export interface ModelSummaryRow {
  model_id: string
  requests: number
  success_requests: number
  failed_requests: number
  stream_requests: number
  token_input: number
  token_output: number
  avg_latency_ms: number
  avg_ttft_ms: number
  output_speed_tps: number
  cache_read_tokens: number
  fresh_input_tokens: number
  cache_hit_rate: number
  total_cost?: number | null
  /** 缓存价差合计（正数=节省，负数=加价）；没有可比较的请求时为 null */
  cache_savings?: number | null
  priced_requests?: number
}

export interface RequestLogItem {
  id: number
  created_at: string
  model_id: string
  platform_name: string
  proxy_name: string
  status_code: number
  token_input: number | null
  token_output: number | null
  latency_ms: number
  ttft_ms: number | null
  output_speed_tps: number | null
  cache_read_tokens: number | null
  cache_write_tokens: number | null
  fresh_input_tokens: number | null
  is_stream: boolean
  error_type: string | null
  api_key_name: string | null
  /** 本条请求的成本估算；模型未定价、只填一侧、上游无 usage 或旧数据时为 null */
  cost: number | null
  /** 本条请求计入上游平台的积分（V24）；未启用积分、失败或旧数据时为 null */
  credits: number | null
  /** 入站路径（V23）；旧数据为 null */
  path: string | null
  /** 网关侧请求 ID（V16）；迁移前写入的旧行为 null，此时重试轨迹不可用 */
  request_id: string | null
  /** 网关为这一笔请求发出的尝试次数（含重试与 fallback）；null = 该行早于 V16，次数无从查起 */
  attempts: number | null
  /** 这一笔的原始正文是否存着（只有开启正文采集、且没超过大小上限时才有） */
  has_body: boolean
  /** 由重放产生时，被重放那条明细行的 ID；原始请求为 null */
  replayed_from: number | null
}

/** 一次请求里的单条尝试：重试与 fallback 各占一行，只含元数据 */
export interface RequestAttempt {
  id: number
  request_id: string
  /** 第几次尝试，从 1 开始 */
  attempt_no: number
  backend_id: string | null
  platform_id: string | null
  /** 实际发往上游的模型 ID */
  model_id: string | null
  /** 上游返回的状态码；连接根本没建立时为 null */
  status_code: number | null
  error_type: string | null
  /** 错误摘要，落库前已截断并脱敏 */
  error_message: string | null
  latency_ms: number | null
  started_at: string
  /** 本次尝试实际请求的上游完整地址（已打码）；旧数据为 null */
  forward_url: string | null
  /** 本次尝试注入的前置随机串；路由开关关闭或旧数据为 null */
  nonce?: string | null
  platform_name: string | null
}

export interface RequestTrace {
  request_stat_id: number
  /** null 表示这条明细行早于 V16，没有可关联的轨迹 */
  request_id: string | null
  attempts: RequestAttempt[]
}

export interface RequestLogPage {
  total: number
  page: number
  page_size: number
  items: RequestLogItem[]
  /** 整个筛选集（不止当前页）的成本合计；全未定价时为 null */
  total_cost?: number | null
  /** 筛选集中已定价的请求数 */
  priced_requests?: number
  /** 整个筛选集的积分合计；一条计过积分的都没有时为 null */
  total_credits?: number | null
}

/** 请求侧详情：请求头与正文分别判断，长请求可能只有头没有正文 */
export interface RequestPayloadRequest {
  headers_captured: boolean
  body_captured: boolean
  protocol: string | null
  headers: Record<string, unknown> | null
  body: string | null
  /** 正文原始字节数（截断时大于实际存下的长度） */
  bytes: number
  headers_truncated: boolean
  body_truncated: boolean
}

/** 响应侧详情：存的是上游原始响应（流式为原始 SSE），不是跨协议转换后的结果 */
export interface RequestPayloadResponse {
  captured: boolean
  protocol: string | null
  status_code: number | null
  content_type: string | null
  body: string | null
  bytes: number
  truncated: boolean
}

export interface RequestPayload {
  request: RequestPayloadRequest
  response: RequestPayloadResponse
  /** 最后一次上游尝试注入的前置随机串；路由开关没开时为 null */
  nonce?: string | null
  /** 当前采集开关：区分“这条没记”与“开关没开” */
  request_capture_enabled: boolean
  response_capture_enabled: boolean
}

export interface PlatformCreditUsage {
  platform_id: string
  platform_name: string
  budget: number
  window: 'day' | 'week' | 'month' | string
  spent: number
  remaining: number
  period_start: string
  reset_at: string
  unpriced_models: number
  exhausted: boolean
}

export interface ErrorBreakdown {
  kind: string
  count: number
}

export interface ErrorStats {
  total: number
  success: number
  errors: ErrorBreakdown[]
}

export interface ModelTimeSeriesPoint {
  period: string
  model_id: string
  requests: number
  token_input: number
  token_output: number
  avg_latency_ms?: number
  failed_requests?: number
  stream_requests?: number
  avg_ttft_ms?: number
  output_speed_tps?: number
  cache_read_tokens?: number
  /** 缓存写入 token 总量（目前仅 Anthropic 等支持 cache_creation 的上游） */
  cache_write_tokens?: number
  /** 新增输入 = 总输入 − 缓存读 − 缓存写；真正新鲜计费的 prompt */
  fresh_input_tokens?: number
  /** 该桶成本合计；全未定价时为 null（不是 0） */
  total_cost?: number | null
  /** 该桶参与成本估算的请求数 */
  priced_requests?: number
  /** 该桶有首字样本的请求数；0 = 没有流式样本，不是首字 0ms */
  ttft_samples?: number
}

export interface ModelTimeSeriesStats {
  granularity: string
  periods: string[]
  data: ModelTimeSeriesPoint[]
}

export interface ApiKeyRow {
  id: string
  name: string
  // 掩码（头尾各 4 位），不是凭据：库里只存 sha256 摘要，列表接口拿不回完整 Key
  key: string
  // 一次性明文：只出现在"创建/轮换"那一次响应里，列表里永远没有
  secret?: string | null
  proxy_id?: string | null
  created_at: string
  last_used?: string | null
  enabled?: boolean
  // 到期时间（RFC3339 UTC）；null = 不过期
  expires_at?: string | null
  // 服务端算好的过期标记：前端不必自己解析时间格式再比时钟
  expired?: boolean
  quota_requests?: number | null
  quota_tokens?: number | null
  quota_cost?: number | null
  // 'day' | 'week' | 'month'；null = 累计（这条 Key 建立至今）
  quota_window?: string | null
  // 配额的分母：这把 Key 在现存统计里、按自己的配额窗口算出的用量
  used_requests?: number
  used_tokens?: number
  // 金额只累加已定价的请求，所以它总是和 priced_requests / used_requests 一起出现
  used_cost?: number | null
  priced_requests?: number
}

export interface ApiKeyUsage {
  api_key_id: string
  requests: number
  success_requests: number
  failed_requests: number
  token_input: number
  token_output: number
  avg_latency_ms: number
  last_used: string | null
  total_cost: number | null
  priced_requests: number
}

export interface ApiKeyModelUsage {
  model_id: string
  requests: number
  failed_requests: number
  token_input: number
  token_output: number
  avg_latency_ms: number
  last_used: string | null
  total_cost: number | null
  priced_requests: number
}

export interface ProxyRow {
  id: string
  name: string
  capabilities?: string[]
  created_at?: string
}

export interface ProxyStats {
  proxy_id?: string
  proxy_name?: string
  total_requests: number
  success_rate: number
  avg_latency_ms: number
  requests_today?: number
  total_token_input: number
  total_token_output: number
  total_cost?: number | null
  priced_requests?: number
}

export interface PlatformRow {
  id: string
  name: string
  type?: string
  platform_type?: string
  /** 上游目录键：同 type + base_url 的账号共享同一份模型目录 */
  catalog_key?: string | null
}

export interface RouteRow {
  id: string
  proxy_id?: string
  lb_strategy?: string
  backends?: { id: string; platform_id?: string; model_id?: string; weight?: number }[]
}

export interface PlatformDetail {
  id: string
  name: string
  type: string
  base_url: string
  api_key: string
  organization?: string | null
  custom_headers?: Record<string, unknown>
  status: string
  rate_limit?: { rpm?: number; tpm?: number } | null
  /** 上游积分预算；null/undefined = 不启用积分 */
  credit_budget?: number | null
  /** 自然窗口：day / week / month */
  credit_window?: string | null
  /** 上游目录键：同 type + base_url 的账号共享模型目录 */
  catalog_key?: string | null
  created_at: string
  updated_at: string
}

export interface PlatformStats {
  platform_id: string
  platform_name: string
  total_requests: number
  success_rate: number
  avg_latency_ms: number
  total_token_input: number
  total_token_output: number
  total_cost?: number | null
  priced_requests?: number
}

/** 时段内每平台连通健康汇总；last_* 为范围内最新一次检测，无检测时为 null */
export interface PlatformHealthSummary {
  platform_id: string
  checks: number
  failures: number
  last_ok: boolean | null
  last_latency_ms: number | null
  last_model_count: number | null
  last_error: string | null
  last_checked_at: string | null
}

/** 单次连通性检测记录 */
export interface PlatformHealthRecord {
  id: number
  platform_id: string
  ok: boolean
  latency_ms: number
  model_count: number
  error: string | null
  created_at: string
}
