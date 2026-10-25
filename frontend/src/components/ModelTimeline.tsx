import { useEffect, useMemo, useState } from 'react'
import {
  Area,
  CartesianGrid,
  ComposedChart,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  XAxis,
  YAxis,
} from 'recharts'
import { t, type Locale, type TranslationKey } from '../i18n'
import { formatCompact, formatCost, formatCostCoverage, formatDuration, formatNumber } from '../lib/format'
import type { ModelSummaryRow, ModelTimeSeriesPoint, ModelTimeSeriesStats } from '../types/stats'
import { ChartAltTable } from './ui/ChartAlt'

export type TimelineGranularity = 'hour' | 'day' | 'month' | 'year'

export interface TimelineColors {
  accent: string
  err: string
  out: string
  axis: string
  grid: string
}

interface Bucket {
  period: string
  requests: number
  failed: number
  failRate: number | null
  tokenIn: number
  freshIn: number
  cacheRead: number
  cacheWrite: number
  tokenOut: number
  latency: number | null
  ttft: number | null
  speed: number | null
  cost: number | null
  priced: number
  ttftSamples: number
}

interface Props {
  stats: ModelTimeSeriesStats
  modelId: string
  granularity: TimelineGranularity
  locale: Locale
  colors: TimelineColors
  summary?: ModelSummaryRow
  onBackToAll: () => void
}

const GRANULARITY_KEY: Record<TimelineGranularity, TranslationKey> = {
  hour: 'granularityHour',
  day: 'granularityDay',
  month: 'granularityMonth',
  year: 'granularityYear',
}

// 左轴固定宽度 + 右轴固定预留：所有 lane 的绘图区必须左右对齐，
// 否则同一条悬停竖线在不同图上会落到不同像素，读起来像两个时段。
const Y_WIDTH = 56
const RATE_WIDTH = 38

const pad2 = (n: number) => String(n).padStart(2, '0')

// 与后端桶口径一致：小时桶按服务器本地时区，日/月/年桶按 UTC（见 db/stats.rs::fill_periods）
function isCurrentPeriod(period: string, granularity: TimelineGranularity): boolean {
  const now = new Date()
  if (granularity === 'hour') {
    return period === `${now.getFullYear()}-${pad2(now.getMonth() + 1)}-${pad2(now.getDate())} ${pad2(now.getHours())}`
  }
  if (granularity === 'day') {
    return period === `${now.getUTCFullYear()}-${pad2(now.getUTCMonth() + 1)}-${pad2(now.getUTCDate())}`
  }
  if (granularity === 'month') {
    return period === `${now.getUTCFullYear()}-${pad2(now.getUTCMonth() + 1)}`
  }
  return period === String(now.getUTCFullYear())
}

function periodLabel(period: string, granularity: TimelineGranularity): string {
  if (granularity === 'hour' && period.length >= 13) return `${period.slice(5, 10)} ${period.slice(11, 13)}:00`
  return period
}

function xTickLabel(period: string, granularity: TimelineGranularity): string {
  if (granularity === 'hour') return period.length >= 13 ? `${period.slice(11, 13)}:00` : period
  if (granularity === 'day') return period.slice(5)
  return period
}

function emptyBucket(period: string): Bucket {
  return {
    period,
    requests: 0,
    failed: 0,
    failRate: null,
    tokenIn: 0,
    freshIn: 0,
    cacheRead: 0,
    cacheWrite: 0,
    tokenOut: 0,
    latency: null,
    ttft: null,
    speed: null,
    cost: null,
    priced: 0,
    ttftSamples: 0,
  }
}

// 计数缺失是 0（该桶确实没请求），均值缺失是 null（没有样本），成本缺失是 null（没定价）。
// 这里绝不能用 `?? 0` 一刀切：把均值补成 0 会让图看起来像性能突然变好。
function buildBuckets(stats: ModelTimeSeriesStats, modelId: string): Bucket[] {
  const byPeriod = new Map<string, ModelTimeSeriesPoint>()
  for (const point of stats.data) {
    if (point.model_id === modelId) byPeriod.set(point.period, point)
  }
  return stats.periods.map((period) => {
    const point = byPeriod.get(period)
    if (!point) return emptyBucket(period)
    const cacheRead = point.cache_read_tokens ?? 0
    const cacheWrite = point.cache_write_tokens ?? 0
    const priced = point.priced_requests ?? 0
    const failed = point.failed_requests ?? 0
    return {
      period,
      requests: point.requests,
      failed,
      failRate: point.requests > 0 ? (failed / point.requests) * 100 : null,
      tokenIn: point.token_input,
      freshIn: point.fresh_input_tokens ?? Math.max(0, point.token_input - cacheRead - cacheWrite),
      cacheRead,
      cacheWrite,
      tokenOut: point.token_output,
      latency: point.avg_latency_ms ?? null,
      ttft: (point.ttft_samples ?? 0) > 0 ? (point.avg_ttft_ms ?? 0) : null,
      speed: point.output_speed_tps ?? null,
      cost: priced > 0 ? point.total_cost ?? null : null,
      priced,
      ttftSamples: point.ttft_samples ?? 0,
    }
  })
}

