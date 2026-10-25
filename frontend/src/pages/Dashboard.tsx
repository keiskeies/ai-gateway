import { useEffect, useState, useCallback, useMemo, useRef, Fragment } from 'react'
import {
  Group,
  Stack,
  ActionIcon,
  Tooltip,
  SegmentedControl,
  LoadingOverlay,
  Table,
  Pagination,
  Select,
} from '@mantine/core'
import {
  LineChart as ReLineChart,
  Line,
  BarChart as ReBarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip as RTooltip,
  ResponsiveContainer,
  PieChart,
  Pie,
  Cell,
  Label,
} from 'recharts'
import {
  IconRefresh,
  IconPlayerPause,
  IconPlayerPlay,
  IconArrowsMaximize,
  IconArrowsMinimize,
  IconStack2,
  IconLayoutGrid,
  IconBolt,
  IconCircleCheck,
  IconStopwatch,
  IconCoin,
  IconStack,
  IconApi,
  IconRocket,
  IconDatabase,
  IconAlertTriangle,
  IconX,
} from '@tabler/icons-react'
import { getOverview, listProxies, listPlatforms, getProxyStats, getPlatformStats, getModelTimeSeries, getErrorStats, getHeatmap, getLatencyPercentiles, getModelSummaries, getRequestLogs } from '../api'
import { ModelTimeline } from '../components/ModelTimeline'
import { ChartAltTable, peakOf } from '../components/ui/ChartAlt'
import { Readout } from '../components/ui/Readout'
import type { ModelSummaryRow, ModelTimeSeriesPoint, ModelTimeSeriesStats } from '../types/stats'
import { useAppContext } from '../ThemeContext'
import { t, type Locale, type TranslationKey } from '../i18n'
import { getPresetName, platformPresets } from '../presets'
import { useNavigate } from 'react-router-dom'
// 成本格式化与大屏/控制台共用同一套规则：NULL 渲染成 —，绝不显示 0
import { formatCost, formatCostCoverage } from '../lib/format'

// ============================================================
// 概览监控大屏 · Quiet Analytics + Wall Display
// 设计稿：docs/dashboard-screen-design.md
// 单一强调色（蓝）、明度分层、tabular-nums 大数字、克制微动效。
// ============================================================

interface OverviewStats {
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
  // 后端轻量增量（§6）：缺失时 delta 徽章隐藏、并发退化为 0
  total_connections_active?: number
  requests_prev_period?: number
  avg_latency_prev_period?: number
  // 性能指标（V7）：全局平均首字 / 输出速度
  avg_ttft_ms?: number
  avg_output_speed_tps?: number
  // 缓存命中（V8，CC Switch 口径）
  total_cache_read_tokens?: number
  fresh_input_tokens?: number
  cache_hit_rate?: number
  // 成本估算（V15）：NULL = 窗口内没有任何"两侧都定价"的请求，与"成本为 0"是两件事
  total_cost?: number | null
  priced_requests?: number
}

interface ProxyWithStats {
  id: string
  name: string
  stats?: {
    total_requests: number
    success_rate: number
    avg_latency_ms: number
    total_token_input: number
    total_token_output: number
  }
}

interface PlatformWithStats {
  id: string
  name: string
  platform_type: string
  base_url: string
  stats?: {
    total_requests: number
    success_rate: number
    avg_latency_ms: number
    total_token_input: number
    total_token_output: number
  }
}

// 时段 → 展示文案 key（面板副标题用，标明统计口径）
// 热力图自带时段（默认 7 天），与页头时段相互独立：周 × 小时需要自己的观察窗口
type HeatmapRange = '7d' | '30d' | '12m' | 'all'

const RANGE_LABEL_KEYS: Record<RangeKey, TranslationKey> = {
  today: 'rangeToday',
  '24h': 'range24h',
  '7d': 'range7d',
  '30d': 'range30d',
  '12m': 'range12m',
  all: 'rangeAll',
}

// 请求明细行（后端 /stats/requests）
interface RequestLogItem {
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
  // 成本估算（V15）：未定价 / 只填一侧 / 上游没回 usage / V15 之前的历史行 → null
  cost: number | null
}

interface RequestLogPage {
  total: number
  page: number
  page_size: number
  items: RequestLogItem[]
}

interface ErrorBreakdown {
  kind: string
  count: number
}

interface ErrorStats {
  total: number
  success: number
  errors: ErrorBreakdown[]
}

interface HeatmapCell {
  weekday: number
  hour: number
  count: number
}

interface ActivityHeatmapData {
  cells: HeatmapCell[]
  max: number
}

interface LatencyPercentiles {
  p50: number
  p90: number
  p99: number
  max: number
}

type Metric = 'requests' | 'failed_requests' | 'token_input' | 'token_output' | 'avg_latency_ms' | 'avg_ttft_ms' | 'output_speed_tps' | 'cache_read_tokens'
type Granularity = 'hour' | 'day' | 'month' | 'year'
type TrendMode = 'all' | 'single'
type RangeKey = 'today' | '24h' | '7d' | '30d' | '12m' | 'all'

// 指标 → 面板文案 key（趋势图切换 + 排行榜副标题共用）
const METRIC_LABEL_KEYS: Record<Metric, TranslationKey> = {
  requests: 'metricRequests',
  failed_requests: 'metricFailed',
  token_input: 'inputTokens',
  token_output: 'outputTokens',
  avg_latency_ms: 'metricLatency',
  avg_ttft_ms: 'metricTtft',
  output_speed_tps: 'metricSpeed',
  cache_read_tokens: 'metricCacheRead',
}

// 可求和指标（计数/token）：排行按求和；平均类指标（延时/首字/速度）按请求数加权
const SUM_METRICS: ReadonlySet<Metric> = new Set(['requests', 'failed_requests', 'token_input', 'token_output', 'cache_read_tokens'])
const isSumMetric = (m: Metric) => SUM_METRICS.has(m)

// 时段 → 允许粒度 + 默认粒度（选时段时自动切粒度，粒度控件按范围禁用）
const RANGE_GRANULARITIES: Record<RangeKey, { allowed: Granularity[]; fallback: Granularity }> = {
  today: { allowed: ['hour'], fallback: 'hour' },
  '24h': { allowed: ['hour'], fallback: 'hour' },
  '7d': { allowed: ['hour', 'day'], fallback: 'day' },
  '30d': { allowed: ['day'], fallback: 'day' },
  '12m': { allowed: ['month', 'year'], fallback: 'month' },
  all: { allowed: ['day', 'month', 'year'], fallback: 'day' },
}

// loadData 一次负责这几块；某块本轮取到了就从失败清单里摘掉
const LOAD_BLOCKS: TranslationKey[] = ['wallBlockKpi', 'healthBoard', 'platformShare', 'errorAnalysis', 'modelMatrix']
// 趋势与明细各自单独重载，失败清单按这个全集汇总展示
const ALL_BLOCKS: TranslationKey[] = [...LOAD_BLOCKS, 'heatmapTitle', 'modelStats', 'requestLogs']

// 时段 → UTC RFC3339 范围；all 不传（走后端 limit 截取）
function rangeToFromTo(range: RangeKey): { from?: string; to?: string } {
  const now = new Date()
  const iso = (d: Date) => d.toISOString()
  switch (range) {
    case 'today': {
      // 本地今日 00:00 起（配合 hour 粒度的本地时区口径）
      const start = new Date(now)
      start.setHours(0, 0, 0, 0)
      return { from: iso(start), to: iso(now) }
    }
    case '24h': return { from: iso(new Date(now.getTime() - 24 * 3600_000)), to: iso(now) }
    case '7d': return { from: iso(new Date(now.getTime() - 7 * 86400_000)), to: iso(now) }
    case '30d': return { from: iso(new Date(now.getTime() - 30 * 86400_000)), to: iso(now) }
    case '12m': return { from: iso(new Date(now.getTime() - 365 * 86400_000)), to: iso(now) }
    default: return {}
  }
}

// 固定 8 色模型调色板：趋势图 / 排行榜 / 环形占比共用，「一个模型 = 一个颜色」贯穿全页
// 暗色组接品牌青 + 数据金，其余按色相大间隔轮转，避免冷色扎堆难以区分；
// 浅色组是同一批色相的深色版本，对白底与 #F4F4F1 都 ≥5.2:1。
const MODEL_PALETTE = {
  dark: ['#45C8C3', '#E0AF63', '#B7A6FF', '#F07C86', '#8FBF7F', '#6FA8DC', '#E39AC8', '#C6D96B'],
  light: ['#0F6E6A', '#8A5C12', '#5B3F9E', '#A62B3A', '#216636', '#1D5A9E', '#8E3B6B', '#5E6B1A'],
}

// 强调色 / 状态语义色：按明暗方案各取一组，暗色高亮通透、亮色保证可读对比。
// 数值必须与 main.css 的 --accent / --ok / --warn / --err 同步——大屏这里走的是 Recharts
// 与内联 style，拿不到 CSS 变量，两处不同步就会出现"同一屏两种红"。
const ACCENT = { dark: '#45C8C3', light: '#0F6E6A' }
const STATUS = {
  dark: { ok: '#6FD39A', warn: '#E8B75C', err: '#F07C86' },
  light: { ok: '#216636', warn: '#8A5A00', err: '#A61B2E' },
}

