import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Button, Table, Tooltip as UiTooltip } from '@mantine/core'
import { useNavigate } from 'react-router-dom'
import { IconArrowRight, IconRefresh } from '@tabler/icons-react'
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import {
  getErrorStats,
  getLatencyPercentiles,
  getModelSummaries,
  getModelTimeSeries,
  getOverview,
  getRequestLogs,
  listAllRoutes,
  listApiKeys,
  listPlatforms,
  listProxies,
} from '../api'
import { rangeToParams, useAppState } from '../app/AppState'
import { useLive, usePollInterval } from '../app/LiveContext'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { EmptyState, ErrorState, PageSkeleton, StaleBadge } from '../components/ui/States'
import { ChartAltTable, peakOf } from '../components/ui/ChartAlt'
import { Readout } from '../components/ui/Readout'
import { StatusPill } from '../components/ui/StatusPill'
import {
  deltaPercent,
  formatCompact,
  formatCost,
  formatCostCoverage,
  formatDuration,
  formatLogTime,
  formatNumber,
  httpTone,
  successRateTone,
} from '../lib/format'
import type {
  ApiKeyRow,
  ErrorStats,
  LatencyPercentiles,
  ModelSummaryRow,
  ModelTimeSeriesStats,
  OverviewStats,
  PlatformRow,
  ProxyRow,
  RequestLogPage,
  RouteRow,
} from '../types/stats'

interface OverviewData {
  stats: OverviewStats
  latency: LatencyPercentiles
  summaries: ModelSummaryRow[]
  errors: ErrorStats
  logs: RequestLogPage
  keys: ApiKeyRow[]
  proxies: ProxyRow[]
  platforms: PlatformRow[]
  backendCount: number
  series: ModelTimeSeriesStats | null
}

function Delta({ value, higherIsBetter = true }: { value: number | null; higherIsBetter?: boolean }) {
  if (value === null) return <span className="console-delta flat">—</span>
  const good = (value >= 0) === higherIsBetter
  return (
    <span className={`console-delta ${good ? 'good' : 'bad'}`}>
      {value >= 0 ? '↑' : '↓'}{Math.abs(value).toFixed(1)}%
    </span>
  )
}

function Sparkline({ values }: { values: number[] }) {
  if (values.length < 2) return null
  const max = Math.max(...values)
  const min = Math.min(...values)
  const span = max - min || 1
  const points = values.map((value, index) => {
    const x = (index / (values.length - 1)) * 62
    const y = 16 - ((value - min) / span) * 12
    return `${x.toFixed(1)},${y.toFixed(1)}`
  }).join(' ')
  return (
    <svg className="console-spark" viewBox="0 0 62 18" aria-hidden="true">
      <polyline points={points} fill="none" stroke="var(--accent)" strokeWidth="1.3" />
    </svg>
  )
}

