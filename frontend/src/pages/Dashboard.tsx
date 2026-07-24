import { useEffect, useState, useCallback, useMemo } from 'react'
import {
  Card,
  SimpleGrid,
  Text,
  Badge,
  Group,
  Stack,
  ThemeIcon,
  ActionIcon,
  Tooltip,
  Center,
  SegmentedControl,
  LoadingOverlay,
} from '@mantine/core'
import {
  AreaChart as ReAreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip as RTooltip,
  Legend,
  ResponsiveContainer,
} from 'recharts'
import {
  IconServer,
  IconApi,
  IconRefresh,
  IconChartBar,
} from '@tabler/icons-react'
import { getOverview, listProxies, listPlatforms, getProxyStats, getPlatformStats, getModelTimeSeries } from '../api'
import { useAppContext } from '../ThemeContext'
import { t, type Locale } from '../i18n'
import { getPresetName, platformPresets } from '../presets'

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

function formatNumber(n: number | undefined): string {
  return (n ?? 0).toLocaleString()
}

function formatTokenCount(n: number | undefined): string {
  const v = n ?? 0
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`
  if (v >= 1_000) return `${(v / 1_000).toFixed(1)}K`
  return v.toString()
}

const successRateColor = (rate: number) => {
  if (rate > 90) return '#52c41a'
  if (rate > 70) return '#fa8c16'
  return '#f5222d'
}

const dashStyles: Record<string, React.CSSProperties> = {
  itemCard: {
    background: 'var(--mantine-color-body)',
    border: '1px solid var(--mantine-color-default-border)',
    borderRadius: 'var(--mantine-radius-sm)',
    padding: '8px 10px',
    transition: 'all 0.2s ease',
    cursor: 'default',
  },
  itemCardHover: {
    borderColor: 'var(--mantine-primary-color-filled)',
    boxShadow: '0 2px 8px rgba(0,0,0,0.08)',
    transform: 'translateY(-1px)',
  },
  sectionTitle: {
    fontWeight: 600 as const,
    fontSize: '13px',
  },
  sectionCount: {
    fontSize: '11px',
    color: 'var(--mantine-color-dimmed)',
    background: 'var(--mantine-color-gray-1)',
    padding: '1px 8px',
    borderRadius: '10px',
  },
}

function ItemCard({ children }: { children: React.ReactNode }) {
  const [hovered, setHovered] = useState(false)
  return (
    <div
      style={{
        ...dashStyles.itemCard,
        ...(hovered ? dashStyles.itemCardHover : {}),
      }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {children}
    </div>
  )
}

// ---- 模型请求统计图表 ----
interface ModelTimeSeriesPoint {
  period: string
  model_id: string
  requests: number
  token_input: number
  token_output: number
}

interface ModelTimeSeriesStats {
  granularity: string
  periods: string[]
  data: ModelTimeSeriesPoint[]
}

type Metric = 'requests' | 'token_input' | 'token_output'
type Granularity = 'day' | 'month' | 'year'

const MODEL_CHART_COLORS = [
  '#228be6', '#7048e8', '#0ca678', '#fd7e14', '#f06595',
  '#4c6ef5', '#1098ad', '#82c91e', '#fa5252', '#cc5de8',
  '#fab005', '#37b24d',
]

function ModelStatsSection({ locale }: { locale: Locale }) {
  const [granularity, setGranularity] = useState<Granularity>('day')
  const [metric, setMetric] = useState<Metric>('requests')
  const [data, setData] = useState<ModelTimeSeriesStats | null>(null)
  const [loading, setLoading] = useState(false)
  const [refreshNonce, setRefreshNonce] = useState(0)

  const loadData = useCallback(async (g: Granularity) => {
    setLoading(true)
    try {
      const result = await getModelTimeSeries(g)
      setData(result)
    } catch {
      setData(null)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    loadData(granularity)
  }, [granularity, loadData, refreshNonce])

  // 将扁平数据透视为图表所需格式：每个周期一行，每个模型一列。
  // 注意：模型名可能包含 "." 或 "/"（如 "Qwen/Qwen3.5-122B-A10B"），
  // recharts 会把 dataKey 中的 "." 当作对象路径解析导致取不到值，
  // 因此用安全索引 "m{index}" 作为 dataKey，用 label 显示真实模型名。
  const { chartData, series } = useMemo(() => {
    if (!data) return { chartData: [], series: [] as { name: string; label: string; color: string }[] }
    const modelSet = new Set<string>()
    data.data.forEach((p) => modelSet.add(p.model_id))
    const models = Array.from(modelSet)

    const lookup: Record<string, Record<string, ModelTimeSeriesPoint>> = {}
    data.data.forEach((p) => {
      if (!lookup[p.period]) lookup[p.period] = {}
      lookup[p.period][p.model_id] = p
    })

    const chartData = data.periods.map((period) => {
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
      color: MODEL_CHART_COLORS[i % MODEL_CHART_COLORS.length],
    }))

    return { chartData, series }
  }, [data, metric])

  const valueFormatter = (value: number) => {
    if (metric === 'requests') return value.toLocaleString()
    return formatTokenCount(value)
  }

  const hasData = series.length > 0

  return (
    <div>
      <Group justify="space-between" mb="xs" align="center">
        <Group gap="xs">
          <ThemeIcon variant="light" size={28} radius="sm" color="blue">
            <IconChartBar size={16} />
          </ThemeIcon>
          <Text style={dashStyles.sectionTitle}>{t(locale, 'modelStats')}</Text>
        </Group>
        <Group gap="xs">
          <SegmentedControl
            size="xs"
            value={metric}
            onChange={(v) => setMetric(v as Metric)}
            data={[
              { label: t(locale, 'metricRequests'), value: 'requests' },
              { label: t(locale, 'inputTokens'), value: 'token_input' },
              { label: t(locale, 'outputTokens'), value: 'token_output' },
            ]}
          />
          <SegmentedControl
            size="xs"
            value={granularity}
            onChange={(v) => setGranularity(v as Granularity)}
            data={[
              { label: t(locale, 'granularityDay'), value: 'day' },
              { label: t(locale, 'granularityMonth'), value: 'month' },
              { label: t(locale, 'granularityYear'), value: 'year' },
            ]}
          />
          <Tooltip label={t(locale, 'refresh')}>
            <ActionIcon variant="subtle" size="sm" loading={loading} onClick={() => setRefreshNonce((n) => n + 1)}>
              <IconRefresh size={14} />
            </ActionIcon>
          </Tooltip>
        </Group>
      </Group>

      <Card
        padding="sm"
        style={{ position: 'relative', minHeight: 320, ...dashStyles.itemCard }}
      >
        <LoadingOverlay visible={loading} />
        {hasData ? (
          <div style={{ width: '100%', height: 300 }}>
            <ResponsiveContainer width="100%" height="100%">
              <ReAreaChart data={chartData} margin={{ top: 10, right: 10, left: 0, bottom: 5 }}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="period" interval={granularity === 'day' ? 2 : 0} angle={granularity === 'day' ? -30 : 0} textAnchor={granularity === 'day' ? 'end' : 'middle'} height={granularity === 'day' ? 60 : 30} tick={{ fontSize: 11 }} />
                <YAxis width={60} tick={{ fontSize: 11 }} tickFormatter={valueFormatter} />
                <RTooltip formatter={valueFormatter} />
                <Legend wrapperStyle={{ fontSize: 11 }} />
                {series.map((s) => (
                  <Area
                    key={s.name}
                    type="monotone"
                    dataKey={s.name}
                    name={s.label}
                    stackId="1"
                    stroke={s.color}
                    fill={s.color}
                    fillOpacity={0.35}
                    strokeWidth={1.5}
                    isAnimationActive={false}
                  />
                ))}
              </ReAreaChart>
            </ResponsiveContainer>
          </div>
        ) : (
          <Center h={300}>
            <Stack align="center" gap="xs">
              <ThemeIcon variant="light" size={36} radius="xl" color="gray">
                <IconChartBar size={18} />
              </ThemeIcon>
              <Text size="xs" c="dimmed">{t(locale, 'noChartData')}</Text>
            </Stack>
          </Center>
        )}
      </Card>
    </div>
  )
}

export default function Dashboard() {
  const [stats, setStats] = useState<OverviewStats>({
    total_requests: 0, success_rate: 0, avg_latency_ms: 0,
    active_proxies: 0, total_proxies: 0,
    active_platforms: 0, total_platforms: 0, total_models: 0,
    total_token_input: 0, total_token_output: 0,
  })
  const [proxies, setProxies] = useState<ProxyWithStats[]>([])
  const [platforms, setPlatforms] = useState<PlatformWithStats[]>([])
  const [loading, setLoading] = useState(false)
  const { locale } = useAppContext()

  const loadData = useCallback(async () => {
    setLoading(true)
    try {
      const [overview, proxyList, platformList] = await Promise.all([
        getOverview().catch(() => ({})),
        listProxies().catch(() => []),
        listPlatforms().catch(() => []),
      ])
      setStats(overview as OverviewStats)

      const proxiesWithStats: ProxyWithStats[] = await Promise.all(
        proxyList.map(async (p: any) => {
          try {
            const pStats = await getProxyStats(p.id).catch(() => null)
            return { ...p, stats: pStats || undefined }
          } catch {
            return { ...p }
          }
        })
      )
      setProxies(proxiesWithStats)

      const platformsWithStats: PlatformWithStats[] = await Promise.all(
        platformList.map(async (p: any) => {
          try {
            const pStats = await getPlatformStats(p.id).catch(() => null)
            return { ...p, stats: pStats || undefined }
          } catch {
            return { ...p }
          }
        })
      )
      setPlatforms(platformsWithStats)
    } catch {
      // silently fail
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    loadData()
  }, [loadData])

  const getPlatformDisplayName = (name: string) => {
    const preset = platformPresets.find((p) => p.name === name)
    return preset ? getPresetName(preset, locale) : name
  }

  const totalTokens = (stats.total_token_input || 0) + (stats.total_token_output || 0)

  return (
    <Stack gap="sm">
      {/* Header */}
      <Group justify="space-between">
        <Text fw={700} size="lg">
          {t(locale, 'dashboard')}
        </Text>
        <Tooltip label={t(locale, 'refresh')}>
          <ActionIcon variant="subtle" size="lg" onClick={loadData} loading={loading}>
            <IconRefresh size={18} />
          </ActionIcon>
        </Tooltip>
      </Group>

      {/* Stats Row - 7 compact cards */}
      <SimpleGrid cols={7} spacing="xs">
        {[
          { label: t(locale, 'totalRequests'), value: formatNumber(stats.total_requests || 0), color: '#1677ff' },
          { label: t(locale, 'successRate'), value: `${(stats.success_rate || 0).toFixed(1)}%`, color: successRateColor(stats.success_rate || 0) },
          { label: t(locale, 'avgLatency'), value: `${Math.round(stats.avg_latency_ms || 0)}ms`, color: '#fa8c16' },
          { label: t(locale, 'activePlatforms'), value: `${stats.active_platforms || 0}/${stats.total_platforms || 0}`, color: '#722ed1' },
          { label: t(locale, 'totalTokens'), value: formatTokenCount(totalTokens), color: '#13c2c2' },
          { label: t(locale, 'inputTokens'), value: formatTokenCount(stats.total_token_input || 0), color: '#1677ff' },
          { label: t(locale, 'outputTokens'), value: formatTokenCount(stats.total_token_output || 0), color: '#52c41a' },
        ].map((item, i) => (
          <ItemCard key={i}>
            <Text size="xs" c="dimmed" ta="center">{item.label}</Text>
            <Text fw={700} size="md" ta="center" style={{ color: item.color }}>{item.value}</Text>
          </ItemCard>
        ))}
      </SimpleGrid>

      {/* Proxy Status Section */}
      <div>
        <Group justify="space-between" mb="xs">
          <Text style={dashStyles.sectionTitle}>{t(locale, 'proxyStatus')}</Text>
          <Text style={dashStyles.sectionCount}>
            {stats.total_proxies || 0} {t(locale, 'total')}
          </Text>
        </Group>

        {proxies.length === 0 ? (
          <Center py="md">
            <Stack align="center" gap="xs">
              <ThemeIcon variant="light" size={36} radius="xl" color="gray">
                <IconApi size={18} />
              </ThemeIcon>
              <Text size="xs" c="dimmed">
                {t(locale, 'noProxies')}
              </Text>
            </Stack>
          </Center>
        ) : (
          <SimpleGrid cols={5} spacing="xs">
            {proxies.map((proxy) => (
              <ItemCard key={proxy.id}>
                <Group justify="space-between" mb={4}>
                  <Badge variant="light" color="violet" size="xs" style={{ fontFamily: 'monospace' }}>
                    {proxy.name}
                  </Badge>
                </Group>

                {proxy.stats && proxy.stats.total_requests > 0 ? (
                  <Group gap="xs" wrap="nowrap">
                    <Stack gap={0}>
                      <Text size="xs" c="dimmed">{t(locale, 'proxyRequests')}</Text>
                      <Text size="xs" fw={600}>{formatNumber(proxy.stats.total_requests)}</Text>
                    </Stack>
                    <Stack gap={0}>
                      <Text size="xs" c="dimmed">{t(locale, 'successRate')}</Text>
                      <Text size="xs" fw={600} c={successRateColor(proxy.stats.success_rate)}>
                        {proxy.stats.success_rate.toFixed(1)}%
                      </Text>
                    </Stack>
                    <Stack gap={0} align="flex-end">
                      <Text size="xs" c="dimmed">{t(locale, 'avgLatency')}</Text>
                      <Text size="xs" fw={600}>{Math.round(proxy.stats.avg_latency_ms)}ms</Text>
                    </Stack>
                  </Group>
                ) : (
                  <Text size="xs" c="dimmed">{t(locale, 'noRequests')}</Text>
                )}
              </ItemCard>
            ))}
          </SimpleGrid>
        )}
      </div>

      {/* Platform List Section */}
      <div>
        <Group justify="space-between" mb="xs">
          <Text style={dashStyles.sectionTitle}>{t(locale, 'platformList')}</Text>
          <Text style={dashStyles.sectionCount}>
            {stats.total_platforms || 0} {t(locale, 'total')}
          </Text>
        </Group>

        {platforms.length === 0 ? (
          <Center py="md">
            <Stack align="center" gap="xs">
              <ThemeIcon variant="light" size={36} radius="xl" color="gray">
                <IconServer size={18} />
              </ThemeIcon>
              <Text size="xs" c="dimmed">
                {t(locale, 'noPlatforms')}
              </Text>
            </Stack>
          </Center>
        ) : (
          <SimpleGrid cols={5} spacing="xs">
            {platforms.map((platform) => (
              <ItemCard key={platform.id}>
                <Group justify="space-between" mb={4}>
                  <Text fw={600} size="xs" truncate style={{ maxWidth: 100 }}>
                    {getPlatformDisplayName(platform.name)}
                  </Text>
                  <Badge variant="outline" size="xs" color="gray">
                    {platform.platform_type}
                  </Badge>
                </Group>

                {platform.stats && platform.stats.total_requests > 0 ? (
                  <Group gap="xs" wrap="nowrap">
                    <Stack gap={0}>
                      <Text size="xs" c="dimmed">{t(locale, 'proxyRequests')}</Text>
                      <Text size="xs" fw={600}>{formatNumber(platform.stats.total_requests)}</Text>
                    </Stack>
                    <Stack gap={0}>
                      <Text size="xs" c="dimmed">{t(locale, 'successRate')}</Text>
                      <Text size="xs" fw={600} c={successRateColor(platform.stats.success_rate)}>
                        {platform.stats.success_rate.toFixed(1)}%
                      </Text>
                    </Stack>
                    <Stack gap={0} align="flex-end">
                      <Text size="xs" c="dimmed">{t(locale, 'avgLatency')}</Text>
                      <Text size="xs" fw={600}>{Math.round(platform.stats.avg_latency_ms)}ms</Text>
                    </Stack>
                  </Group>
                ) : (
                  <Text size="xs" c="dimmed">{t(locale, 'noRequests')}</Text>
                )}
              </ItemCard>
            ))}
          </SimpleGrid>
        )}
      </div>

      {/* Model Request Stats Chart */}
      <ModelStatsSection locale={locale} />
    </Stack>
  )
}
