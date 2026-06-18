import { useEffect, useState, useCallback } from 'react'
import { Button, Tag, Tooltip, Typography } from 'antd'
import {
  CloudServerOutlined, ApiOutlined,
  CheckCircleOutlined, SafetyCertificateOutlined,
  ClockCircleOutlined, ReloadOutlined, DollarOutlined, ArrowUpOutlined, ArrowDownOutlined,
  ThunderboltOutlined,
} from '@ant-design/icons'
import { getOverview, listProxies, listPlatforms, getProxyStats, getPlatformStats } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { getPresetName, platformPresets } from '../presets'
import PageLayout from '../components/PageLayout'
import StatCard from '../components/StatCard'
import SectionCard from '../components/SectionCard'
import EmptyState from '../components/EmptyState'

const { Text } = Typography

interface ProxyWithStats {
  id: string
  name: string
  stats?: {
    total_requests: number
    success_rate: number
    avg_latency_ms: number
    requests_today: number
    total_token_input: number
    total_token_output: number
  }
}

interface PlatformWithStats {
  id: string
  name: string
  type: string
  base_url: string
  stats?: {
    total_requests: number
    success_rate: number
    avg_latency_ms: number
    total_token_input: number
    total_token_output: number
  }
}

const StatItem = ({ label, value, color }: { label: string; value: string; color?: string }) => (
  <div>
    <div className="stat-item-label">{label}</div>
    <div className="stat-item-value" style={{ color }}>{value}</div>
  </div>
)

