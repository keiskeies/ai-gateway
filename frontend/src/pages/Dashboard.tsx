import { useEffect, useState, useCallback } from 'react'
import {
  Card,
  Grid,
  Text,
  Badge,
  Group,
  Stack,
  RingProgress,
  ThemeIcon,
  Loader,
  ActionIcon,
  Tooltip,
  Center,
} from '@mantine/core'
import {
  IconBolt,
  IconCheck,
  IconClock,
  IconServer,
  IconCoin,
  IconArrowDown,
  IconArrowUp,
  IconApi,
  IconRefresh,
  IconCircleCheck,
} from '@tabler/icons-react'
import { getOverview, listProxies, listPlatforms, getProxyStats, getPlatformStats } from '../api'
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

function StatCard({
  icon,
  label,
  value,
  suffix,
  color,
}: {
  icon: React.ReactNode
  label: string
  value: string
  suffix?: string
  color?: string
}) {
  return (
    <Card shadow="sm" padding="lg" radius="md" withBorder>
      <Group justify="space-between" align="flex-start">
        <Stack gap={4}>
          <Text size="sm" c="dimmed">
            {label}
          </Text>
          <Group gap={4} align="baseline">
            <Text fw={700} size="xl" style={{ color }}>
              {value}
            </Text>
            {suffix && (
              <Text size="sm" c="dimmed">
                {suffix}
              </Text>
            )}
          </Group>
        </Stack>
        <ThemeIcon variant="light" size="lg" radius="md" color="gray">
          {icon}
        </ThemeIcon>
      </Group>
    </Card>
  )
}