function formatNumber(n: number | undefined): string {
  return (n ?? 0).toLocaleString()
}

function formatTokenCount(n: number | undefined): string {
  const v = n ?? 0
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`
  if (v >= 1_000) return `${(v / 1_000).toFixed(1)}K`
  return v.toString()
}

// 时间 ms → 自适应单位：<1s 纯数字(ms)；1s~60s 一位小数+s；≥1m 分钟
function formatDurationMs(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`
  return `${(ms / 60_000).toFixed(1)}m`
}

// 指标值 → 展示文案（计数 / token / 延时(ms) / 首字(ms) / 速度(tok/s)）
function formatMetricValue(m: Metric, v: number): string {
  switch (m) {
    case 'requests':
    case 'failed_requests':
      return Math.round(v).toLocaleString()
    case 'token_input':
    case 'token_output':
    case 'cache_read_tokens':
      return formatTokenCount(v)
    case 'avg_latency_ms':
    case 'avg_ttft_ms':
      return formatDurationMs(v)
    case 'output_speed_tps':
      return v.toFixed(1)
  }
}

// 请求明细时间：后端 created_at 为 UTC RFC3339，转本地时分秒（跨天时补 MM-DD）
function formatLogTime(iso: string, locale: Locale): string {
  const d = new Date(iso)
  if (isNaN(d.getTime())) return iso
  const hm = d.toLocaleTimeString(locale === 'zh' ? 'zh-CN' : 'en-US', { hour12: false })
  const isToday = d.toDateString() === new Date().toDateString()
  return isToday ? hm : `${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${hm}`
}

const statusColor = (rate: number, dark: boolean) => {
  const s = dark ? STATUS.dark : STATUS.light
  if (rate > 90) return s.ok
  if (rate > 70) return s.warn
  return s.err
}

// 错误类型 → 语义色 / 文案（仅用琥珀/红/灰蓝，不引入新颜色）
const errorColors = (dark: boolean) => {
  const s = dark ? STATUS.dark : STATUS.light
  return {
    RateLimit: s.warn,
    Timeout: s.warn,
    ServerError: s.err,
    ConnectionError: s.err,
    StreamError: s.err,
    UpstreamError: s.err,
    Unknown: 'var(--text-3)',
  } as Record<string, string>
}
const ERROR_LABELS: Record<string, TranslationKey> = {
  RateLimit: 'errRateLimit',
  Timeout: 'errTimeout',
  ServerError: 'errServerError',
  ConnectionError: 'errConnection',
  StreamError: 'errStream',
  UpstreamError: 'errUpstream',
  Unknown: 'errUnknown',
}

// 图表配色：暗/亮自适应；网格和坐标轴用中性细线，不再染蓝
function chartTokens(isDark: boolean) {
  return {
    accent: isDark ? ACCENT.dark : ACCENT.light,
    axis: isDark ? '#858D88' : '#5C635D',
    grid: isDark ? 'rgba(230,245,242,0.08)' : 'rgba(22,30,25,0.09)',
    tooltipBg: isDark ? 'rgba(17,20,20,0.94)' : 'rgba(255,255,255,0.92)',
    tooltipBorder: isDark ? 'rgba(230,245,242,0.14)' : 'rgba(22,30,25,0.14)',
    text: isDark ? '#F1F3F1' : '#141714',
  }
}

// KPI 数字刷新过渡：300ms ease-out 计数动画
function useCountUp(target: number, duration = 300): number {
  const [display, setDisplay] = useState(target)
  const prevRef = useRef(target)
  useEffect(() => {
    const from = prevRef.current
    prevRef.current = target
    if (from === target) return
    const start = performance.now()
    let raf = 0
    const tick = (now: number) => {
      const p = Math.min(1, (now - start) / duration)
      const eased = 1 - Math.pow(1 - p, 3)
      setDisplay(from + (target - from) * eased)
      if (p < 1) raf = requestAnimationFrame(tick)
    }
    raf = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(raf)
  }, [target, duration])
  return display
}

// 迷你走势线：纯 SVG，无发光、细线 + 淡面积（KPI 卡右侧微件）

// 成功率环形仪表：stroke-dasharray 填充到目标，600ms（尺寸自适应，小号用于 KPI 卡右侧）

// 活跃热力图（周 × 时）：accent 单色明度阶梯表达强度
function Heatmap({ data, locale, isDark }: { data: ActivityHeatmapData | null; locale: Locale; isDark: boolean }) {
  if (!data || data.max === 0) {
    return <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t(locale, 'noChartData')}</span>
  }
  const map = new Map<string, number>()
  data.cells.forEach((c) => map.set(`${c.weekday}-${c.hour}`, c.count))
  const order = [1, 2, 3, 4, 5, 6, 0] // 周一 → 周日（%w 0=周日）
  const wd = locale === 'zh' ? ['一', '二', '三', '四', '五', '六', '日'] : ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su']
  // 浅色底上 15% 的 accent 与面板底色几乎同亮度，最低档要抬到 35% 才分得出格子
  const steps = isDark ? [0.15, 0.35, 0.6, 0.85, 1] : [0.35, 0.55, 0.7, 0.85, 1]
  const intensity = (c: number) => {
    if (!c) return 'var(--panel-bg-2)'
    const idx = Math.min(steps.length - 1, Math.floor((c / data.max) * steps.length))
    return `color-mix(in srgb, var(--accent) ${steps[idx] * 100}%, var(--panel-bg-2))`
  }
  return (
    <div>
      <div className="wall-heatmap">
        {order.map((w, ri) => (
          <Fragment key={w}>
            <span className="wall-heat-label">{wd[ri]}</span>
            {Array.from({ length: 24 }, (_, h) => {
              const c = map.get(`${w}-${h}`) || 0
              return (
                <span
                  key={h}
                  className="wall-heat-cell"
                  style={{ background: intensity(c) }}
                  title={`${wd[ri]} ${String(h).padStart(2, '0')}:00 · ${c}`}
                />
              )
            })}
          </Fragment>
        ))}
      </div>
      <div className="wall-heat-hours">
        <span />
        {[0, 6, 12, 18].map((h) => (
          <span key={h} className="wall-heat-hour" style={{ gridColumn: h + 2 }}>{h}h</span>
        ))}
      </div>
    </div>
  )
}

// delta 徽章：与上一周期比；prev 缺失时不渲染（紧凑卡内随数字同行）
function DeltaBadge({ cur, prev, higherIsBetter }: {
  cur: number
  prev: number | undefined
  higherIsBetter: boolean
}) {
  if (prev === undefined || (prev === 0 && cur === 0)) return null
  const pct = prev === 0 ? 100 : ((cur - prev) / prev) * 100
  if (!isFinite(pct)) return null
  const up = pct >= 0
  const good = up === higherIsBetter
  return (
    <span className={`wall-delta ${good ? 'good' : 'bad'}`}>
      {up ? '↑' : '↓'}{Math.abs(pct).toFixed(1)}%
    </span>
  )
}

// 页头副标题（时段 + 秒级时钟）。单独成组件是为了把每秒一次的 setState 关在叶子里：
// 挂在墙上常驻时，页面级 now 会让整棵树（两张 recharts 图 + 168 格热力图）每秒重建一次。
function PageSub({ locale, label }: { locale: Locale; label: string }) {
  const [now, setNow] = useState(() => new Date())
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000)
    return () => clearInterval(id)
  }, [])
  return (
    <div className="console-page-sub">
      {label} · {now.toLocaleTimeString(locale === 'zh' ? 'zh-CN' : 'en-US', { hour12: false })}
    </div>
  )
}

// KPI 卡：左上 3px 强调竖条（CSS ::before）+ 右上水印图标；widget 为右侧微件（走势线/环形），与文字列垂直居中

// 面板取数失败时不能退回"暂无数据"——那是把"没取到"说成"确实没有"
function PanelFailed({ locale }: { locale: Locale }) {
  return <span style={{ fontSize: 12, color: 'var(--warn)' }}>{t(locale, 'wallPanelFailed')}</span>
}