export default function Dashboard() {
  const [stats, setStats] = useState<any>({})
  const [proxies, setProxies] = useState<ProxyWithStats[]>([])
  const [platforms, setPlatforms] = useState<PlatformWithStats[]>([])
  const [loading, setLoading] = useState(false)
  const { locale, isDark } = useAppContext()

  const loadData = useCallback(async () => {
    setLoading(true)
    try {
      const [overview, proxyList, platformList] = await Promise.all([
        getOverview().catch(() => ({})),
        listProxies().catch(() => []),
        listPlatforms().catch(() => []),
      ])
      setStats(overview)

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
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { loadData() }, [loadData])

  const getPlatformDisplayName = (name: string) => {
    const preset = platformPresets.find(p => p.name === name)
    return preset ? getPresetName(preset, locale) : name
  }

  const formatTokenCount = (count: number) => {
    if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`
    if (count >= 1_000) return `${(count / 1_000).toFixed(1)}K`
    return count.toString()
  }

  const successColor = (stats.success_rate || 0) > 90 ? 'var(--ant-color-success)' : 'var(--ant-color-error)'

  return (
    <PageLayout
      title={t(locale, 'dashboard')}
      extra={
        <Tooltip title={t(locale, 'refresh')}>
          <Button icon={<ReloadOutlined />} onClick={loadData} loading={loading} type="text" size="small" />
        </Tooltip>
      }
    >
      <div className="stat-cards-row">
        <StatCard
          title={t(locale, 'totalRequests')}
          value={stats.total_requests || 0}
          prefix={<ThunderboltOutlined style={{ color: '#2563eb' }} />}
        />
        <StatCard
          title={t(locale, 'successRate')}
          value={stats.success_rate || 0}
          suffix="%"
          precision={1}
          prefix={<SafetyCertificateOutlined style={{ color: successColor }} />}
          valueStyle={{ color: successColor }}
        />
        <StatCard
          title={t(locale, 'avgLatency')}
          value={Math.round(stats.avg_latency_ms || 0)}
          suffix="ms"
          prefix={<ClockCircleOutlined style={{ color: '#d97706' }} />}
        />
        <StatCard
          title={t(locale, 'activePlatforms')}
          value={stats.active_platforms || 0}
          suffix={`/ ${stats.total_platforms || 0}`}
          prefix={<CloudServerOutlined style={{ color: '#7c3aed' }} />}
        />
      </div>

      <div className="stat-cards-row cols-3">
        <StatCard
          title={t(locale, 'totalTokens')}
          value={formatTokenCount((stats.total_token_input || 0) + (stats.total_token_output || 0))}
          prefix={<DollarOutlined style={{ color: '#0891b2' }} />}
        />
        <StatCard
          title={t(locale, 'inputTokens')}
          value={formatTokenCount(stats.total_token_input || 0)}
          prefix={<ArrowDownOutlined style={{ color: '#2563eb' }} />}
        />
        <StatCard
          title={t(locale, 'outputTokens')}
          value={formatTokenCount(stats.total_token_output || 0)}
          prefix={<ArrowUpOutlined style={{ color: '#16a34a' }} />}
        />
      </div>

      <SectionCard
        title={t(locale, 'proxyStatus')}
        extra={<Text type="secondary">{stats.total_proxies || 0} {t(locale, 'total')}</Text>}
      >
        {proxies.length === 0 ? (
          <EmptyState icon={<ApiOutlined />} description={t(locale, 'noProxies')} />
        ) : (
          <div className="stat-cards-row">
            {proxies.map((p) => (
              <div key={p.id} className="card-inner">
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <Tag color="purple" className="tag-mono" style={{ padding: '2px 10px' }}>{p.name}</Tag>
                  <Tag icon={<CheckCircleOutlined />} color="processing">Ready</Tag>
                </div>
                {p.stats && p.stats.total_requests > 0 ? (
                  <div className="stat-items">
                    <StatItem label={t(locale, 'proxyRequests')} value={String(p.stats.total_requests)} />
                    <StatItem label={t(locale, 'successRate')} value={`${p.stats.success_rate.toFixed(1)}%`} color={p.stats.success_rate > 90 ? '#16a34a' : '#dc2626'} />
                    <StatItem label={t(locale, 'avgLatency')} value={`${p.stats.avg_latency_ms.toFixed(0)}ms`} />
                    <StatItem label={t(locale, 'proxyTokens')} value={formatTokenCount(p.stats.total_token_input + p.stats.total_token_output)} />
                  </div>
                ) : (
                  <Text type="secondary" style={{ fontSize: 12, marginTop: 8, display: 'block' }}>{t(locale, 'noRequests')}</Text>
                )}
              </div>
            ))}
          </div>
        )}
      </SectionCard>

      <SectionCard title={t(locale, 'platformList')}>
        {platforms.length === 0 ? (
          <EmptyState icon={<CloudServerOutlined />} description={t(locale, 'noPlatforms')} />
        ) : (
          <div className="stat-cards-row">
            {platforms.map((p) => (
              <div key={p.id} className="card-inner hoverable">
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <Text strong>{getPlatformDisplayName(p.name)}</Text>
                  <Tag>{p.type}</Tag>
                </div>
                <Text type="secondary" style={{ fontSize: 12, marginTop: 4, display: 'block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {p.base_url}
                </Text>
                {p.stats && p.stats.total_requests > 0 ? (
                  <div className="stat-items">
                    <StatItem label={t(locale, 'proxyRequests')} value={String(p.stats.total_requests)} />
                    <StatItem label={t(locale, 'successRate')} value={`${p.stats.success_rate.toFixed(1)}%`} color={p.stats.success_rate > 90 ? '#16a34a' : '#dc2626'} />
                    <StatItem label={t(locale, 'avgLatency')} value={`${p.stats.avg_latency_ms.toFixed(0)}ms`} />
                    <StatItem label={t(locale, 'proxyTokens')} value={formatTokenCount(p.stats.total_token_input + p.stats.total_token_output)} />
                  </div>
                ) : (
                  <Text type="secondary" style={{ fontSize: 12, marginTop: 6 }}>{t(locale, 'noRequests')}</Text>
                )}
              </div>
            ))}
          </div>
        )}
      </SectionCard>
    </PageLayout>
  )
}