function SuccessRing({ rate }: { rate: number }) {
  const color = rate > 90 ? 'green' : rate > 70 ? 'yellow' : 'red'
  return (
    <RingProgress
      size={44}
      thickness={4}
      roundCaps
      sections={[{ value: Math.min(rate, 100), color }]}
      label={
        <Center>
          <Text size="xs" fw={600}>
            {rate.toFixed(0)}
          </Text>
        </Center>
      }
    />
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

  const successRateColor = (rate: number) => {
    if (rate > 90) return 'green'
    if (rate > 70) return 'yellow'
    return 'red'
  }

  const totalTokens = (stats.total_token_input || 0) + (stats.total_token_output || 0)

  return (
    <Stack gap="lg">
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

      {/* Stats Cards Row 1 */}
      <Grid>
        <Grid.Col span={{ base: 12, xs: 6, sm: 3 }}>
          <StatCard
            icon={<IconBolt size={20} />}
            label={t(locale, 'totalRequests')}
            value={formatNumber(stats.total_requests || 0)}
            color="#2563eb"
          />
        </Grid.Col>
        <Grid.Col span={{ base: 12, xs: 6, sm: 3 }}>
          <StatCard
            icon={<IconCircleCheck size={20} />}
            label={t(locale, 'successRate')}
            value={(stats.success_rate || 0).toFixed(1)}
            suffix="%"
            color={(stats.success_rate || 0) > 90 ? '#16a34a' : '#dc2626'}
          />
        </Grid.Col>
        <Grid.Col span={{ base: 12, xs: 6, sm: 3 }}>
          <StatCard
            icon={<IconClock size={20} />}
            label={t(locale, 'avgLatency')}
            value={formatNumber(Math.round(stats.avg_latency_ms || 0))}
            suffix="ms"
            color="#d97706"
          />
        </Grid.Col>
        <Grid.Col span={{ base: 12, xs: 6, sm: 3 }}>
          <StatCard
            icon={<IconServer size={20} />}
            label={t(locale, 'activePlatforms')}
            value={String(stats.active_platforms || 0)}
            suffix={`/ ${stats.total_platforms || 0}`}
            color="#7c3aed"
          />
        </Grid.Col>
      </Grid>

      {/* Stats Cards Row 2 - Tokens */}
      <Grid>
        <Grid.Col span={{ base: 12, xs: 4 }}>
          <StatCard
            icon={<IconCoin size={20} />}
            label={t(locale, 'totalTokens')}
            value={formatTokenCount(totalTokens)}
            color="#0891b2"
          />
        </Grid.Col>
        <Grid.Col span={{ base: 12, xs: 4 }}>
          <StatCard
            icon={<IconArrowDown size={20} />}
            label={t(locale, 'inputTokens')}
            value={formatTokenCount(stats.total_token_input || 0)}
            color="#2563eb"
          />
        </Grid.Col>
        <Grid.Col span={{ base: 12, xs: 4 }}>
          <StatCard
            icon={<IconArrowUp size={20} />}
            label={t(locale, 'outputTokens')}
            value={formatTokenCount(stats.total_token_output || 0)}
            color="#16a34a"
          />
        </Grid.Col>
      </Grid>

      {/* Proxy Status Section */}
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Group justify="space-between" mb="md">
          <Text fw={600}>{t(locale, 'proxyStatus')}</Text>
          <Text size="sm" c="dimmed">
            {stats.total_proxies || 0} {t(locale, 'total')}
          </Text>
        </Group>

        {proxies.length === 0 ? (
          <Center py="xl">
            <Stack align="center" gap="xs">
              <ThemeIcon variant="light" size={48} radius="xl" color="gray">
                <IconApi size={24} />
              </ThemeIcon>
              <Text size="sm" c="dimmed">
                {t(locale, 'noProxies')}
              </Text>
            </Stack>
          </Center>
        ) : (
          <Grid>
            {proxies.map((proxy) => (
              <Grid.Col key={proxy.id} span={{ base: 12, sm: 6, md: 4 }}>
                <Card shadow="xs" padding="md" radius="sm" withBorder>
                  <Group justify="space-between" mb="sm">
                    <Badge variant="light" color="violet" size="sm" style={{ fontFamily: 'monospace' }}>
                      {proxy.name}
                    </Badge>
                    <Badge
                      variant="light"
                      color="green"
                      size="sm"
                      leftSection={
                        <ThemeIcon size={14} variant="transparent" color="green" style={{ minWidth: 14 }}>
                          <IconCheck size={10} />
                        </ThemeIcon>
                      }
                    >
                      Ready
                    </Badge>
                  </Group>

                  {proxy.stats && proxy.stats.total_requests > 0 ? (
                    <Group justify="space-between" wrap="nowrap">
                      <Stack gap={2}>
                        <Text size="xs" c="dimmed">
                          {t(locale, 'proxyRequests')}
                        </Text>
                        <Text size="sm" fw={600}>
                          {formatNumber(proxy.stats.total_requests)}
                        </Text>
                      </Stack>
                      <Stack gap={2} align="center">
                        <Text size="xs" c="dimmed">
                          {t(locale, 'successRate')}
                        </Text>
                        <SuccessRing rate={proxy.stats.success_rate} />
                      </Stack>
                      <Stack gap={2} align="flex-end">
                        <Text size="xs" c="dimmed">
                          {t(locale, 'avgLatency')}
                        </Text>
                        <Text size="sm" fw={600}>
                          {Math.round(proxy.stats.avg_latency_ms)}ms
                        </Text>
                      </Stack>
                    </Group>
                  ) : (
                    <Text size="xs" c="dimmed" mt={4}>
                      {t(locale, 'noRequests')}
                    </Text>
                  )}
                </Card>
              </Grid.Col>
            ))}
          </Grid>
        )}
      </Card>

      {/* Platform List Section */}
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Text fw={600} mb="md">
          {t(locale, 'platformList')}
        </Text>

        {platforms.length === 0 ? (
          <Center py="xl">
            <Stack align="center" gap="xs">
              <ThemeIcon variant="light" size={48} radius="xl" color="gray">
                <IconServer size={24} />
              </ThemeIcon>
              <Text size="sm" c="dimmed">
                {t(locale, 'noPlatforms')}
              </Text>
            </Stack>
          </Center>
        ) : (
          <Grid>
            {platforms.map((platform) => (
              <Grid.Col key={platform.id} span={{ base: 12, sm: 6, md: 3 }}>
                <Card shadow="xs" padding="md" radius="sm" withBorder>
                  <Group justify="space-between" mb={4}>
                    <Group gap="sm" style={{ flex: 1, minWidth: 0 }}>
                      <Text fw={600} size="sm" truncate>
                        {getPlatformDisplayName(platform.name)}
                      </Text>
                      <Badge variant="outline" size="sm" color="gray" style={{ flexShrink: 0 }}>
                        {platform.platform_type}
                      </Badge>
                    </Group>
                    {platform.stats && platform.stats.total_requests > 0 && (
                      <SuccessRing rate={platform.stats.success_rate} />
                    )}
                  </Group>

                  <Text size="xs" c="dimmed" truncate>
                    {platform.base_url}
                  </Text>

                  {platform.stats && platform.stats.total_requests > 0 ? (
                    <Group gap="xs" mt="sm">
                      <Stack gap={0}>
                        <Text size="xs" c="dimmed">
                          {t(locale, 'proxyRequests')}
                        </Text>
                        <Text size="sm" fw={600}>
                          {formatNumber(platform.stats.total_requests)}
                        </Text>
                      </Stack>
                      <Stack gap={0}>
                        <Text size="xs" c="dimmed">
                          {t(locale, 'successRate')}
                        </Text>
                        <Text size="sm" fw={600} c={successRateColor(platform.stats.success_rate)}>
                          {platform.stats.success_rate.toFixed(1)}%
                        </Text>
                      </Stack>
                      <Stack gap={0}>
                        <Text size="xs" c="dimmed">
                          {t(locale, 'avgLatency')}
                        </Text>
                        <Text size="sm" fw={600}>
                          {Math.round(platform.stats.avg_latency_ms)}ms
                        </Text>
                      </Stack>
                      <Stack gap={0}>
                        <Text size="xs" c="dimmed">
                          {t(locale, 'proxyTokens')}
                        </Text>
                        <Text size="sm" fw={600}>
                          {formatTokenCount((platform.stats.total_token_input || 0) + (platform.stats.total_token_output || 0))}
                        </Text>
                      </Stack>
                    </Group>
                  ) : (
                    <Text size="xs" c="dimmed" mt={4}>
                      {t(locale, 'noRequests')}
                    </Text>
                  )}
                </Card>
              </Grid.Col>
            ))}
          </Grid>
        )}
      </Card>
    </Stack>
  )
}