function SignalSpine({
  clients,
  models,
  routes,
  backends,
  degradedModels,
  onNavigate,
}: {
  clients: number
  models: number
  routes: number
  backends: number
  degradedModels: number
  onNavigate: (path: string) => void
}) {
  const { locale } = useAppContext()
  const nodes = [
    { key: 'clients', kicker: t(locale, 'spineClients'), value: clients, label: t(locale, 'apiKeys'), state: t(locale, 'keysCount', String(clients)), path: '/keys', tone: 'ok' as const },
    { key: 'models', kicker: t(locale, 'spineVirtualModels'), value: models, label: t(locale, 'proxies'), state: degradedModels > 0 ? t(locale, 'stateDegraded', String(degradedModels)) : t(locale, 'stateHealthy'), path: '/models', tone: degradedModels > 0 ? 'warn' as const : 'ok' as const, primary: true },
    { key: 'routes', kicker: t(locale, 'spineRoutes'), value: routes, label: t(locale, 'routes'), state: t(locale, 'stateOneToOne'), path: '/models', tone: 'ok' as const },
    { key: 'backends', kicker: t(locale, 'spineBackends'), value: backends, label: t(locale, 'backends'), state: degradedModels > 0 ? t(locale, 'stateAffected', String(degradedModels)) : t(locale, 'stateReady'), path: '/providers', tone: degradedModels > 0 ? 'warn' as const : 'ok' as const },
  ]

  return (
    <section className="console-panel console-spine" aria-label={t(locale, 'modelHealth')}>
      <div className="console-spine-head">
        <div>
          <div className="console-kicker">{t(locale, 'kickerLive')}</div>
          <h2 className="console-spine-title">{t(locale, 'routeSpine')}</h2>
        </div>
        <div className="console-spine-meta">
          <span>{t(locale, 'nodesCount', formatNumber(clients + models + backends))}</span>
          {degradedModels > 0 && <><span className="console-dot warn" /><span>{t(locale, 'stateDegraded', String(degradedModels))}</span></>}
        </div>
      </div>
      <div className="console-spine-track">
        {nodes.map((node, index) => (
          <div className="console-spine-segment" key={node.key}>
            <button
              type="button"
              className={`console-spine-node${node.primary ? ' primary' : ''}${node.tone === 'warn' ? ' warn' : ''}`}
              onClick={() => onNavigate(node.path)}
            >
              <span className="console-spine-node-top">
                <span>{node.kicker}</span>
                <span className={`console-dot ${node.tone}`} />
              </span>
              <span className="console-spine-value">{node.value}</span>
              <span className="console-spine-label">{node.label}</span>
              <span className="console-spine-state">{node.state}</span>
            </button>
            {index < nodes.length - 1 && (
              <span className="console-spine-link" aria-hidden="true">
                <span className="console-packet" />
                <span className="console-packet p2" />
              </span>
            )}
          </div>
        ))}
      </div>
      <div className="console-spine-foot">
        <span><strong>{t(locale, 'currentChain')}</strong> · {t(locale, 'chainFlow')}</span>
        <button type="button" className="console-text-btn" onClick={() => onNavigate('/traffic')}>
          {t(locale, 'viewTraffic')} <IconArrowRight size={12} />
        </button>
      </div>
    </section>
  )
}

function SetupChecklist({
  platforms,
  proxies,
  keys,
  requests,
  onNavigate,
}: {
  platforms: number
  proxies: number
  keys: number
  requests: number
  onNavigate: (path: string) => void
}) {
  const { locale } = useAppContext()
  const steps = [
    { done: platforms > 0, title: t(locale, 'stepPlatforms'), path: '/providers' },
    { done: proxies > 0, title: t(locale, 'stepModels'), path: '/models' },
    { done: keys > 0, title: t(locale, 'stepKeys'), path: '/keys' },
    { done: requests > 0, title: t(locale, 'stepRequests'), path: '/playground' },
  ]
  return (
    <section className="console-panel console-setup">
      <div className="console-setup-copy">
        <div className="console-kicker">{t(locale, 'kickerGetStarted')}</div>
        <h2>{t(locale, 'gettingStartedTitle')}</h2>
        <p>{t(locale, 'setupDesc')}</p>
        <Button size="xs" mt="sm" onClick={() => onNavigate('/setup')}>{t(locale, 'wizardOpen')}</Button>
      </div>
      <div className="console-setup-steps">
        {steps.map((step, index) => (
          <button
            key={step.path}
            type="button"
            className={`console-setup-step${step.done ? ' done' : ''}`}
            onClick={() => onNavigate(step.path)}
          >
            <span className="console-setup-index">0{index + 1}</span>
            <span>{step.title}</span>
            <span className={`console-dot ${step.done ? 'ok' : 'idle'}`} />
          </button>
        ))}
      </div>
    </section>
  )
}