function cacheHitRate(read: number, fresh: number): number | null {
  const denominator = read + fresh
  return denominator > 0 ? (read / denominator) * 100 : null
}

function LegendItem({ color, label, dashed }: { color: string; label: string; dashed?: boolean }) {
  return (
    <span className="wall-timeline-legend-item">
      <span
        className="wall-timeline-legend-swatch"
        style={dashed
          ? { height: 0, background: 'transparent', borderTop: `2px dashed ${color}` }
          : { height: 2, background: color }}
      />
      {label}
    </span>
  )
}

function LaneFrame({ title, note, legend, ariaLabel, children }: {
  title: string
  note?: string
  legend: React.ReactNode
  ariaLabel: string
  children: React.ReactNode
}) {
  return (
    <section className="wall-timeline-lane" role="group" aria-label={ariaLabel}>
      <div className="wall-timeline-lane-head">
        <span className="wall-timeline-lane-title">
          {title}
          {note && <span className="wall-timeline-lane-note">{note}</span>}
        </span>
        <span className="wall-timeline-legend">{legend}</span>
      </div>
      <div className="wall-timeline-lane-chart">{children}</div>
    </section>
  )
}

export function ModelTimeline({ stats, modelId, granularity, locale, colors, summary, onBackToAll }: Props) {
  const buckets = useMemo(() => buildBuckets(stats, modelId), [stats, modelId])
  const [hoveredPeriod, setHoveredPeriod] = useState<string | null>(null)

  useEffect(() => { setHoveredPeriod(null) }, [modelId])

  // 均值/成本在 0 请求桶里是 null：connectNulls=false 时孤立点既没有线段也没有点，
  // 图上看起来像"没有数据"。只在没有非空邻居时补一个点，既不跨缺口连线，也不给稠密序列添噪点。
  const sparseDot = (key: keyof Bucket, color: string) => (props: any) => {
    const { cx, cy, payload } = props
    if (!payload || payload[key] == null || cx == null || cy == null) return <g />
    const index = buckets.findIndex((bucket) => bucket.period === payload.period)
    const prev = index > 0 ? buckets[index - 1][key] : null
    const next = index >= 0 && index < buckets.length - 1 ? buckets[index + 1][key] : null
    if (prev != null || next != null) return <g />
    return <circle cx={cx} cy={cy} r={2.5} fill={color} />
  }

  const totals = useMemo(() => {
    const out = {
      requests: 0, failed: 0, tokenIn: 0, freshIn: 0, cacheRead: 0, cacheWrite: 0,
      tokenOut: 0, priced: 0, cost: 0, hasCost: false, ttftSamples: 0,
    }
    for (const bucket of buckets) {
      out.requests += bucket.requests
      out.failed += bucket.failed
      out.tokenIn += bucket.tokenIn
      out.freshIn += bucket.freshIn
      out.cacheRead += bucket.cacheRead
      out.cacheWrite += bucket.cacheWrite
      out.tokenOut += bucket.tokenOut
      out.priced += bucket.priced
      out.ttftSamples += bucket.ttftSamples
      if (bucket.cost != null) {
        out.cost += bucket.cost
        out.hasCost = true
      }
    }
    return out
  }, [buckets])

  const hasRequests = totals.requests > 0
  const hasFailures = totals.failed > 0
  const hasCacheRead = totals.cacheRead > 0
  const hasCacheWrite = totals.cacheWrite > 0
  const hasCache = hasCacheRead || hasCacheWrite
  const hasOutput = totals.tokenOut > 0
  const hasTokens = totals.tokenIn > 0 || hasOutput
  const hasTtft = totals.ttftSamples > 0
  const hasCost = totals.priced > 0 && totals.hasCost
  const lastBucket = buckets[buckets.length - 1]
  const lastInProgress = lastBucket ? isCurrentPeriod(lastBucket.period, granularity) : false

  // 窗口汇总直接读 model-summaries 的同一行，不从桶数据重算：矩阵与这里必须逐项相等。
  // summaries 失败时 view 为 null，读数条显示 —，由 failbar 的 modelMatrix 点名，不编造数字。
  const windowView = useMemo<Bucket | null>(() => {
    if (!summary) return null
    const priced = summary.priced_requests ?? 0
    const cacheWrite = Math.max(0, summary.token_input - summary.fresh_input_tokens - summary.cache_read_tokens)
    return {
      period: '',
      requests: summary.requests,
      failed: summary.failed_requests,
      failRate: summary.requests > 0 ? (summary.failed_requests / summary.requests) * 100 : null,
      tokenIn: summary.token_input,
      freshIn: summary.fresh_input_tokens,
      cacheRead: summary.cache_read_tokens,
      cacheWrite,
      tokenOut: summary.token_output,
      latency: summary.requests > 0 ? summary.avg_latency_ms : null,
      ttft: summary.stream_requests > 0 ? summary.avg_ttft_ms : null,
      speed: summary.requests > 0 ? summary.output_speed_tps : null,
      cost: priced > 0 ? summary.total_cost ?? null : null,
      priced,
      ttftSamples: summary.stream_requests,
    }
  }, [summary])

  const hoveredBucket = hoveredPeriod ? buckets.find((bucket) => bucket.period === hoveredPeriod) : undefined
  const view = hoveredBucket ?? windowView
  const hoveredIsLast = Boolean(hoveredBucket && lastBucket && hoveredBucket.period === lastBucket.period)
  const showInProgress = hoveredIsLast && lastInProgress

  const items = useMemo(() => {
    const rate = view ? cacheHitRate(view.cacheRead, view.freshIn) : null
    const coverage = view ? formatCostCoverage(view.priced, view.requests) : '—'
    return [
      { key: 'requests', label: t(locale, 'mShortRequests'), value: view ? formatNumber(view.requests) : '—' },
      {
        key: 'failed',
        label: t(locale, 'mShortFailed'),
        value: view
          ? view.failRate == null ? formatNumber(view.failed) : `${formatNumber(view.failed)} · ${view.failRate.toFixed(1)}%`
          : '—',
      },
      { key: 'in', label: t(locale, 'mShortTokenIn'), value: view ? formatCompact(view.tokenIn) : '—' },
      { key: 'fresh', label: t(locale, 'colFreshInput'), value: view ? formatCompact(view.freshIn) : '—' },
      { key: 'out', label: t(locale, 'mShortTokenOut'), value: view ? formatCompact(view.tokenOut) : '—' },
      {
        key: 'cache',
        label: t(locale, 'mShortCache'),
        value: view ? formatCompact(view.cacheRead) : '—',
        title: view
          ? `${t(locale, 'metricCacheRead')} ${formatCompact(view.cacheRead)} · ${t(locale, 'colCacheWrite')} ${formatCompact(view.cacheWrite)} · ${t(locale, 'kpiCacheHit')} ${rate == null ? '—' : `${rate.toFixed(1)}%`}`
          : undefined,
      },
      { key: 'latency', label: t(locale, 'mShortLatency'), value: view?.latency == null ? '—' : formatDuration(view.latency) },
      {
        key: 'ttft',
        label: t(locale, 'mShortTtft'),
        value: view?.ttft == null ? '—' : formatDuration(view.ttft),
        title: view?.ttft == null ? t(locale, 'ttftNoSamples') : undefined,
      },
      { key: 'speed', label: t(locale, 'mShortSpeed'), value: view?.speed == null ? '—' : view.speed.toFixed(1) },
      {
        key: 'cost',
        label: t(locale, 'cost'),
        value: view?.cost == null ? '—' : formatCost(view.cost),
        title: view
          ? view.priced === 0 ? t(locale, 'unpricedModel') : t(locale, 'costCoverage', coverage)
          : undefined,
      },
    ]
  }, [view, locale])

  const curveType: 'linear' | 'monotone' = buckets.length < 8 ? 'linear' : 'monotone'
  const axisTick = { fontSize: 11, fill: colors.axis, style: { fontVariantNumeric: 'tabular-nums' } }
  const rateAxis = hasFailures ? (
    <YAxis
      key="rate-axis"
      yAxisId="rate"
      orientation="right"
      width={RATE_WIDTH}
      domain={[0, 100]}
      tick={axisTick}
      tickFormatter={(value: number) => `${Math.round(value)}%`}
      tickLine={false}
      axisLine={false}
      allowDecimals={false}
    />
  ) : null
  const chartMargin = (usesRateAxis: boolean) => ({
    top: 4,
    right: usesRateAxis ? 0 : RATE_WIDTH,
    left: 0,
    bottom: 0,
  })
  const commonXAxis = (periods: string[]) => (
    <XAxis
      key="x"
      dataKey="period"
      tick={periods.length ? axisTick : false}
      tickLine={false}
      axisLine={{ stroke: colors.grid }}
      height={26}
      interval="preserveStartEnd"
      minTickGap={18}
      tickFormatter={(value: string) => xTickLabel(value, granularity)}
    />
  )
  const hiddenXAxis = (
    <XAxis key="x" dataKey="period" tick={false} tickLine={false} axisLine={false} height={6} />
  )
  const crosshair = hoveredPeriod
    ? <ReferenceLine key="hover" x={hoveredPeriod} stroke={colors.accent} strokeOpacity={0.55} strokeDasharray="4 4" />
    : null
  const grid = <CartesianGrid key="grid" strokeDasharray="3 3" stroke={colors.grid} vertical={false} />

  const onMouseMove = (state: any) => {
    const label = state?.activeLabel
    if (label != null) setHoveredPeriod(String(label))
  }

  const laneKeys: string[] = ['traffic']
  if (hasTtft || totals.requests > 0) laneKeys.push('latency')
  if (hasOutput) laneKeys.push('speed')
  if (hasTokens) laneKeys.push('token')
  if (hasCost) laneKeys.push('cost')
  const lastLane = laneKeys[laneKeys.length - 1]

  const lanes = laneKeys.map((key) => {
    const showX = key === lastLane
    if (key === 'traffic') {
      return {
        key,
        title: t(locale, 'timelineTraffic'),
        note: hasFailures ? undefined : t(locale, 'zeroFailuresNote'),
        legend: (
          <>
            <LegendItem color={colors.accent} label={t(locale, 'mShortRequests')} />
            {hasFailures && <LegendItem color={colors.err} label={t(locale, 'failureRate')} dashed />}
          </>
        ),
        chart: (
          <LineChart data={buckets} margin={chartMargin(hasFailures)} onMouseMove={onMouseMove} accessibilityLayer={false}>
            {grid}
            {showX ? commonXAxis(buckets.map((bucket) => bucket.period)) : hiddenXAxis}
            {/* 左轴保持默认 id：ReferenceLine/Line 不写 yAxisId 时才有轴可挂；右轴才用显式 id */}
            <YAxis key="count-axis" width={Y_WIDTH} tick={axisTick} tickFormatter={(value: number) => formatNumber(value)} tickLine={false} axisLine={false} allowDecimals={false} />
            {rateAxis}
            {crosshair}
            <Line type={curveType} dataKey="requests" name={t(locale, 'mShortRequests')} stroke={colors.accent} strokeWidth={1.5} dot={false} connectNulls={false} isAnimationActive={false} />
            {hasFailures && (
              <Line yAxisId="rate" type={curveType} dataKey="failRate" name={t(locale, 'failureRate')} stroke={colors.err} strokeWidth={1.5} strokeDasharray="4 3" dot={sparseDot('failRate', colors.err)} connectNulls={false} isAnimationActive={false} />
            )}
          </LineChart>
        ),
      }
    }
    if (key === 'latency') {
      return {
        key,
        title: t(locale, 'timelineLatency'),
        note: hasTtft ? undefined : t(locale, 'zeroStreamNote'),
        legend: (
          <>
            <LegendItem color={colors.accent} label={t(locale, 'mShortLatency')} />
            {hasTtft && <LegendItem color={colors.accent} label={t(locale, 'mShortTtft')} dashed />}
          </>
        ),
        chart: (
          <LineChart data={buckets} margin={chartMargin(false)} onMouseMove={onMouseMove} accessibilityLayer={false}>
            {grid}
            {showX ? commonXAxis(buckets.map((bucket) => bucket.period)) : hiddenXAxis}
            <YAxis key="ms-axis" width={Y_WIDTH} tick={axisTick} tickFormatter={(value: number) => value >= 60_000 ? `${(value / 60_000).toFixed(0)}m` : value >= 1000 ? `${(value / 1000).toFixed(1)}s` : `${Math.round(value)}`} tickLine={false} axisLine={false} />
            {crosshair}
            <Line type={curveType} dataKey="latency" name={t(locale, 'mShortLatency')} stroke={colors.accent} strokeWidth={1.5} dot={sparseDot('latency', colors.accent)} connectNulls={false} isAnimationActive={false} />
            {hasTtft && <Line type={curveType} dataKey="ttft" name={t(locale, 'mShortTtft')} stroke={colors.accent} strokeWidth={1.5} strokeDasharray="4 3" dot={sparseDot('ttft', colors.accent)} connectNulls={false} isAnimationActive={false} />}
          </LineChart>
        ),
      }
    }
    if (key === 'speed') {
      return {
        key,
        title: t(locale, 'timelineSpeed'),
        legend: <LegendItem color={colors.accent} label={t(locale, 'mShortSpeed')} />,
        chart: (
          <LineChart data={buckets} margin={chartMargin(false)} onMouseMove={onMouseMove} accessibilityLayer={false}>
            {grid}
            {showX ? commonXAxis(buckets.map((bucket) => bucket.period)) : hiddenXAxis}
            <YAxis key="speed-axis" width={Y_WIDTH} tick={axisTick} tickFormatter={(value: number) => value >= 100 ? Math.round(value).toString() : value.toFixed(1)} tickLine={false} axisLine={false} />
            {crosshair}
            <Line type={curveType} dataKey="speed" name={t(locale, 'mShortSpeed')} stroke={colors.accent} strokeWidth={1.5} dot={sparseDot('speed', colors.accent)} connectNulls={false} isAnimationActive={false} />
          </LineChart>
        ),
      }
    }
    if (key === 'token') {
      return {
        key,
        title: t(locale, 'timelineTokens'),
        note: hasCache ? undefined : t(locale, 'zeroCacheNote'),
        legend: (
          <>
            <LegendItem color={colors.accent} label={t(locale, 'colFreshInput')} />
            {hasCacheRead && <LegendItem color={colors.accent} label={t(locale, 'metricCacheRead')} />}
            {hasCacheWrite && <LegendItem color={colors.accent} label={t(locale, 'colCacheWrite')} />}
            {hasOutput && <LegendItem color={colors.out} label={t(locale, 'mShortTokenOut')} />}
          </>
        ),
        chart: (
          <ComposedChart data={buckets} margin={chartMargin(false)} onMouseMove={onMouseMove} accessibilityLayer={false}>
            {grid}
            {showX ? commonXAxis(buckets.map((bucket) => bucket.period)) : hiddenXAxis}
            <YAxis key="token-axis" width={Y_WIDTH} tick={axisTick} tickFormatter={(value: number) => formatCompact(value)} tickLine={false} axisLine={false} allowDecimals={false} />
            {crosshair}
            <Area type={curveType} dataKey="freshIn" name={t(locale, 'colFreshInput')} stackId="input" stroke={colors.accent} fill={colors.accent} fillOpacity={0.22} strokeWidth={1.2} isAnimationActive={false} />
            {hasCacheRead && <Area type={curveType} dataKey="cacheRead" name={t(locale, 'metricCacheRead')} stackId="input" stroke={colors.accent} fill={colors.accent} fillOpacity={0.12} strokeWidth={1.2} isAnimationActive={false} />}
            {hasCacheWrite && <Area type={curveType} dataKey="cacheWrite" name={t(locale, 'colCacheWrite')} stackId="input" stroke={colors.accent} fill={colors.accent} fillOpacity={0.06} strokeWidth={1.2} isAnimationActive={false} />}
            {hasOutput && <Line type={curveType} dataKey="tokenOut" name={t(locale, 'mShortTokenOut')} stroke={colors.out} strokeWidth={1.5} dot={false} connectNulls={false} isAnimationActive={false} />}
          </ComposedChart>
        ),
      }
    }
    return {
      key: 'cost',
      title: t(locale, 'timelineCost'),
      legend: <LegendItem color={colors.accent} label={t(locale, 'cost')} />,
      chart: (
        <LineChart data={buckets} margin={chartMargin(false)} onMouseMove={onMouseMove} accessibilityLayer={false}>
          {grid}
          {showX ? commonXAxis(buckets.map((bucket) => bucket.period)) : hiddenXAxis}
          <YAxis key="cost-axis" width={Y_WIDTH} tick={axisTick} tickFormatter={(value: number) => formatCost(value)} tickLine={false} axisLine={false} />
          {crosshair}
          <Line type={curveType} dataKey="cost" name={t(locale, 'cost')} stroke={colors.accent} strokeWidth={1.5} dot={sparseDot('cost', colors.accent)} connectNulls={false} isAnimationActive={false} />
        </LineChart>
      ),
    }
  })

  const notes: string[] = []
  if (lastInProgress) notes.push(t(locale, 'partialBucketNote'))
  notes.push(t(locale, 'timelineTzNote'))
  if (buckets.some((bucket) => bucket.requests === 0)) notes.push(t(locale, 'timelineZeroNote'))
  if (!hasOutput) notes.push(t(locale, 'zeroOutputNote'))
  if (!hasCost) notes.push(t(locale, 'unpricedModel'))
  if (!summary) notes.push(t(locale, 'timelineSummaryUnavailable'))

  const altHead = [
    t(locale, 'chartAltPeriod'), t(locale, 'colRequests'), t(locale, 'colFailed'),
    t(locale, 'colTokenIn'), t(locale, 'colFreshInput'), t(locale, 'metricCacheRead'),
    t(locale, 'colCacheWrite'), t(locale, 'colTokenOut'), t(locale, 'colDuration'),
    t(locale, 'colTtft'), t(locale, 'colSpeed'), t(locale, 'cost'),
  ]
  const altRows = buckets.map((bucket) => [
    periodLabel(bucket.period, granularity),
    formatNumber(bucket.requests),
    formatNumber(bucket.failed),
    formatNumber(bucket.tokenIn),
    formatNumber(bucket.freshIn),
    formatNumber(bucket.cacheRead),
    formatNumber(bucket.cacheWrite),
    formatNumber(bucket.tokenOut),
    bucket.latency == null ? '—' : formatDuration(bucket.latency),
    bucket.ttft == null ? '—' : formatDuration(bucket.ttft),
    bucket.speed == null ? '—' : bucket.speed.toFixed(1),
    bucket.cost == null ? '—' : formatCost(bucket.cost),
  ])

  if (!modelId) {
    return <div className="wall-timeline-empty">{t(locale, 'singleModelSelect')}</div>
  }
  if (!hasRequests) {
    return (
      <div className="wall-timeline-empty">
        <span>{t(locale, 'singleModelEmpty')}</span>
        <button type="button" className="wall-timeline-empty-action" onClick={onBackToAll}>
          {t(locale, 'trendModeAll')}
        </button>
      </div>
    )
  }

  return (
    <div className="wall-timeline">
      <div className={`wall-timeline-readout${hoveredBucket ? ' hovered' : ''}`}>
        <div className="wall-timeline-readout-head">
          <span className="wall-timeline-readout-title">
            {hoveredBucket ? periodLabel(hoveredBucket.period, granularity) : t(locale, 'timelineWindowTotal')}
          </span>
          <span className="wall-timeline-readout-tail">
            {showInProgress && <span className="wall-timeline-badge">{t(locale, 'bucketInProgress')}</span>}
            {!hoveredBucket && <span className="wall-timeline-hint">{t(locale, 'timelineHoverHint')}</span>}
          </span>
        </div>
        <div className="wall-timeline-readout-grid">
          {items.map((item) => (
            <div className="wall-timeline-chip" key={item.key} title={item.title}>
              <span className="wall-timeline-chip-label">{item.label}</span>
              <span className="wall-timeline-chip-value">{item.value}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="wall-timeline-lanes" onMouseLeave={() => setHoveredPeriod(null)}>
        {lanes.map((lane) => (
          <LaneFrame key={lane.key} title={lane.title} note={lane.note} legend={lane.legend} ariaLabel={`${lane.title} · ${modelId}`}>
            <ResponsiveContainer width="100%" height="100%">
              {lane.chart}
            </ResponsiveContainer>
          </LaneFrame>
        ))}
      </div>

      <div className="wall-timeline-notes">
        {notes.map((note, index) => <span key={index}>{note}</span>)}
      </div>

      <ChartAltTable
        caption={t(locale, 'timelineAltCaption', modelId, String(buckets.length), t(locale, GRANULARITY_KEY[granularity]))}
        head={altHead}
        rows={altRows}
      />
    </div>
  )
}