export default function Dashboard() {
  const { locale, isDark } = useAppContext()
  const navigate = useNavigate()
  const palette = isDark ? MODEL_PALETTE.dark : MODEL_PALETTE.light

  const [stats, setStats] = useState<OverviewStats>({
    total_requests: 0, success_rate: 0, avg_latency_ms: 0,
    active_proxies: 0, total_proxies: 0,
    active_platforms: 0, total_platforms: 0, total_models: 0,
    total_token_input: 0, total_token_output: 0,
  })
  const [proxies, setProxies] = useState<ProxyWithStats[]>([])
  const [platforms, setPlatforms] = useState<PlatformWithStats[]>([])
  const [errorStats, setErrorStats] = useState<ErrorStats | null>(null)
  const [heatmap, setHeatmap] = useState<ActivityHeatmapData | null>(null)
  const [percentiles, setPercentiles] = useState<LatencyPercentiles | null>(null)
  const [loading, setLoading] = useState(false)
  // 三组请求并发写同一份"取不到的模块"，用 ref 收口再触发一次重渲染，免得互相覆盖
  const failedRef = useRef<Set<TranslationKey>>(new Set())
  const [, setFailedTick] = useState(0)

  const [granularity, setGranularity] = useState<Granularity>('hour')
  const [rangeKey, setRangeKey] = useState<RangeKey>('today')
  const [heatmapRange, setHeatmapRange] = useState<HeatmapRange>('7d')
  const [metric, setMetric] = useState<Metric>('requests')
  const [stacked, setStacked] = useState(false)
  const [trendMode, setTrendMode] = useState<TrendMode>('all')
  const [selectedModel, setSelectedModel] = useState('')
  const [tsData, setTsData] = useState<ModelTimeSeriesStats | null>(null)
  const [tsLoading, setTsLoading] = useState(false)
  const [refreshNonce, setRefreshNonce] = useState(0)
  const trendPanelRef = useRef<HTMLDivElement>(null)

  // 模型性能矩阵（全量汇总，随时段联动）+ 明细筛选用的模型选项
  const [summaries, setSummaries] = useState<ModelSummaryRow[]>([])
  const [modelOptions, setModelOptions] = useState<string[]>([])

  // 请求明细（分页 + 筛选 + 静默刷新）
  const [logs, setLogs] = useState<RequestLogPage | null>(null)
  const [logsLoading, setLogsLoading] = useState(false)
  const [logsPage, setLogsPage] = useState(1)
  const [logsModel, setLogsModel] = useState<string>('')
  const [logsStatus, setLogsStatus] = useState<string>('')

  const [autoRefresh, setAutoRefresh] = useState(true)
  const [paused, setPaused] = useState(false)

  const [fsOn, setFsOn] = useState(false)
  const [fsManual, setFsManual] = useState(false)

  // ---- 数据加载（所有统计共用页头的时段筛选） ----
  // 取不到就记下是哪一块：大屏是远看的，"读数停在 0"和"这块没取到"必须分得开
  const applyFailed = useCallback((owned: TranslationKey[], missing: TranslationKey[]) => {
    owned.forEach((block) => failedRef.current.delete(block))
    missing.forEach((block) => failedRef.current.add(block))
    setFailedTick((n) => n + 1)
  }, [])

  // silent=true：自动刷新时静默拉取，不闪 loading 遮罩，仅原地换数据
  const loadData = useCallback(async (r: RangeKey, silent = false) => {
    const range = rangeToFromTo(r)
    if (!silent) setLoading(true)
    const missing: TranslationKey[] = []
    const note = (block: TranslationKey) => {
      if (!missing.includes(block)) missing.push(block)
    }
    try {
      const [overview, proxyList, platformList, errs, pct, summaries] = await Promise.all([
        getOverview(range).catch(() => { note('wallBlockKpi'); return {} }),
        listProxies().catch(() => { note('healthBoard'); return [] }),
        listPlatforms().catch(() => { note('platformShare'); return [] }),
        getErrorStats(range).catch(() => { note('errorAnalysis'); return null }),
        getLatencyPercentiles(range).catch(() => { note('wallBlockKpi'); return null }),
        getModelSummaries(range).catch(() => { note('modelMatrix'); return [] }),
      ])
      setStats(overview as OverviewStats)
      setErrorStats(errs as ErrorStats | null)
      setPercentiles(pct as LatencyPercentiles | null)
      const sum = (summaries as ModelSummaryRow[]) || []
      setSummaries(sum)
      // 明细筛选下拉：从本次时段实际出现过的模型集合生成
      setModelOptions(Array.from(new Set(sum.map((s) => s.model_id))).sort())

      const proxiesWithStats: ProxyWithStats[] = await Promise.all(
        proxyList.map(async (p: any) => {
          const pStats = await getProxyStats(p.id, range).catch(() => null)
          return { ...p, stats: pStats || undefined }
        })
      )
      setProxies(proxiesWithStats)

      const platformsWithStats: PlatformWithStats[] = await Promise.all(
        platformList.map(async (p: any) => {
          const pStats = await getPlatformStats(p.id, range).catch(() => null)
          return { ...p, stats: pStats || undefined }
        })
      )
      setPlatforms(platformsWithStats)
    } catch {
      // 每个接口自己已经记下失败；走到这里只能是意外，不再吞掉状态
    } finally {
      applyFailed(LOAD_BLOCKS, missing)
      if (!silent) setLoading(false)
    }
  }, [applyFailed])

  const loadTs = useCallback(async (g: Granularity, r: RangeKey, silent = false) => {
    if (!silent) setTsLoading(true)
    try {
      setTsData(await getModelTimeSeries(g, rangeToFromTo(r)))
      applyFailed(['modelStats'], [])
    } catch {
      setTsData(null)
      applyFailed(['modelStats'], ['modelStats'])
    } finally {
      if (!silent) setTsLoading(false)
    }
  }, [applyFailed])

  // 请求明细：跟随页头时段 + 模型/状态筛选 + 分页
  const loadLogs = useCallback(async (r: RangeKey, page: number, model: string, status: string, silent = false) => {
    if (!silent) setLogsLoading(true)
    try {
      const range = rangeToFromTo(r)
      const data = await getRequestLogs({
        ...range,
        page,
        page_size: 50,
        model: model || undefined,
        status: (status as 'success' | 'fail') || undefined,
      })
      setLogs(data as RequestLogPage)
      applyFailed(['requestLogs'], [])
    } catch {
      setLogs(null)
      applyFailed(['requestLogs'], ['requestLogs'])
    } finally {
      if (!silent) setLogsLoading(false)
    }
  }, [applyFailed])


  // 热力图看的是"星期 × 小时"的长期作息：跟着"今日/24h"走只剩一两行，没有意义。
  // 固定取最近 30 天，与页头时段解耦，面板上写明窗口。
  const loadHeatmap = useCallback(async (r: HeatmapRange) => {
    const missing: TranslationKey[] = []
    try {
      setHeatmap(await getHeatmap(rangeToFromTo(r)) as ActivityHeatmapData)
    } catch {
      setHeatmap(null)
      missing.push('heatmapTitle')
    } finally {
      applyFailed(['heatmapTitle'], missing)
    }
  }, [applyFailed])

  useEffect(() => { void loadHeatmap(heatmapRange) }, [loadHeatmap, heatmapRange, refreshNonce])
  useEffect(() => { loadData(rangeKey) }, [loadData, rangeKey, refreshNonce])
  useEffect(() => { loadTs(granularity, rangeKey) }, [granularity, rangeKey, loadTs, refreshNonce])
  // 时段/筛选/分页/手动刷新变化时重载明细（非静默，显示 loading）
  useEffect(() => { loadLogs(rangeKey, logsPage, logsModel, logsStatus) }, [rangeKey, logsPage, logsModel, logsStatus, loadLogs, refreshNonce])

  // 矩阵失败数下钻：一次只展开一个模型；切时段/重试时收起，避免旧明细挂在新数据下面
  const [errorModel, setErrorModel] = useState<string | null>(null)
  const [errorModelStats, setErrorModelStats] = useState<ErrorStats | null>(null)
  const [errorModelLoading, setErrorModelLoading] = useState(false)
  const errorRequestRef = useRef<string | null>(null)

  const toggleModelErrors = useCallback(async (modelId: string) => {
    if (errorModel === modelId) { setErrorModel(null); return }
    setErrorModel(modelId)
    setErrorModelStats(null)
    setErrorModelLoading(true)
    errorRequestRef.current = modelId
    try {
      const data = await getErrorStats({ ...rangeToFromTo(rangeKey), model: modelId }) as ErrorStats
      if (errorRequestRef.current === modelId) setErrorModelStats(data)
    } catch {
      if (errorRequestRef.current === modelId) setErrorModelStats(null)
    } finally {
      if (errorRequestRef.current === modelId) setErrorModelLoading(false)
    }
  }, [errorModel, rangeKey])

  useEffect(() => {
    errorRequestRef.current = null
    setErrorModel(null)
    setErrorModelStats(null)
    setErrorModelLoading(false)
  }, [rangeKey, refreshNonce])

  // 选择时段时自动切到该时段的默认粒度（当前粒度若不在允许集合内）；明细回到第一页
  const handleRangeChange = useCallback((v: string) => {
    const r = v as RangeKey
    setRangeKey(r)
    setGranularity((g) => (RANGE_GRANULARITIES[r].allowed.includes(g) ? g : RANGE_GRANULARITIES[r].fallback))
    setLogsPage(1)
  }, [])

  // 自动刷新：默认开启，30s；静默异步拉取数据（不闪遮罩、不重绘整页）
  useEffect(() => {
    if (!autoRefresh || paused) return
    const id = setInterval(() => {
      loadData(rangeKey, true)
      loadTs(granularity, rangeKey, true)
      loadLogs(rangeKey, logsPage, logsModel, logsStatus, true)
    }, 30_000)
    return () => clearInterval(id)
  }, [autoRefresh, paused, rangeKey, granularity, logsPage, logsModel, logsStatus, loadData, loadTs, loadLogs])

  // 全屏模式：documentElement 加 .wall-fullscreen；Esc / 再点按钮退出。
  // requestFullscreen 不可用（如部分 webview）时退化为仅切换 class 的伪全屏。
  useEffect(() => {
    const sync = () => setFsOn(!!document.fullscreenElement)
    document.addEventListener('fullscreenchange', sync)
    return () => {
      document.removeEventListener('fullscreenchange', sync)
      if (document.fullscreenElement) document.exitFullscreen().catch(() => {})
      document.documentElement.classList.remove('wall-fullscreen')
    }
  }, [])

  useEffect(() => {
    document.documentElement.classList.toggle('wall-fullscreen', fsOn || fsManual)
  }, [fsOn, fsManual])

  const toggleFullscreen = useCallback(() => {
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => {})
      return
    }
    if (fsManual) {
      setFsManual(false)
      return
    }
    document.documentElement.requestFullscreen?.().catch(() => setFsManual(true))
  }, [fsManual])

  // ---- 派生数据：透视趋势 + 排行 + sparkline（共用同一份时序数据与调色板） ----
  const { chartData, series } = useMemo(() => {
    if (!tsData) return { chartData: [], series: [] as { name: string; label: string; color: string }[] }
    const modelSet = new Set<string>()
    tsData.data.forEach((p) => modelSet.add(p.model_id))
    const models = Array.from(modelSet)

    const lookup: Record<string, Record<string, ModelTimeSeriesPoint>> = {}
    tsData.data.forEach((p) => {
      if (!lookup[p.period]) lookup[p.period] = {}
      lookup[p.period][p.model_id] = p
    })

    const chartData = tsData.periods.map((period) => {
      const row: Record<string, any> = { period }
      models.forEach((m, i) => {
        const point = lookup[period]?.[m]
        row[`m${i}`] = point ? point[metric] : 0
      })
      return row
    })

    const series = models.map((m, i) => ({
      name: `m${i}`,
      label: m,
      color: palette[i % palette.length],
    }))

    return { chartData, series }
  }, [tsData, metric, palette])

  // 模型矩阵排序（唯一"每模型"视图，替代原排行条形图）：计数/速度降序，延时/首字升序——最优在前。
  // max 供排序列内嵌微条做宽度分母。
  const sortedSummaries = useMemo(() => {
    const asc = metric === 'avg_latency_ms' || metric === 'avg_ttft_ms'
    const rows = [...summaries].sort((a, b) =>
      asc ? (a[metric] || 0) - (b[metric] || 0) : (b[metric] || 0) - (a[metric] || 0)
    )
    return { rows, max: Math.max(...rows.map((r) => r[metric] || 0), 1) }
  }, [summaries, metric])

  // 矩阵色点沿用趋势图"一个模型一个颜色"的映射；无趋势数据的模型按序补色
  const trendColorOf = useMemo(() => new Map(series.map((s) => [s.label, s.color])), [series])
  const selectedModelColor = trendColorOf.get(selectedModel) || palette[0]

  // 单模型视图的模型集合：与当前时段的趋势数据同源，按请求数降序、同数按 id 升序，
  // 保证默认选中与截图/测试都稳定。
  const modelTotals = useMemo(() => {
    const totals = new Map<string, number>()
    tsData?.data.forEach((point) => {
      totals.set(point.model_id, (totals.get(point.model_id) || 0) + point.requests)
    })
    return Array.from(totals, ([model_id, requests]) => ({ model_id, requests }))
      .sort((a, b) => b.requests - a.requests || a.model_id.localeCompare(b.model_id))
  }, [tsData])

  // 切时段后选中的模型可能已不在新窗口：仍保留在选项里，面板显示空态，
  // 不静默跳到另一个模型——否则用户以为看的是 A，实际是 B。
  const timelineOptions = useMemo(() => {
    const ids = modelTotals.map((item) => item.model_id)
    if (selectedModel && !ids.includes(selectedModel)) ids.unshift(selectedModel)
    return ids
  }, [modelTotals, selectedModel])

  const selectedSummary = useMemo(
    () => summaries.find((row) => row.model_id === selectedModel),
    [summaries, selectedModel],
  )

  useEffect(() => {
    if (trendMode !== 'single' || selectedModel || modelTotals.length === 0) return
    setSelectedModel(modelTotals[0].model_id)
  }, [trendMode, selectedModel, modelTotals])

  const handleTrendMode = useCallback((value: string) => {
    const mode = value as TrendMode
    setTrendMode(mode)
    if (mode === 'single' && !selectedModel && modelTotals[0]) {
      setSelectedModel(modelTotals[0].model_id)
    }
  }, [selectedModel, modelTotals])

  // 从模型矩阵下钻：切到单模型并滚到统计面板；面板已经可见时 scrollIntoView 只做最小移动
  const openModelTimeline = useCallback((modelId: string) => {
    setTrendMode('single')
    setSelectedModel(modelId)
    requestAnimationFrame(() => {
      const panel = trendPanelRef.current
      if (!panel) return
      const reduce = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
      panel.scrollIntoView({ behavior: reduce ? 'auto' : 'smooth', block: 'nearest' })
    })
  }, [])

  // KPI 走势微件：请求数与加权平均延迟按周期聚合（随时段/粒度联动）
  const periodTotals = useMemo(() => {
    if (!tsData) return { requests: [] as number[], latency: [] as number[] }
    const req: Record<string, number> = {}
    const latW: Record<string, number> = {}
    const latN: Record<string, number> = {}
    tsData.data.forEach((p) => {
      req[p.period] = (req[p.period] || 0) + p.requests
      latW[p.period] = (latW[p.period] || 0) + (p.avg_latency_ms || 0) * p.requests
      latN[p.period] = (latN[p.period] || 0) + p.requests
    })
    return {
      requests: tsData.periods.map((p) => req[p] || 0),
      latency: tsData.periods.map((p) => (latN[p] ? latW[p] / latN[p] : 0)),
    }
  }, [tsData])

  // 平台分布环形：中心总量 + 8px 色点图例
  const platformShare = useMemo(() => {
    return platforms
      .map((p, i) => ({
        label: p.name,
        value: p.stats?.total_requests || 0,
        color: palette[i % palette.length],
      }))
      .filter((p) => p.value > 0)
      .sort((a, b) => b.value - a.value)
  }, [platforms, palette])

  const platformTotal = platformShare.reduce((s, p) => s + p.value, 0)
  const sharePercent = (v: number) => (platformTotal ? Math.round((v / platformTotal) * 100) : 0)

  const valueFormatter = (value: number) => formatMetricValue(metric, value)

  const getPlatformDisplayName = (name: string) => {
    const preset = platformPresets.find((p) => p.name === name)
    return preset ? getPresetName(preset, locale) : name
  }

  // ---- KPI 派生值 ----
  const totalRequests = useCountUp(stats.total_requests || 0)
  const avgLatency = useCountUp(Math.round(stats.avg_latency_ms || 0))
  const totalTokens = (stats.total_token_input || 0) + (stats.total_token_output || 0)
  const connections = useCountUp(stats.total_connections_active || 0)
  const activeProxies = proxies.filter((p) => (p.stats?.total_requests || 0) > 0).length
  const tokenIn = stats.total_token_input || 0
  const tokenOut = stats.total_token_output || 0
  const rateColor = statusColor(stats.success_rate || 0, isDark)
  const status = isDark ? STATUS.dark : STATUS.light
  const errorColorMap = errorColors(isDark)
  // 首字/速度 KPI：流式请求数占比（全时段汇总口径）
  const streamShare = useMemo(() => {
    const totalReq = summaries.reduce((s, m) => s + m.requests, 0)
    const streamReq = summaries.reduce((s, m) => s + m.stream_requests, 0)
    return totalReq ? (streamReq / totalReq) * 100 : 0
  }, [summaries])

  // 平均类指标（延时/首字/速度）堆叠无意义：堆叠模式仅对可求和指标生效
  const effectiveStacked = stacked && isSumMetric(metric)

  // 图表的文本替代（§8.8）：大屏上的图是给两米外看的，读屏的人要拿到同一份数值而不是"这里有一张图"
  const trendTitle = `${t(locale, 'modelStats')} · ${t(locale, METRIC_LABEL_KEYS[metric])}`
  // 计数类指标一行是"各模型相加"，平均类相加没有意义——那一行的峰值只能取单模型最大值
  const trendRowValue = (row: Record<string, any>) => {
    const values = series.map(s => Number(row[s.name]) || 0)
    return isSumMetric(metric) ? values.reduce((sum, v) => sum + v, 0) : Math.max(0, ...values)
  }
  const trendPeak = peakOf(chartData, trendRowValue)
  const trendAlt = t(locale, effectiveStacked ? 'chartAltBar' : 'chartAltTrend', trendTitle, String(chartData.length),
    trendPeak ? valueFormatter(trendRowValue(trendPeak)) : '—', trendPeak ? String(trendPeak.period) : '—')
  const piePeak = platformShare[0]
  const pieAlt = t(locale, 'chartAltPie', t(locale, 'platformShare'), String(platformShare.length),
    piePeak ? getPlatformDisplayName(piePeak.label) : '—', piePeak ? `${sharePercent(piePeak.value)}%` : '—')

  // 矩阵排序列渲染：当前指标所在单元格内嵌相对宽度微条，直观呈现"排行"（替代原排行面板）
  const mxCell = (m: ModelSummaryRow, key: Metric, content: React.ReactNode, extraStyle?: React.CSSProperties) => {
    if (key !== metric) return <td className="wall-num" style={extraStyle}>{content}</td>
    const w = ((m[key] || 0) / sortedSummaries.max) * 100
    return (
      <td
        className="wall-num wall-mx-sorted"
        style={{ ...extraStyle, '--bar-w': `${w.toFixed(1)}%` } as React.CSSProperties}
      >
        {content}
      </td>
    )
  }
  // 排序列表头高亮
  const thCls = (key: Metric) => `wall-num${key === metric ? ' wall-mx-th-sorted' : ''}`

  const cc = chartTokens(isDark)
  const axisTick = { fontSize: 12, fill: cc.axis, style: { fontVariantNumeric: 'tabular-nums' } }

  // 趋势图共用子元素：分组折线 / 堆叠柱状两种形态复用。
  // 注意：必须是数组而不是 Fragment——recharts 2.x 依赖的 react-is 无法识别 React 19
  // 的 Fragment 元素，包在 Fragment 里的轴与 tooltip 会被 recharts 整体忽略。
  const trendAxes = [
    <CartesianGrid key="grid" strokeDasharray="3 3" stroke={cc.grid} vertical={false} />,
    // X 轴按粒度格式化：小时=HH:00、日=MM-DD、月/年=原值；抽稀：日/月=每 2 个、小时=每 2 小时、年=全显
    <XAxis
      key="x"
      dataKey="period"
      interval={granularity === 'year' ? 0 : 1}
      tick={axisTick}
      tickLine={false}
      axisLine={{ stroke: cc.grid }}
      height={26}
      tickFormatter={(v: string) => {
        if (granularity === 'hour') return v.slice(11, 16)
        if (granularity === 'day') return v.slice(5)
        return v
      }}
    />,
    <YAxis key="y" width={52} tick={axisTick} tickFormatter={valueFormatter} tickLine={false} axisLine={false} allowDecimals={false} />,
    // hover 十字线 + 毛玻璃 tooltip
    <RTooltip
      key="tt"
      cursor={{ stroke: cc.accent, strokeOpacity: 0.6, strokeDasharray: '4 4' }}
      contentStyle={{
        background: cc.tooltipBg,
        border: `1px solid ${cc.tooltipBorder}`,
        borderRadius: 8,
        fontSize: 12,
        color: cc.text,
        backdropFilter: 'blur(8px)',
        boxShadow: 'none',
      }}
      labelStyle={{ color: cc.axis, fontSize: 12, marginBottom: 4, fontVariantNumeric: 'tabular-nums' }}
      labelFormatter={(v: string) => (granularity === 'hour' ? `${v.slice(5, 10)} ${v.slice(11, 16)}` : v)}
      formatter={(value: number, name: string) => [valueFormatter(value), series.find((s) => s.name === name)?.label || name]}
    />,
  ]

  const failedBlocks = ALL_BLOCKS.filter((b) => failedRef.current.has(b))
  const failed = (b: TranslationKey) => failedRef.current.has(b)
  const kpiDown = failed('wallBlockKpi')

  return (
    <div className="wall-root">
      {/* ===== 页头：与控制台其他页面同一套 kicker / 标题 / 副标题 + 右侧动作 ===== */}
      <div className="console-page-head wall-page-head">
        <div>
          <div className="console-kicker">
            {t(locale, 'kickerLive')}
          </div>
          <h1 className="console-page-title">{t(locale, 'wallTitle')}</h1>
          <PageSub locale={locale} label={t(locale, RANGE_LABEL_KEYS[rangeKey])} />
        </div>
        <div className="console-page-actions">
          {/* 全局时段筛选：页面所有统计（KPI/趋势/排行/错误/热力图/平台/健康看板）共用 */}
          <SegmentedControl
            size="sm"
            value={rangeKey}
            onChange={handleRangeChange}
            data={[
              { label: t(locale, 'rangeToday'), value: 'today' },
              { label: t(locale, 'range24h'), value: '24h' },
              { label: t(locale, 'range7d'), value: '7d' },
              { label: t(locale, 'range30d'), value: '30d' },
              { label: t(locale, 'range12m'), value: '12m' },
              { label: t(locale, 'rangeAll'), value: 'all' },
            ]}
          />
          <Group gap={4} wrap="nowrap">
            <Tooltip label={t(locale, 'autoRefresh')}>
              <ActionIcon
                variant="subtle"
                size="lg"
                aria-label={t(locale, 'autoRefresh')}
                onClick={() => setAutoRefresh((v) => !v)}
                style={{ color: autoRefresh ? ACCENT[isDark ? 'dark' : 'light'] : 'var(--text-3)' }}
              >
                <IconRefresh size={18} />
              </ActionIcon>
            </Tooltip>
            <Tooltip label={paused ? t(locale, 'resume') : t(locale, 'pause')}>
              <ActionIcon
                variant="subtle"
                size="lg"
                aria-label={t(locale, paused ? 'resume' : 'pause')}
                onClick={() => setPaused((v) => !v)}
              >
                {paused ? <IconPlayerPlay size={18} /> : <IconPlayerPause size={18} />}
              </ActionIcon>
            </Tooltip>
            <Tooltip label={fsOn || fsManual ? t(locale, 'exitFullscreen') : t(locale, 'fullscreen')}>
              <ActionIcon
                variant="subtle"
                size="lg"
                aria-label={t(locale, fsOn || fsManual ? 'exitFullscreen' : 'fullscreen')}
                onClick={toggleFullscreen}
              >
                {fsOn || fsManual ? <IconArrowsMinimize size={18} /> : <IconArrowsMaximize size={18} />}
              </ActionIcon>
            </Tooltip>
            <Tooltip label={t(locale, 'exitWall')}>
              <ActionIcon
                variant="subtle"
                size="lg"
                className="wall-route-exit"
                aria-label={t(locale, 'exitWall')}
                onClick={() => navigate('/')}
              >
                <IconX size={18} />
              </ActionIcon>
            </Tooltip>
          </Group>
        </div>
      </div>

      {/* ===== 取数失败：把"没取到"和"读数是 0"分开，重试一次拉全部 ===== */}
      {failedBlocks.length > 0 && (
        <div className="wall-failbar" role="status">
          <IconAlertTriangle size={16} stroke={1.6} style={{ color: 'var(--warn)', flexShrink: 0 }} />
          <span className="wall-failbar-list">
            {t(locale, 'wallPartialFail', failedBlocks.map((b) => t(locale, b)).join(' · '))}
          </span>
          <ActionIcon
            variant="subtle"
            size="sm"
            aria-label={t(locale, 'retry')}
            onClick={() => setRefreshNonce((n) => n + 1)}
          >
            <IconRefresh size={14} />
          </ActionIcon>
        </div>
      )}

      {/* ===== KPI 行：与控制台其他页面同一套 readouts（标签 / 大数 / 脚注 + 迷你走势） ===== */}
      <div className="console-readouts wall-readouts">
        <Readout
          label={t(locale, 'totalRequests')}
          value={kpiDown ? '—' : formatNumber(Math.round(totalRequests))}
          meta={kpiDown ? null : <DeltaBadge cur={stats.total_requests || 0} prev={stats.requests_prev_period} higherIsBetter />}
        />

        <Readout
          label={t(locale, 'successRate')}
          value={kpiDown ? '—' : <span style={{ color: rateColor }}>{(stats.success_rate || 0).toFixed(1)}</span>}
          unit={kpiDown ? undefined : '%'}
          meta={
            kpiDown
              ? null
              : failed('errorAnalysis')
                ? `${t(locale, 'successCount')} — · ${t(locale, 'failureCount')} —`
                : `${t(locale, 'successCount')} ${(errorStats?.success ?? 0).toLocaleString()} · ${t(locale, 'failureCount')} ${Math.max(0, (errorStats?.total ?? 0) - (errorStats?.success ?? 0)).toLocaleString()}`
          }
        />

        <Readout
          label={t(locale, 'avgLatency')}
          value={kpiDown ? '—' : formatNumber(Math.round(avgLatency))}
          unit={kpiDown ? undefined : 'ms'}
          // 分位数：均值会被流式长请求拉高，P50/P90/P99 更接近真实体验
          meta={
            kpiDown
              ? null
              : <>
                  <DeltaBadge cur={Math.round(stats.avg_latency_ms || 0)} prev={stats.avg_latency_prev_period} higherIsBetter={false} />
                  {percentiles && percentiles.max > 0 && (
                    <span>
                      P50 {formatNumber(Math.round(percentiles.p50))} · P90 {formatNumber(Math.round(percentiles.p90))} · P99 {formatNumber(Math.round(percentiles.p99))}
                    </span>
                  )}
                </>
          }
        />

        <Readout
          label={t(locale, 'metricTtft')}
          value={kpiDown ? '—' : formatNumber(Math.round(stats.avg_ttft_ms || 0))}
          unit={kpiDown ? undefined : 'ms'}
          meta={kpiDown ? null : `${t(locale, 'metricSpeed')} ${(stats.avg_output_speed_tps || 0).toFixed(1)} ${t(locale, 'speedUnit')} · ${t(locale, 'metricStream')} ${streamShare.toFixed(0)}%`}
        />

        {/* 缓存命中率：CC Switch 口径 cache_read / (cache_read + fresh_input) */}
        <Readout
          label={t(locale, 'kpiCacheHit')}
          value={kpiDown ? '—' : (stats.cache_hit_rate || 0).toFixed(1)}
          unit={kpiDown ? undefined : '%'}
          meta={kpiDown ? null : `${t(locale, 'colFreshInput')} ${formatTokenCount(stats.fresh_input_tokens || 0)} · ${t(locale, 'metricCacheRead')} ${formatTokenCount(stats.total_cache_read_tokens || 0)}`}
        />

        <Readout
          label={t(locale, 'totalTokens')}
          value={kpiDown ? '—' : formatTokenCount(totalTokens)}
          meta={
            kpiDown
              ? null
              : <span title={t(locale, 'costCoverageHint', String(stats.priced_requests ?? 0), String(stats.total_requests || 0))}>
                  {t(locale, 'inputTokens')} {formatTokenCount(tokenIn)} · {t(locale, 'outputTokens')} {formatTokenCount(tokenOut)} · {t(locale, 'cost')} {formatCost(stats.total_cost)}
                </span>
          }
        />

        {/* 网关与并发合并：大数字 = 活跃网关 N/M，脚注 = 实时状态 + 并发数 */}
        <Readout
          label={t(locale, 'activeProxies')}
          value={kpiDown || failed('healthBoard') ? '—' : <>{activeProxies}<span className="console-readout-unit"> / {proxies.length || stats.total_proxies || 0}</span></>}
          meta={
            kpiDown || failed('healthBoard')
              ? null
              : <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                  <span
                    className={`wall-dot${(stats.total_connections_active || 0) > 0 ? ' breathing' : ''}`}
                    style={{ background: (stats.total_connections_active || 0) > 0 ? status.ok : 'var(--text-3)' }}
                  />
                  {(stats.total_connections_active || 0) > 0 ? t(locale, 'inFlight') : t(locale, 'idleState')} · {t(locale, 'concurrent')} {formatNumber(Math.round(connections))}
                </span>
          }
        />
      </div>

      {/* ===== 趋势：整行宽，时间轴更长 ===== */}
      <div
        ref={trendPanelRef}
        className={`wall-panel wall-trend-panel${trendMode === 'single' ? ' wall-trend-single' : ''}`}
        style={{ display: 'flex', flexDirection: 'column', position: 'relative', height: trendMode === 'single' ? 'auto' : 320 }}
      >
          <div className="wall-panel-head">
            <span className="wall-panel-title">{t(locale, 'modelStats')}</span>
            <Group gap="xs" wrap="wrap">
              {(series.length > 0 || trendMode === 'single') && (
                <SegmentedControl
                  size="xs"
                  value={trendMode}
                  onChange={handleTrendMode}
                  aria-label={t(locale, 'timelineModeLabel')}
                  data={[
                    { label: t(locale, 'trendModeAll'), value: 'all' },
                    { label: t(locale, 'trendModeSingle'), value: 'single' },
                  ]}
                />
              )}
              {trendMode === 'single' && (
                <Group gap={6} wrap="nowrap">
                  <span className="wall-dot" style={{ background: selectedModelColor }} />
                  <Select
                    size="xs"
                    w={200}
                    aria-label={t(locale, 'singleModelSelect')}
                    value={selectedModel || null}
                    onChange={(value) => { if (value) setSelectedModel(value) }}
                    data={timelineOptions.map((model) => ({ value: model, label: model }))}
                    allowDeselect={false}
                    searchable
                  />
                </Group>
              )}
              {trendMode === 'all' && (
                <SegmentedControl
                  size="xs"
                  value={metric}
                  onChange={(v) => setMetric(v as Metric)}
                  data={[
                    { label: t(locale, 'mShortRequests'), value: 'requests' },
                    { label: t(locale, 'mShortFailed'), value: 'failed_requests' },
                    { label: t(locale, 'mShortTokenIn'), value: 'token_input' },
                    { label: t(locale, 'mShortTokenOut'), value: 'token_output' },
                    { label: t(locale, 'mShortLatency'), value: 'avg_latency_ms' },
                    { label: t(locale, 'mShortTtft'), value: 'avg_ttft_ms' },
                    { label: t(locale, 'mShortSpeed'), value: 'output_speed_tps' },
                    { label: t(locale, 'mShortCache'), value: 'cache_read_tokens' },
                  ]}
                />
              )}
              <SegmentedControl
                size="xs"
                value={granularity}
                onChange={(v) => setGranularity(v as Granularity)}
                data={[
                  { label: t(locale, 'granularityHour'), value: 'hour', disabled: !RANGE_GRANULARITIES[rangeKey].allowed.includes('hour') },
                  { label: t(locale, 'granularityDay'), value: 'day', disabled: !RANGE_GRANULARITIES[rangeKey].allowed.includes('day') },
                  { label: t(locale, 'granularityMonth'), value: 'month', disabled: !RANGE_GRANULARITIES[rangeKey].allowed.includes('month') },
                  { label: t(locale, 'granularityYear'), value: 'year', disabled: !RANGE_GRANULARITIES[rangeKey].allowed.includes('year') },
                ]}
              />
              {trendMode === 'all' && (
                <Tooltip label={isSumMetric(metric) ? (stacked ? t(locale, 'groupedMode') : t(locale, 'stackedMode')) : ''}>
                  <ActionIcon
                    variant="subtle"
                    size="sm"
                    aria-label={t(locale, effectiveStacked ? 'groupedMode' : 'stackedMode')}
                    disabled={!isSumMetric(metric)}
                    onClick={() => setStacked((s) => !s)}
                  >
                    {effectiveStacked ? <IconStack2 size={14} /> : <IconLayoutGrid size={14} />}
                  </ActionIcon>
                </Tooltip>
              )}
              <Tooltip label={t(locale, 'refresh')}>
                <ActionIcon
                  variant="subtle"
                  size="sm"
                  aria-label={t(locale, 'refresh')}
                  loading={tsLoading}
                  onClick={() => setRefreshNonce((n) => n + 1)}
                >
                  <IconRefresh size={14} />
                </ActionIcon>
              </Tooltip>
            </Group>
          </div>
          {failed('modelStats') ? (
            <PanelFailed locale={locale} />
          ) : trendMode === 'single' ? (
            !tsData ? (
              <div style={{ flex: 1, minHeight: 0, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                <Stack align="center" gap="xs">
                  <IconChartBarPlaceholder />
                  <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t(locale, 'noChartData')}</span>
                </Stack>
              </div>
            ) : (
              <ModelTimeline
                stats={tsData}
                modelId={selectedModel}
                granularity={granularity}
                locale={locale}
                summary={selectedSummary}
                onBackToAll={() => setTrendMode('all')}
                colors={{
                  accent: ACCENT[isDark ? 'dark' : 'light'],
                  err: status.err,
                  out: isDark ? '#E0AF63' : '#8A5C12',
                  axis: cc.axis,
                  grid: cc.grid,
                }}
              />
            )
          ) : series.length ? (
            <div style={{ flex: 1, minHeight: 0 }} role="group" aria-label={trendAlt}>
              <ResponsiveContainer width="100%" height="100%">
                {effectiveStacked ? (
                  /* 堆叠柱状：展示各时段内模型用量的构成与总量（仅计数类指标可用） */
                  <ReBarChart data={chartData} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
                    {trendAxes}
                    {series.map((s) => (
                      <Bar
                        key={s.name}
                        dataKey={s.name}
                        name={s.label}
                        stackId="1"
                        fill={s.color}
                        maxBarSize={28}
                        animationDuration={500}
                      />
                    ))}
                  </ReBarChart>
                ) : (
                  /* 分组折线（默认）：各模型独立一条线，便于横向对比 */
                  <ReLineChart data={chartData} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
                    {trendAxes}
                    {series.map((s) => (
                      <Line
                        key={s.name}
                        type="monotone"
                        dataKey={s.name}
                        name={s.label}
                        stroke={s.color}
                        strokeWidth={1.5}
                        dot={false}
                        activeDot={{ r: 3.5, fill: s.color, stroke: isDark ? '#0B0D0D' : '#FFFFFF', strokeWidth: 2 }}
                        animationDuration={500}
                      />
                    ))}
                  </ReLineChart>
                )}
              </ResponsiveContainer>
              <ChartAltTable
                caption={t(locale, 'chartAltCaption', trendTitle)}
                head={[t(locale, 'chartAltPeriod'), ...series.map(s => s.label)]}
                rows={chartData.map(row => [String(row.period), ...series.map(s => valueFormatter(Number(row[s.name]) || 0))])}
              />
            </div>
          ) : (
            <div style={{ flex: 1, minHeight: 0, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
              <Stack align="center" gap="xs">
                <IconChartBarPlaceholder />
                <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t(locale, 'noChartData')}</span>
              </Stack>
            </div>
          )}
          <LoadingOverlay visible={tsLoading} />
        </div>

      {/* ===== 热力图 + 平台分布：同一行，7 / 5 ===== */}
      <div className="wall-pair">
        {/* 活动热力图 */}
        <div className="wall-panel">
          <div className="wall-panel-head">
            <span className="wall-panel-title">{t(locale, 'heatmapTitle')}</span>
            <Tooltip label={t(locale, 'heatmapIndependentHint')} withArrow>
              <SegmentedControl
                size="xs"
                value={heatmapRange}
                onChange={(v) => setHeatmapRange(v as HeatmapRange)}
                aria-label={t(locale, 'heatmapRange')}
                data={[
                  { label: t(locale, 'range7d'), value: '7d' },
                  { label: t(locale, 'range30d'), value: '30d' },
                  { label: t(locale, 'range12m'), value: '12m' },
                  { label: t(locale, 'rangeAll'), value: 'all' },
                ]}
              />
            </Tooltip>
          </div>
          {failed('heatmapTitle') ? <PanelFailed locale={locale} /> : <Heatmap data={heatmap} locale={locale} isDark={isDark} />}
        </div>

        {/* 平台分布：横排——左环形右双列图例 */}
        <div className="wall-panel" style={{ display: 'flex', flexDirection: 'column', minWidth: 0 }}>
          <div className="wall-panel-head">
            <span className="wall-panel-title">{t(locale, 'platformShare')}</span>
            <span className="wall-panel-sub">{failed('platformShare') ? '—' : `${platforms.length} ${t(locale, 'total')}`}</span>
          </div>
          <div style={{ flex: 1, display: 'flex', gap: 12, alignItems: 'center', minHeight: 0 }}>
            {/* 环形本身只给一句摘要：旁边那两列"名称 + 数值 + 占比"就是它的表格替代，不必再补一份 */}
            <div style={{ width: 120, height: 120, flexShrink: 0 }} role="img" aria-label={pieAlt}>
              {failed('platformShare') ? (
                <PanelFailed locale={locale} />
              ) : platformShare.length ? (
                <ResponsiveContainer width="100%" height="100%">
                  <PieChart>
                    <Pie
                      data={platformShare}
                      dataKey="value"
                      nameKey="label"
                      cx="50%"
                      cy="50%"
                      innerRadius="60%"
                      outerRadius="88%"
                      paddingAngle={2}
                      stroke="none"
                      animationDuration={500}
                    >
                      {platformShare.map((p) => (
                        /* 每一块扇形自己报数：recharts 给 path 上了 role="img"，不给名字就是一堆无名图片 */
                        <Cell
                          key={p.label}
                          fill={p.color}
                          aria-label={`${getPlatformDisplayName(p.label)} ${p.value.toLocaleString()} (${sharePercent(p.value)}%)`}
                        />
                      ))}
                      <Label
                        position="center"
                        content={() => (
                          <text
                            x="50%"
                            y="50%"
                            dominantBaseline="central"
                            textAnchor="middle"
                            fill="var(--text-1)"
                            fontSize={15}
                            fontWeight={700}
                            style={{ fontVariantNumeric: 'tabular-nums' }}
                          >
                            {formatTokenCount(platformTotal)}
                          </text>
                        )}
                      />
                    </Pie>
                    <RTooltip
                      contentStyle={{
                        background: cc.tooltipBg,
                        border: `1px solid ${cc.tooltipBorder}`,
                        borderRadius: 8,
                        fontSize: 12,
                        color: cc.text,
                        backdropFilter: 'blur(8px)',
                        boxShadow: 'none',
                      }}
                      formatter={(value: number, name: string) => [`${value.toLocaleString()} (${sharePercent(value)}%)`, getPlatformDisplayName(String(name))]}
                    />
                  </PieChart>
                </ResponsiveContainer>
              ) : (
                <div style={{ height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                  <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t(locale, 'noChartData')}</span>
                </div>
              )}
            </div>
            <div style={{ flex: 1, display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '3px 12px', alignContent: 'center', minWidth: 0 }}>
              {platformShare.map((p) => (
                <div key={p.label} style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 6, fontSize: 12, minWidth: 0 }}>
                  <span style={{ display: 'flex', alignItems: 'center', gap: 6, minWidth: 0 }}>
                    <span className="wall-dot" style={{ background: p.color }} />
                    <span style={{ color: 'var(--text-2)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                      {getPlatformDisplayName(p.label)}
                    </span>
                  </span>
                  <span style={{ color: 'var(--text-1)', fontWeight: 600, fontVariantNumeric: 'tabular-nums', whiteSpace: 'nowrap' }}>
                    {p.value.toLocaleString()}
                    <span style={{ color: 'var(--text-3)', fontWeight: 400, marginLeft: 3 }}>{sharePercent(p.value)}%</span>
                  </span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* ===== 模型性能矩阵：整行宽；失败数可点开按错误类型下钻 ===== */}
      <div className="wall-panel" style={{ flexShrink: 0, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
          <div className="wall-panel-head">
            <span className="wall-panel-title">{t(locale, 'modelMatrix')}</span>
            <span className="wall-panel-sub">
              {t(locale, 'sortedBy', t(locale, METRIC_LABEL_KEYS[metric]))} · {failed('modelMatrix') ? '—' : `${summaries.length} ${t(locale, 'activeModels')}`}
            </span>
          </div>
          {failed('modelMatrix') ? (
            <PanelFailed locale={locale} />
          ) : sortedSummaries.rows.length ? (
            <div className="wall-matrix-scroll">
              <Table className="wall-matrix" striped highlightOnHover>
                <thead>
                  <tr>
                    <th>{t(locale, 'colModel')}</th>
                    <th className={thCls('requests')}>{t(locale, 'colRequests')}</th>
                    <th className={thCls('failed_requests')}>{t(locale, 'colFailed')}</th>
                    <th className="wall-num">{t(locale, 'successRate')}</th>
                    <th className={thCls('token_input')}>{t(locale, 'colTokenIn')}</th>
                    <th className="wall-num">{t(locale, 'colFreshInput')}</th>
                    <th className={thCls('token_output')}>{t(locale, 'colTokenOut')}</th>
                    <th className={thCls('avg_latency_ms')}>{t(locale, 'colDuration')}</th>
                    <th className={thCls('avg_ttft_ms')}>{t(locale, 'colTtft')}</th>
                    <th className={thCls('output_speed_tps')}>{t(locale, 'colSpeed')}</th>
                    <th className={thCls('cache_read_tokens')}>{t(locale, 'colCacheHit')}</th>
                    <th className="wall-num">{t(locale, 'colStream')}</th>
                  </tr>
                </thead>
                <tbody>
                  {sortedSummaries.rows.map((m, i) => {
                    const rate = m.requests ? (m.success_requests / m.requests) * 100 : 0
                    const dot = trendColorOf.get(m.model_id) || palette[i % palette.length]
                    return (
                      <Fragment key={m.model_id}>
                      <tr>
                        <td className="wall-mx-model" title={m.model_id}>
                          <button
                            type="button"
                            className="wall-mx-model-btn"
                            aria-label={t(locale, 'viewModelTimeline', m.model_id)}
                            onClick={() => openModelTimeline(m.model_id)}
                          >
                            <span className="wall-dot" style={{ background: dot, marginRight: 6 }} />
                            <span className="wall-mx-model-name">{m.model_id}</span>
                          </button>
                        </td>
                        {mxCell(m, 'requests', formatNumber(m.requests))}
                        {mxCell(
                          m,
                          'failed_requests',
                          m.failed_requests > 0 ? (
                            <button
                              type="button"
                              className="wall-mx-failed"
                              aria-expanded={errorModel === m.model_id}
                              aria-label={t(locale, 'errorDetailsFor', m.model_id, String(m.failed_requests))}
                              title={t(locale, 'errorDetailsHint')}
                              onClick={() => void toggleModelErrors(m.model_id)}
                            >
                              {formatNumber(m.failed_requests)}
                            </button>
                          ) : '0',
                          m.failed_requests > 0 ? { color: status.err } : undefined,
                        )}
                        <td className="wall-num" style={{ color: statusColor(rate, isDark) }}>{rate.toFixed(1)}%</td>
                        {mxCell(m, 'token_input', formatTokenCount(m.token_input))}
                        <td className="wall-num">{formatTokenCount(m.fresh_input_tokens)}</td>
                        {mxCell(m, 'token_output', formatTokenCount(m.token_output))}
                        {mxCell(m, 'avg_latency_ms', formatDurationMs(m.avg_latency_ms))}
                        {mxCell(m, 'avg_ttft_ms', m.avg_ttft_ms > 0 ? formatDurationMs(m.avg_ttft_ms) : '—')}
                        {mxCell(m, 'output_speed_tps', m.output_speed_tps > 0 ? m.output_speed_tps.toFixed(1) : '—')}
                        <td className="wall-num" title={`${t(locale, 'metricCacheRead')} ${formatTokenCount(m.cache_read_tokens)} · ${t(locale, 'colFreshInput')} ${formatTokenCount(m.fresh_input_tokens)}`}>
                          {m.cache_read_tokens > 0 ? `${m.cache_hit_rate.toFixed(1)}%` : '—'}
                        </td>
                        <td className="wall-num">{m.requests ? `${((m.stream_requests / m.requests) * 100).toFixed(0)}%` : '—'}</td>
                      </tr>
                      {errorModel === m.model_id && (
                        <tr className="wall-mx-err-row">
                          <td colSpan={12}>
                            <div className="wall-mx-err">
                              <div className="wall-mx-err-head">
                                <span className="wall-panel-title">{t(locale, 'errorDetails')} · <span className="console-mono">{m.model_id}</span></span>
                                <button type="button" className="wall-mx-err-close" aria-label={t(locale, 'close')} onClick={() => setErrorModel(null)}>
                                  <IconX size={12} />
                                </button>
                              </div>
                              {errorModelLoading ? (
                                <span className="wall-mx-err-empty">{t(locale, 'loading')}</span>
                              ) : !errorModelStats ? (
                                <span className="wall-mx-err-empty">{t(locale, 'loadFailed')}</span>
                              ) : errorModelStats.errors.length === 0 ? (
                                <span className="wall-mx-err-empty">{t(locale, 'noErrors')}</span>
                              ) : (
                                errorModelStats.errors.map((e) => (
                                  <div key={e.kind} className="wall-rank-row" title={`${ERROR_LABELS[e.kind] ? t(locale, ERROR_LABELS[e.kind]) : e.kind}: ${e.count}`}>
                                    <span className="wall-rank-label" style={{ maxWidth: 140 }}>
                                      {ERROR_LABELS[e.kind] ? t(locale, ERROR_LABELS[e.kind]) : e.kind}
                                    </span>
                                    <div className="wall-rank-track">
                                      <div
                                        className="wall-rank-fill"
                                        style={{ width: `${(e.count / errorModelStats.errors[0].count) * 100}%`, background: errorColorMap[e.kind] || 'var(--text-3)' }}
                                      />
                                    </div>
                                    <span className="wall-rank-value">{e.count.toLocaleString()}</span>
                                  </div>
                                ))
                              )}
                            </div>
                          </td>
                        </tr>
                      )}
                      </Fragment>
                    )
                  })}
                </tbody>
              </Table>
            </div>
          ) : (
            <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t(locale, 'noChartData')}</span>
          )}
        </div>

      {/* ===== 请求明细：逐条展示 token/耗时/首字/速度，分页 + 模型/状态筛选 ===== */}
      <div className="wall-panel" style={{ position: 'relative', flexShrink: 0 }}>
        <div className="wall-panel-head">
          <span className="wall-panel-title">{t(locale, 'requestLogs')}</span>
          <Group gap="xs" wrap="wrap">
            {/* Mantine Select 值不允许空串，用哨兵代表"全部模型" */}
            <Select
              size="xs"
              w={170}
              aria-label={t(locale, 'modelFilter')}
              value={logsModel || '__all__'}
              onChange={(v) => setLogsModel(!v || v === '__all__' ? '' : v)}
              data={[{ value: '__all__', label: t(locale, 'allModels') }, ...modelOptions.map((m) => ({ value: m, label: m }))]}
              allowDeselect={false}
            />
            <SegmentedControl
              size="xs"
              value={logsStatus || 'all'}
              onChange={(v) => setLogsStatus(v === 'all' ? '' : v)}
              data={[
                { label: t(locale, 'statusAll'), value: 'all' },
                { label: t(locale, 'statusSuccess'), value: 'success' },
                { label: t(locale, 'statusFail'), value: 'fail' },
              ]}
            />
            <span style={{ fontSize: 12, color: 'var(--text-3)', fontVariantNumeric: 'tabular-nums' }}>
              {t(locale, 'totalRecords', failed('requestLogs') ? '—' : (logs?.total ?? 0).toLocaleString())}
            </span>
          </Group>
        </div>
        {failed('requestLogs') ? (
          <PanelFailed locale={locale} />
        ) : logs && logs.items.length ? (
          <>
            <div className="wall-logs-scroll">
              <Table className="wall-logs" striped highlightOnHover>
                <thead>
                  <tr>
                    <th>{t(locale, 'colTime')}</th>
                    <th>{t(locale, 'colModel')}</th>
                    <th>{t(locale, 'colGateway')}</th>
                    <th>{t(locale, 'colPlatform')}</th>
                    <th className="wall-num">{t(locale, 'colStatus')}</th>
                    <th className="wall-num">{t(locale, 'colTokenIn')}</th>
                    <th className="wall-num">{t(locale, 'colFreshInput')}</th>
                    <th className="wall-num">{t(locale, 'colTokenOut')}</th>
                    <th className="wall-num">{t(locale, 'colCacheHit')}</th>
                    <th className="wall-num" title={t(locale, 'costHint')}>{t(locale, 'cost')}</th>
                    <th className="wall-num">{t(locale, 'colDuration')}</th>
                    <th className="wall-num">{t(locale, 'colTtft')}</th>
                    <th className="wall-num">{t(locale, 'colSpeed')}</th>
                    <th className="wall-num">{t(locale, 'colStream')}</th>
                    <th>{t(locale, 'colError')}</th>
                  </tr>
                </thead>
                <tbody>
                  {logs.items.map((r) => {
                    const ok = r.status_code >= 200 && r.status_code < 300
                    return (
                      <tr key={r.id}>
                        <td className="wall-num">{formatLogTime(r.created_at, locale)}</td>
                        <td className="wall-mx-model" title={r.model_id}>{r.model_id}</td>
                        <td>{r.proxy_name}</td>
                        <td>{getPlatformDisplayName(r.platform_name)}</td>
                        <td className="wall-num" style={{ color: ok ? status.ok : status.err }}>
                          {r.status_code || t(locale, 'errShort')}
                        </td>
                        <td className="wall-num">{r.token_input != null ? formatTokenCount(r.token_input) : '—'}</td>
                        <td className="wall-num">{r.fresh_input_tokens != null ? formatTokenCount(r.fresh_input_tokens) : '—'}</td>
                        <td className="wall-num">{r.token_output != null ? formatTokenCount(r.token_output) : '—'}</td>
                        <td
                          className="wall-num"
                          title={r.cache_read_tokens || r.cache_write_tokens ? `${t(locale, 'metricCacheRead')} ${formatTokenCount(r.cache_read_tokens || 0)} · ${t(locale, 'colCacheWrite')} ${formatTokenCount(r.cache_write_tokens || 0)}` : undefined}
                        >
                          {r.cache_read_tokens != null && r.cache_read_tokens > 0 ? formatTokenCount(r.cache_read_tokens) : '—'}
                        </td>
                        <td className="wall-num">{r.cost == null ? '—' : formatCost(r.cost)}</td>
                        <td className="wall-num">{formatDurationMs(r.latency_ms)}</td>
                        <td className="wall-num">{r.ttft_ms != null ? formatDurationMs(r.ttft_ms) : '—'}</td>
                        <td className="wall-num">{r.output_speed_tps != null ? r.output_speed_tps.toFixed(1) : '—'}</td>
                        <td className="wall-num">{r.is_stream ? '●' : '—'}</td>
                        <td style={{ color: 'var(--text-3)' }}>{r.error_type || ''}</td>
                      </tr>
                    )
                  })}
                </tbody>
              </Table>
            </div>
            <div style={{ display: 'flex', justifyContent: 'center', marginTop: 8 }}>
              <Pagination
                size="xs"
                total={Math.ceil((logs.total || 0) / logs.page_size)}
                value={logsPage}
                onChange={setLogsPage}
                boundaries={1}
                siblings={1}
                getControlProps={(part) => {
                  const key = part === 'first' ? 'firstPage'
                    : part === 'previous' ? 'prevPage'
                    : part === 'next' ? 'nextPage'
                    : part === 'last' ? 'lastPage' : null
                  return key ? { 'aria-label': t(locale, key) } : {}
                }}
              />
            </div>
          </>
        ) : (
          <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t(locale, 'noChartData')}</span>
        )}
        <LoadingOverlay visible={logsLoading} />
      </div>
    </div>
  )
}

// 空态占位图标（保持克制的灰度，不抢强调色）
function IconChartBarPlaceholder() {
  return (
    <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="var(--text-3)" strokeWidth="1.5" style={{ opacity: 0.5 }}>
      <path d="M3 3v18h18" />
      <path d="M7 14l4-4 3 3 5-6" />
    </svg>
  )
}