export default function Overview() {
  const navigate = useNavigate()
  const { locale, isDark } = useAppContext()
  const { range, live } = useAppState()
  const pollInterval = usePollInterval()
  const { status } = useLive()

  const { data, isPending, isError, refetch, dataUpdatedAt } = useQuery({
    queryKey: ['overview', range],
    queryFn: async (): Promise<OverviewData> => {
      const { from, to } = rangeToParams(range)
      const [stats, latency, summaries, errors, logs, keys, proxies, platforms, routes] = await Promise.all([
        getOverview({ from, to }) as Promise<OverviewStats>,
        getLatencyPercentiles({ from, to }) as Promise<LatencyPercentiles>,
        getModelSummaries({ from, to }) as Promise<ModelSummaryRow[]>,
        getErrorStats({ from, to }) as Promise<ErrorStats>,
        getRequestLogs({ from, to, status: 'fail', page: 1, page_size: 5 }) as Promise<RequestLogPage>,
        listApiKeys() as Promise<ApiKeyRow[]>,
        listProxies() as Promise<ProxyRow[]>,
        listPlatforms() as Promise<PlatformRow[]>,
        listAllRoutes().catch(() => []) as Promise<RouteRow[]>,
      ])
      const backendCount = routes.reduce((count, route) => count + (route.backends?.length ?? 0), 0)
      const series = await getModelTimeSeries(
        range === '7d' || range === '30d' ? 'day' : 'hour',
        { from, to },
      ).catch(() => null) as ModelTimeSeriesStats | null

      return { stats, latency, summaries, errors, logs, keys, proxies, platforms, backendCount, series }
    },
    refetchInterval: pollInterval,
  })

  const chartData = useMemo(() => {
    if (!data?.series) return []
    return data.series.periods.map(period => {
      const points = data.series!.data.filter(point => point.period === period)
      return {
        period,
        requests: points.reduce((sum, point) => sum + point.requests, 0),
        failures: points.reduce((sum, point) => sum + (point.failed_requests ?? 0), 0),
      }
    })
  }, [data?.series])

  // Legend 可点：点掉的系列从图里撤掉，但文本替代仍给全量——读屏的人不该因为谁点了图例少一行数据
  const [hiddenSeries, setHiddenSeries] = useState<string[]>([])
  const toggleSeries = (key: string) =>
    setHiddenSeries(prev => (prev.includes(key) ? prev.filter(x => x !== key) : [...prev, key]))

  // 全新安装（无平台/模型/密钥）第一次进总览时直接接入向导；跳过或完成后不再打断
  useEffect(() => {
    if (!data || localStorage.getItem('wizard')) return
    const fresh = data.platforms.length === 0 && data.proxies.length === 0 && data.keys.length === 0
    if (!fresh) return
    localStorage.setItem('wizard', 'seen')
    navigate('/setup', { replace: true })
  }, [data, navigate])

  if (isPending) return <PageSkeleton />
  if (isError && !data) return <ErrorState onRetry={() => void refetch()} />
  if (!data) return null

  const { stats, latency, summaries, errors, logs, keys, proxies, platforms, backendCount, series } = data
  const failedCount = Math.max(0, errors.total - errors.success)
  const failedModels = summaries.filter(model => model.requests > 0 && model.failed_requests > 0)
  const topModels = [...summaries].sort((a, b) => b.requests - a.requests).slice(0, 6)
  const bottleneck = [...summaries].filter(model => model.requests > 0).sort((a, b) => b.avg_latency_ms - a.avg_latency_ms)[0]
  const idleKey = keys.find(key => !key.last_used)
  const topError = [...errors.errors].sort((a, b) => b.count - a.count)[0]
  const lastSync = dataUpdatedAt
    ? new Date(dataUpdatedAt).toLocaleTimeString(locale === 'zh' ? 'zh-CN' : 'en-US', { hour12: false })
    : '—'
  const issueCount = (topError ? 1 : 0) + (failedModels.length > 0 ? 1 : 0) + (idleKey ? 1 : 0)
  const requestsDelta = deltaPercent(stats.total_requests, stats.requests_prev_period)
  const latencyDelta = deltaPercent(stats.avg_latency_ms, stats.avg_latency_prev_period)
  const totalTokens = stats.total_token_input + stats.total_token_output
  const missingSetup = platforms.length === 0 || proxies.length === 0 || keys.length === 0
  const chartStroke = isDark ? '#5C9DFF' : '#2E6BE6'
  const errorStroke = isDark ? '#FF6B7A' : '#DC2626'
  const axisColor = isDark ? '#8FA3CE' : '#40507A'
  const gridColor = isDark ? 'rgba(140,180,255,.11)' : 'rgba(18,38,86,.09)'

  const errorLabels: Record<string, string> = {
    RateLimit: locale === 'zh' ? '限流' : 'Rate limit',
    ServerError: locale === 'zh' ? '上游错误' : 'Server error',
    Timeout: locale === 'zh' ? '超时' : 'Timeout',
    ConnectionError: locale === 'zh' ? '连接错误' : 'Connection error',
    StreamError: locale === 'zh' ? '流中断' : 'Stream error',
    UpstreamError: locale === 'zh' ? '上游异常' : 'Upstream error',
    Unknown: locale === 'zh' ? '未知' : 'Unknown',
  }

  const tickFormatter = (value: string) => {
    if (series?.granularity === 'hour') return `${value.slice(11, 13)}:00`
    if (series?.granularity === 'day') return value.slice(5)
    return value
  }

  const trendPeak = peakOf(chartData, point => point.requests)
  const trendTitle = `${t(locale, 'traffic')} / ${range.toUpperCase()}`
  const trendSummary = t(locale, 'chartAltTrend', trendTitle, String(chartData.length),
    trendPeak ? formatNumber(trendPeak.requests) : '—', trendPeak ? tickFormatter(trendPeak.period) : '—')

  return (
    <div className="console-page">
      <div className="console-status-strip">
        <span className="console-dot ok" />
        <strong>{t(locale, 'localInstance')}</strong>
        <span>{t(locale, 'running')}</span>
        <span className="console-status-sep">·</span>
        <span>{t(locale, 'lastSync')} {lastSync}</span>
        <span className="console-status-right">
          {isError && <StaleBadge updatedAt={dataUpdatedAt} />}
          <span className={`console-issue${issueCount > 0 ? ' warn' : ''}`}>
            <span className={`console-dot ${issueCount > 0 ? 'warn' : 'ok'}`} />
            {issueCount > 0 ? t(locale, 'needsAttention', String(issueCount)) : t(locale, 'allHealthy')}
          </span>
          <button type="button" className="console-text-btn" onClick={() => navigate('/traffic')}>
            {t(locale, 'viewTraffic')} <IconArrowRight size={12} />
          </button>
          <button type="button" className="console-icon-btn" onClick={() => void refetch()} title={t(locale, 'refresh')}>
            <IconRefresh size={14} />
          </button>
        </span>
      </div>

      <SignalSpine
        clients={keys.length}
        models={proxies.length}
        routes={proxies.length}
        backends={backendCount}
        degradedModels={failedModels.length}
        onNavigate={navigate}
      />

      <div className="console-readouts">
        <Readout
          label={t(locale, 'totalRequests').toUpperCase()}
          value={formatNumber(stats.total_requests)}
          meta={<><Delta value={requestsDelta} /> <span>{range.toUpperCase()}</span></>}
          spark={<Sparkline values={chartData.map(point => point.requests)} />}
        />
        <Readout
          label={t(locale, 'successRate').toUpperCase()}
          value={(stats.success_rate || 0).toFixed(1)}
          unit="%"
          meta={<><span>{formatNumber(errors.success)} {t(locale, 'successCount')}</span><span>{formatNumber(failedCount)} {t(locale, 'failureCount')}</span></>}
        />
        <Readout
          label={t(locale, 'p90Latency').toUpperCase()}
          value={formatDuration(latency.p90)}
          meta={<><Delta value={latencyDelta} higherIsBetter={false} /> <span>P50 {formatDuration(latency.p50)} · P99 {formatDuration(latency.p99)}</span></>}
        />
        <Readout
          label={t(locale, 'tokens').toUpperCase()}
          value={formatCompact(totalTokens)}
          meta={<><span>{t(locale, 'inputTokens')} {formatCompact(stats.total_token_input)}</span><span>{t(locale, 'outputTokens')} {formatCompact(stats.total_token_output)}</span><UiTooltip withArrow label={t(locale, 'costCoverageHint', String(stats.priced_requests), String(stats.total_requests))}><span>{t(locale, 'cost')} {formatCost(stats.total_cost)} · {formatCostCoverage(stats.priced_requests, stats.total_requests)}</span></UiTooltip></>}
        />
      </div>

      {missingSetup && (
        <SetupChecklist
          platforms={platforms.length}
          proxies={proxies.length}
          keys={keys.length}
          requests={stats.total_requests}
          onNavigate={navigate}
        />
      )}

      <div className="console-grid-main">
        <section className="console-panel">
          <div className="console-panel-head">
            <div className="console-panel-title">{t(locale, 'traffic')} / {range.toUpperCase()}</div>
            <div className="console-chart-legend">
              {([['requests', t(locale, 'requests'), ''], ['failures', t(locale, 'failures'), ' err']] as const).map(
                ([key, label, swatch]) => (
                  <button
                    type="button"
                    key={key}
                    className="console-legend-item"
                    aria-pressed={!hiddenSeries.includes(key)}
                    title={t(locale, 'chartSeriesToggle')}
                    onClick={() => toggleSeries(key)}
                  >
                    <span className={`console-legend-swatch${swatch}`} />{label}
                  </button>
                )
              )}
            </div>
          </div>
          <div className="console-panel-body console-chart-body" role="group" aria-label={trendSummary}>
            {chartData.length > 0 ? (
              <>
                <ResponsiveContainer width="100%" height={248}>
                  <LineChart data={chartData} margin={{ top: 8, right: 8, left: -12, bottom: 0 }}>
                    <CartesianGrid strokeDasharray="3 3" stroke={gridColor} vertical={false} />
                    <XAxis
                      dataKey="period"
                      tick={{ fontSize: 11, fill: axisColor }}
                      tickLine={false}
                      axisLine={{ stroke: gridColor }}
                      tickFormatter={tickFormatter}
                      interval="preserveStartEnd"
                    />
                    <YAxis
                      tick={{ fontSize: 11, fill: axisColor }}
                      tickLine={false}
                      axisLine={false}
                      tickFormatter={value => formatCompact(Number(value))}
                      width={44}
                      allowDecimals={false}
                    />
                    <Tooltip
                      contentStyle={{ background: isDark ? '#121F3D' : '#FFFFFF', border: `1px solid ${gridColor}`, borderRadius: 8, fontSize: 12 }}
                      labelStyle={{ color: axisColor, fontFamily: 'var(--font-mono)', fontSize: 11 }}
                      formatter={(value) => formatNumber(Number(value))}
                    />
                    <Line type="monotone" dataKey="requests" name={t(locale, 'requests')} stroke={chartStroke} strokeWidth={1.6} dot={false} animationDuration={400} hide={hiddenSeries.includes('requests')} />
                    <Line type="monotone" dataKey="failures" name={t(locale, 'failures')} stroke={errorStroke} strokeWidth={1.2} strokeDasharray="3 4" dot={false} animationDuration={400} hide={hiddenSeries.includes('failures')} />
                  </LineChart>
                </ResponsiveContainer>
                <ChartAltTable
                  caption={t(locale, 'chartAltCaption', trendTitle)}
                  head={[t(locale, 'chartAltPeriod'), t(locale, 'requests'), t(locale, 'failures')]}
                  rows={chartData.map(point => [tickFormatter(point.period), point.requests, point.failures])}
                />
              </>
            ) : (
              <EmptyState title={t(locale, 'noData')} description={t(locale, 'noRequests')} />
            )}
          </div>
          <div className="console-panel-foot">
            {/* 连上流之后不该再承诺"30s"：数字是事件驱动的，轮询只是断线兜底 */}
            <span>{live ? t(locale, status === 'live' ? 'liveTail' : status === 'connecting' ? 'liveConnecting' : 'livePolling') : t(locale, 'paused')}</span>
            <span>{bottleneck ? <>P90 {formatDuration(bottleneck.avg_latency_ms)} · {bottleneck.model_id}</> : '—'}</span>
          </div>
        </section>

        <section className="console-panel">
          <div className="console-panel-head">
            <div className="console-panel-title">{t(locale, 'attentionTitle')}</div>
            <StatusPill tone={issueCount > 0 ? 'warn' : 'ok'}>{issueCount > 0 ? t(locale, 'needsAttention', String(issueCount)) : t(locale, 'allHealthy')}</StatusPill>
          </div>
          <div className="console-attention-list">
            {topError && (
              <button type="button" className="console-attention-item err" onClick={() => navigate('/traffic')}>
                <span className="console-attention-bar" />
                <span className="console-attention-main">
                  <span className="console-attention-title">{errorLabels[topError.kind] || topError.kind} · {topError.count}</span>
                  <span className="console-attention-meta">{t(locale, 'failures')} · {range.toUpperCase()}</span>
                </span>
                <span className="console-attention-chev">›</span>
              </button>
            )}
            {failedModels.length > 0 && (
              <button type="button" className="console-attention-item warn" onClick={() => navigate('/models')}>
                <span className="console-attention-bar" />
                <span className="console-attention-main">
                  <span className="console-attention-title">{failedModels[0].model_id} · {failedModels[0].failed_requests} {t(locale, 'failures')}</span>
                  <span className="console-attention-meta">{t(locale, 'successShort')} {((failedModels[0].success_requests / Math.max(1, failedModels[0].requests)) * 100).toFixed(1)}%</span>
                </span>
                <span className="console-attention-chev">›</span>
              </button>
            )}
            {idleKey && (
              <button type="button" className="console-attention-item idle" onClick={() => navigate('/keys')}>
                <span className="console-attention-bar" />
                <span className="console-attention-main">
                  <span className="console-attention-title">Key {idleKey.name} · {t(locale, 'noTraffic')}</span>
                  <span className="console-attention-meta">{new Date(idleKey.created_at).toLocaleDateString()}</span>
                </span>
                <span className="console-attention-chev">›</span>
              </button>
            )}
            {!topError && failedModels.length === 0 && !idleKey && (
              <div className="console-attention-empty">{t(locale, 'noAttention')}</div>
            )}
          </div>
          <div className="console-panel-foot">
            <span>{t(locale, 'attentionTitle')}</span>
            <button type="button" className="console-text-btn" onClick={() => navigate('/traffic')}>
              {t(locale, 'viewTraffic')} <IconArrowRight size={12} />
            </button>
          </div>
        </section>
      </div>

      <div className="console-grid-lower">
        <section className="console-panel">
          <div className="console-panel-head">
            <div className="console-panel-title">{t(locale, 'modelHealth')}</div>
            <span className="console-panel-sub">{t(locale, 'modelsCount', String(summaries.length))}</span>
          </div>
          {topModels.length > 0 ? (
            <div className="console-table-wrap">
              <Table className="console-table">
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>{t(locale, 'model')}</Table.Th>
                    <Table.Th>{t(locale, 'status')}</Table.Th>
                    <Table.Th className="num">{t(locale, 'requests')}</Table.Th>
                    <Table.Th className="num">{t(locale, 'successShort')}</Table.Th>
                    <Table.Th className="num">{t(locale, 'avgLatencyShort')}</Table.Th>
                    <Table.Th className="num">{t(locale, 'failures')}</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {topModels.map(model => {
                    const rate = model.requests ? (model.success_requests / model.requests) * 100 : 0
                    const tone = successRateTone(rate, model.requests > 0)
                    return (
                      <Table.Tr key={model.model_id} onClick={() => navigate('/models')} style={{ cursor: 'pointer' }}>
                        <Table.Td><span className="console-model-name"><span className={`console-dot ${tone}`} />{model.model_id}</span></Table.Td>
                        <Table.Td><StatusPill tone={tone}>{t(locale, tone === 'ok' ? 'healthHealthy' : tone === 'warn' ? 'healthDegraded' : tone === 'err' ? 'healthFailing' : 'healthIdle')}</StatusPill></Table.Td>
                        <Table.Td className="num">{formatNumber(model.requests)}</Table.Td>
                        <Table.Td className="num">{rate.toFixed(1)}%</Table.Td>
                        <Table.Td className="num">{formatDuration(model.avg_latency_ms)}</Table.Td>
                        <Table.Td className="num" style={{ color: model.failed_requests > 0 ? 'var(--err)' : undefined }}>{formatNumber(model.failed_requests)}</Table.Td>
                      </Table.Tr>
                    )
                  })}
                </Table.Tbody>
              </Table>
            </div>
          ) : (
            <EmptyState title={t(locale, 'noData')} description={t(locale, 'noRequests')} />
          )}
          <div className="console-panel-foot">
            <span>{t(locale, 'modelHealth')}</span>
            <button type="button" className="console-text-btn" onClick={() => navigate('/models')}>
              {t(locale, 'openModel')} <IconArrowRight size={12} />
            </button>
          </div>
        </section>

        <section className="console-panel">
          <div className="console-panel-head">
            <div className="console-panel-title">{t(locale, 'recentFailures')}</div>
            <span className="console-panel-sub">{range.toUpperCase()}</span>
          </div>
          {logs.items.length > 0 ? (
            <div className="console-fault-list">
              {logs.items.map(item => (
                <button
                  key={item.id}
                  type="button"
                  className="console-fault-row"
                  onClick={() => navigate('/traffic')}
                >
                  <span className="console-fault-time">{formatLogTime(item.created_at, locale)}</span>
                  <span className="console-fault-main">
                    <span className="console-fault-model">{item.model_id}</span>
                    <span className="console-fault-meta">{item.platform_name} · {item.error_type || '—'}</span>
                  </span>
                  <span className="console-fault-right">
                    <span className="console-fault-code">{item.status_code}</span>
                    <span className="console-fault-latency">{formatDuration(item.latency_ms)}</span>
                  </span>
                </button>
              ))}
            </div>
          ) : (
            <EmptyState title={t(locale, 'noRecentFailures')} description={t(locale, 'noData')} />
          )}
          <div className="console-panel-foot">
            <span>{t(locale, 'recentFailures')}</span>
            <button type="button" className="console-text-btn" onClick={() => navigate('/traffic')}>
              {t(locale, 'viewTraffic')} <IconArrowRight size={12} />
            </button>
          </div>
        </section>
      </div>
    </div>
  )
}
