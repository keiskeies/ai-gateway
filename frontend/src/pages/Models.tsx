import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Button, Modal, Table, TextInput, Tooltip } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconArrowRight, IconPlus, IconRefresh, IconSearch } from '@tabler/icons-react'
import { useNavigate } from 'react-router-dom'
import { createProxy, getAllProxyStats, listAllRoutes, listProxies } from '../api'
import { rangeToParams, useAppState } from '../app/AppState'
import { usePollInterval } from '../app/LiveContext'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { PageHeader } from '../components/ui/PageHeader'
import { EmptyState, ErrorState, PageSkeleton } from '../components/ui/States'
import { StatusPill } from '../components/ui/StatusPill'
import { formatCost, formatDuration, formatNumber, successRateTone } from '../lib/format'
import type { ProxyRow, ProxyStats, RouteRow } from '../types/stats'

interface ModelListItem extends ProxyRow {
  routes: RouteRow[]
  stats: ProxyStats | null
}

const STRATEGY_LABELS: Record<string, { zh: string; en: string }> = {
  RoundRobin: { zh: '轮询', en: 'Round robin' },
  WeightedRandom: { zh: '加权随机', en: 'Weighted' },
  LeastConnections: { zh: '最少连接', en: 'Least connections' },
  Priority: { zh: '优先级', en: 'Priority' },
  LatencyBased: { zh: '延迟优先', en: 'Latency based' },
}

function strategyLabel(value: string | undefined, locale: 'zh' | 'en'): string {
  if (!value) return '—'
  return STRATEGY_LABELS[value]?.[locale] || value
}

export default function Models() {
  const navigate = useNavigate()
  const { locale } = useAppContext()
  const { range } = useAppState()
  const pollInterval = usePollInterval()
  const [search, setSearch] = useState('')
  const [createOpen, setCreateOpen] = useState(false)
  const [name, setName] = useState('')
  const [saving, setSaving] = useState(false)

  const { data, isPending, isError, refetch, dataUpdatedAt } = useQuery({
    queryKey: ['models', range],
    queryFn: async (): Promise<ModelListItem[]> => {
      const { from, to } = rangeToParams(range)
      const [proxies, routes, statsRows] = await Promise.all([
        listProxies() as Promise<ProxyRow[]>,
        listAllRoutes().catch(() => []) as Promise<RouteRow[]>,
        getAllProxyStats({ from, to }).catch(() => []) as Promise<ProxyStats[]>,
      ])
      const routesByProxy = new Map<string, RouteRow[]>()
      for (const route of routes) {
        if (!route.proxy_id) continue
        const bucket = routesByProxy.get(route.proxy_id)
        if (bucket) bucket.push(route)
        else routesByProxy.set(route.proxy_id, [route])
      }
      const statsByProxy = new Map<string, ProxyStats>()
      for (const row of statsRows ?? []) {
        if (row.proxy_id) statsByProxy.set(row.proxy_id, row)
      }
      return proxies.map(proxy => ({
        ...proxy,
        routes: routesByProxy.get(proxy.id) ?? [],
        stats: statsByProxy.get(proxy.id) ?? null,
      }))
    },
    refetchInterval: pollInterval,
  })

  const items = useMemo(() => {
    const query = search.trim().toLowerCase()
    const list = data ?? []
    return query ? list.filter(item => item.name.toLowerCase().includes(query)) : list
  }, [data, search])

  const degraded = (data ?? []).filter(item => {
    const stats = item.stats
    if (!stats || stats.total_requests === 0) return false
    return stats.success_rate < 99
  }).length

  const create = async () => {
    if (!name.trim()) return
    setSaving(true)
    try {
      const proxy = await createProxy({ name: name.trim() }) as ProxyRow
      setCreateOpen(false)
      setName('')
      await refetch()
      navigate(`/models/${proxy.id}`)
    } catch {
      notifications.show({ message: t(locale, 'createFailed'), color: 'red' })
    } finally {
      setSaving(false)
    }
  }

  if (isPending) return <PageSkeleton rows={6} />
  if (isError && !data) return <ErrorState onRetry={() => void refetch()} />

  const lastSync = dataUpdatedAt
    ? new Date(dataUpdatedAt).toLocaleTimeString(locale === 'zh' ? 'zh-CN' : 'en-US', { hour12: false })
    : '—'

  return (
    <div className="console-page">
      <PageHeader
        kicker={<>{t(locale, 'kickerConfigure')} <span className="console-kicker-sep" /> {t(locale, 'spineVirtualModels')}</>}
        title={t(locale, 'proxies')}
        subtitle={`${data?.length ?? 0} ${t(locale, 'total')} · ${lastSync}`}
        actions={
          <>
            <Button size="xs" variant="default" leftSection={<IconRefresh size={14} />} onClick={() => void refetch()}>
              {t(locale, 'refresh')}
            </Button>
            <Button size="xs" leftSection={<IconPlus size={14} />} onClick={() => setCreateOpen(true)}>
              {t(locale, 'newProxy')}
            </Button>
          </>
        }
      />

      <div className="console-status-strip">
        <span className={`console-dot ${degraded > 0 ? 'warn' : 'ok'}`} />
        <strong>{t(locale, 'proxies')}</strong>
        <span>{data?.length ?? 0} {t(locale, 'total')}</span>
        {degraded > 0 && <span className="console-issue warn"><span className="console-dot warn" />{degraded} degraded</span>}
        <span className="console-status-right">
          <TextInput
            size="xs"
            leftSection={<IconSearch size={13} />}
            placeholder={t(locale, 'search')}
            value={search}
            onChange={event => setSearch(event.currentTarget.value)}
            w={220}
          />
        </span>
      </div>

      <section className="console-panel list-panel">
        <div className="console-panel-head">
          <div className="console-panel-title">{t(locale, 'proxies')}</div>
          <span className="console-panel-sub">{range.toUpperCase()}</span>
        </div>
        {items.length > 0 ? (
          <div className="console-table-wrap">
            <Table className="console-table">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>{t(locale, 'modelId')}</Table.Th>
                  <Table.Th>{t(locale, 'status')}</Table.Th>
                  <Table.Th>{t(locale, 'backends')}</Table.Th>
                  <Table.Th>{t(locale, 'strategy')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'reqCount')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'successCountShort')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'avgShort')}</Table.Th>
                  <Table.Th className="num">
                    <Tooltip label={t(locale, 'costHint')}><span>{t(locale, 'cost')}</span></Tooltip>
                  </Table.Th>
                  <Table.Th />
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {items.map(item => {
                  const stats = item.stats
                  const rate = stats?.success_rate ?? 0
                  const tone = successRateTone(rate, (stats?.total_requests ?? 0) > 0)
                  const backendCount = item.routes.reduce((sum, route) => sum + (route.backends?.length ?? 0), 0)
                  return (
                    <Table.Tr key={item.id} onClick={() => navigate(`/models/${item.id}`)}>
                      <Table.Td><span className="console-strong console-mono">{item.name}</span></Table.Td>
                      <Table.Td>
                        <StatusPill tone={tone}>
                          {t(locale, tone === 'ok' ? 'healthHealthy' : tone === 'warn' ? 'healthDegraded' : tone === 'err' ? 'healthFailing' : 'healthIdle')}
                        </StatusPill>
                      </Table.Td>
                      <Table.Td>{backendCount}</Table.Td>
                      <Table.Td>{strategyLabel(item.routes[0]?.lb_strategy, locale)}</Table.Td>
                      <Table.Td className="num">{formatNumber(stats?.total_requests ?? 0)}</Table.Td>
                      <Table.Td className="num">{stats?.total_requests ? `${rate.toFixed(1)}%` : '—'}</Table.Td>
                      <Table.Td className="num">{stats?.total_requests ? formatDuration(stats.avg_latency_ms) : '—'}</Table.Td>
                      {/* 未定价的模型整列留 —，不要显示 0：0 会被读成"这个模型免费" */}
                      <Table.Td className="num">{stats?.total_cost == null ? <span className="muted">—</span> : formatCost(stats.total_cost)}</Table.Td>
                      <Table.Td className="num"><IconArrowRight size={14} /></Table.Td>
                    </Table.Tr>
                  )
                })}
              </Table.Tbody>
            </Table>
          </div>
        ) : (
          <EmptyState
            title={search ? t(locale, 'noMatchingRequests') : t(locale, 'noProxies')}
            description={search ? t(locale, 'clearFilters') : t(locale, 'proxyDesc')}
            action={search ? (
              <Button size="xs" variant="default" onClick={() => setSearch('')}>{t(locale, 'clearFilters')}</Button>
            ) : (
              <Button size="xs" leftSection={<IconPlus size={14} />} onClick={() => setCreateOpen(true)}>{t(locale, 'newProxy')}</Button>
            )}
          />
        )}
      </section>

      <Modal
        opened={createOpen}
        onClose={() => setCreateOpen(false)}
        title={t(locale, 'newProxy')}
        size="sm"
        closeOnClickOutside={false}
      >
        <TextInput
          label={t(locale, 'proxyName')}
          placeholder="qwen3.8-flash-next"
          description={t(locale, 'virtualModelPlaceholder')}
          value={name}
          onChange={event => setName(event.currentTarget.value)}
          data-autofocus
        />
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button variant="default" onClick={() => setCreateOpen(false)}>{t(locale, 'cancel')}</Button>
          <Button loading={saving} disabled={!name.trim()} onClick={() => void create()}>{t(locale, 'create')}</Button>
        </div>
      </Modal>
    </div>
  )
}
