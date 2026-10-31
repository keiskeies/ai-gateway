import { useEffect, useMemo, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ActionIcon, Button, Group, Modal, Table, TextInput, Tooltip } from '@mantine/core'
import { notify } from '../lib/notify'
import {
  IconArrowRight,
  IconAlarm,
  IconGift,
  IconPlugConnected,
  IconPlus,
  IconRefresh,
  IconSearch,
  IconTrash,
} from '@tabler/icons-react'
import { useNavigate } from 'react-router-dom'
import {
  deletePlatform,
  getAllPlatformHealth,
  getAllPlatformStats,
  getAuthStatus,
  getCheckin,
  getPlatformCredits,
  listAllRoutes,
  listModels,
  listPlatforms,
  runAllCheckin,
  runCheckin,
  startPlatformCheckinLogin,
  waitForCheckinAttempt,
} from '../api'
import { rangeToParams, useAppState } from '../app/AppState'
import { usePollInterval } from '../app/LiveContext'
import { useAppContext } from '../ThemeContext'
import { t, type Locale } from '../i18n'
import { PageHeader } from '../components/ui/PageHeader'
import { EmptyState, ErrorState, PageSkeleton } from '../components/ui/States'
import { StatusPill } from '../components/ui/StatusPill'
import { formatDuration, formatLogTime, formatNumber, successRateTone } from '../lib/format'
import { platformTypeLabel, probePlatformConfig, type PlatformTestResult } from '../lib/platform'
import { ProviderCreateModal } from '../components/ProviderCreateModal'
import { CheckinTasksModal, type CheckinAccountOption } from '../components/CheckinTasksModal'
import { getPresetName, platformPresets } from '../presets'
import type { PlatformCreditUsage, PlatformDetail, PlatformHealthSummary, PlatformStats, RouteRow } from '../types/stats'
import type { CheckinStatus } from '../types/checkin'

interface ModelRow {
  id: string
  platform_id: string
  catalog_key?: string | null
  model_id: string
}

interface ProviderRow {
  platform: PlatformDetail
  modelCount: number
  refModels: number
  refBackends: number
  stats: PlatformStats | null
  health: PlatformHealthSummary | null
  credit: PlatformCreditUsage | null
}

const CHECKIN_TONE: Record<CheckinStatus, string> = {
  new: 'signal',
  ok: 'ok',
  waiting: 'warn',
  expired: 'err',
  error: 'err',
}

function errorMessage(error: unknown, locale: Locale): string {
  const anyError = error as { response?: { data?: { error?: { message?: string } } }; message?: string }
  return anyError?.response?.data?.error?.message || anyError?.message || t(locale, 'requestFailed')
}


export default function Platforms() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { locale } = useAppContext()
  const { range } = useAppState()
  const pollInterval = usePollInterval()
  const [search, setSearch] = useState('')
  const [createOpen, setCreateOpen] = useState(false)
  const [saving, setSaving] = useState(false)
  const [testingId, setTestingId] = useState<string | null>(null)
  // 行内测试的即时结果：后端已落健康历史，这里只用于刷新前的即时反馈
  const [tests, setTests] = useState<Record<string, PlatformTestResult>>({})
  const [deleteTarget, setDeleteTarget] = useState<ProviderRow | null>(null)
  const [localConsole, setLocalConsole] = useState(false)
  const [checkinLogin, setCheckinLogin] = useState(false)
  const [loginBusyId, setLoginBusyId] = useState<string | null>(null)
  const [claimBusyId, setClaimBusyId] = useState<string | null>(null)
  const [bulkBusy, setBulkBusy] = useState(false)
  const [bulkNote, setBulkNote] = useState<string | null>(null)
  const [tasksOpen, setTasksOpen] = useState(false)

  useEffect(() => {
    getAuthStatus()
      .then(status => {
        setLocalConsole(Boolean(status.local_console))
        setCheckinLogin(Boolean(status.checkin_login))
      })
      .catch(() => {})
  }, [])

  // 登录/领取只在本机桌面端存在：远程控制台连请求都不发，避免每轮轮询刷 403
  const checkinQuery = useQuery({
    queryKey: ['checkin'],
    queryFn: getCheckin,
    enabled: localConsole,
  })

  const providersQuery = useQuery({
    queryKey: ['platforms', range],
    queryFn: async (): Promise<ProviderRow[]> => {
      const { from, to } = rangeToParams(range)
      const [platforms, models, routes, statsRows, healthRows, creditRows] = await Promise.all([
        listPlatforms() as Promise<PlatformDetail[]>,
        listModels().catch(() => []) as Promise<ModelRow[]>,
        listAllRoutes().catch(() => []) as Promise<RouteRow[]>,
        getAllPlatformStats({ from, to }).catch(() => []) as Promise<PlatformStats[]>,
        getAllPlatformHealth({ from, to }).catch(() => []) as Promise<PlatformHealthSummary[]>,
        getPlatformCredits().catch(() => []) as Promise<PlatformCreditUsage[]>,
      ])

      const modelCount = new Map<string, number>()
      for (const model of models ?? []) {
        // 同 URL 的账号共享目录；按 catalog_key 计数，第二个账号不会显示 0。
        if (!model.catalog_key) continue
        modelCount.set(model.catalog_key, (modelCount.get(model.catalog_key) ?? 0) + 1)
      }

      const refs = new Map<string, { proxies: Set<string>; backends: number }>()
      for (const route of routes ?? []) {
        if (!route.proxy_id) continue
        for (const backend of route.backends ?? []) {
          if (!backend.platform_id) continue
          const entry = refs.get(backend.platform_id) ?? { proxies: new Set<string>(), backends: 0 }
          entry.proxies.add(route.proxy_id)
          entry.backends += 1
          refs.set(backend.platform_id, entry)
        }
      }

      const statsByPlatform = new Map<string, PlatformStats>()
      for (const row of statsRows ?? []) {
        if (row.platform_id) statsByPlatform.set(row.platform_id, row)
      }

      const healthByPlatform = new Map<string, PlatformHealthSummary>()
      for (const row of healthRows ?? []) {
        if (row.platform_id) healthByPlatform.set(row.platform_id, row)
      }

      const creditByPlatform = new Map<string, PlatformCreditUsage>()
      for (const row of creditRows ?? []) {
        if (row.platform_id) creditByPlatform.set(row.platform_id, row)
      }

      return (platforms ?? []).map(platform => {
        const ref = refs.get(platform.id)
        return {
          platform,
          modelCount: modelCount.get(platform.catalog_key ?? '') ?? 0,
          refModels: ref?.proxies.size ?? 0,
          refBackends: ref?.backends ?? 0,
          stats: statsByPlatform.get(platform.id) ?? null,
          health: healthByPlatform.get(platform.id) ?? null,
          credit: creditByPlatform.get(platform.id) ?? null,
        }
      })
    },
    refetchInterval: pollInterval,
  })

  const items = useMemo(() => {
    const query = search.trim().toLowerCase()
    const list = providersQuery.data ?? []
    if (!query) return list
    return list.filter(item => {
      const haystack = [
        item.platform.name,
        item.platform.base_url,
        platformTypeLabel(item.platform.type),
      ].join(' ').toLowerCase()
      return haystack.includes(query)
    })
  }, [providersQuery.data, search])

  const degraded = (providersQuery.data ?? []).filter(item => {
    const stats = item.stats
    if (!stats || stats.total_requests === 0) return false
    return stats.success_rate < 99
  }).length

  const runTest = async (row: ProviderRow) => {
    setTestingId(row.platform.id)
    const result = await probePlatformConfig(locale, {
      type: row.platform.type,
      base_url: row.platform.base_url,
      platform_id: row.platform.id,
    })
    setTests(prev => ({ ...prev, [row.platform.id]: result }))
    notify(locale, { title: row.platform.name, message: result.message, color: result.ok ? 'green' : 'red' })
    setTestingId(null)
    // 检测已落库，重取列表让 CHECK 列回到持久数据（本次会话结果继续覆盖，避免闪烁）
    void queryClient.invalidateQueries({ queryKey: ['platforms'] })
  }

  const remove = async () => {
    if (!deleteTarget) return
    setSaving(true)
    try {
      await deletePlatform(deleteTarget.platform.id)
      notify(locale, { message: t(locale, 'deleteSuccess'), color: 'green' })
      setDeleteTarget(null)
      await queryClient.invalidateQueries({ queryKey: ['platforms'] })
      await queryClient.invalidateQueries({ queryKey: ['models'] })
      await queryClient.invalidateQueries({ queryKey: ['overview'] })
      await queryClient.invalidateQueries({ queryKey: ['checkin'] })
    } catch (error: any) {
      notify(locale, {
        message: error?.response?.data?.error?.message || t(locale, 'loadFailed'),
        color: 'red',
      })
    } finally {
      setSaving(false)
    }
  }

  const showCheckin = localConsole && checkinLogin
  const checkinByPlatform = useMemo(
    () => new Map((checkinQuery.data?.accounts ?? []).map(account => [account.platform_id, account])),
    [checkinQuery.data],
  )
  const capabilityFor = (baseUrl: string) => {
    const normalized = baseUrl.trim().toLowerCase()
    return checkinQuery.data?.platforms.find(cap =>
      normalized.startsWith(cap.base_url_prefix.toLowerCase()),
    )
  }
  const supportedRows = (providersQuery.data ?? []).filter(item =>
    capabilityFor(item.platform.base_url),
  )
  // 定时任务的候选账号：受支持的平台行全列出来，没登录的禁用——否则用户会找不到自己的账号
  const taskOptions: CheckinAccountOption[] = supportedRows.map(row => {
    const account = checkinByPlatform.get(row.platform.id)
    return {
      value: row.platform.id,
      label: account ? `${row.platform.name} · ${account.identity || account.label}` : row.platform.name,
      disabled: !account,
    }
  })

  const claimPlatform = async (row: ProviderRow) => {
    setClaimBusyId(row.platform.id)
    try {
      const view = await runCheckin(row.platform.id)
      notify(locale, {
        color: view.status === 'ok' ? 'teal' : view.status === 'waiting' ? 'yellow' : 'red',
        message: view.message || t(locale, 'checkinRunDone'),
        autoClose: 6000,
      })
      await checkinQuery.refetch()
      // 积分列用的是领取时同步的余额，必须一起刷新
      await queryClient.invalidateQueries({ queryKey: ['platforms'] })
    } catch (error) {
      notify(locale, { color: 'red', message: errorMessage(error, locale) })
    } finally {
      setClaimBusyId(null)
    }
  }

  const loginPlatform = async (row: ProviderRow) => {
    setLoginBusyId(row.platform.id)
    try {
      const { attempt_id } = await startPlatformCheckinLogin(row.platform.id)
      notify(locale, { message: t(locale, 'checkinLoginWaiting'), autoClose: 5000 })
      const attempt = await waitForCheckinAttempt(attempt_id)
      if (!attempt) {
        notify(locale, { color: 'yellow', message: t(locale, 'checkinLoginTimeout'), autoClose: 8000 })
        return
      }
      if (attempt.status !== 'ok') {
        notify(locale, { color: 'red', message: attempt.message || t(locale, 'checkinLoginFailed') })
        return
      }
      await checkinQuery.refetch()
      const view = await runCheckin(row.platform.id)
      notify(locale, {
        color: view.status === 'ok' ? 'teal' : view.status === 'waiting' ? 'yellow' : 'red',
        message: view.message,
        autoClose: 6000,
      })
      await checkinQuery.refetch()
      await queryClient.invalidateQueries({ queryKey: ['platforms'] })
    } catch (error) {
      notify(locale, { color: 'red', message: errorMessage(error, locale) })
    } finally {
      setLoginBusyId(null)
    }
  }

  // 一键：先把已登录的都领一遍，再给还没登录的平台逐个弹登录窗口（一次一个）
  const oneClick = async () => {
    setBulkBusy(true)
    try {
      setBulkNote(t(locale, 'checkinBulkClaiming'))
      await runAllCheckin()
      const missing = supportedRows.filter(row => !checkinByPlatform.has(row.platform.id))
      let logged = 0
      for (const row of missing) {
        setBulkNote(t(locale, 'checkinLoginWaiting'))
        const { attempt_id } = await startPlatformCheckinLogin(row.platform.id)
        const attempt = await waitForCheckinAttempt(attempt_id)
        if (attempt?.status === 'ok') {
          logged += 1
          await runCheckin(row.platform.id)
        } else if (attempt?.status === 'error') {
          notify(locale, { color: 'red', message: attempt.message || t(locale, 'checkinLoginFailed') })
        }
      }
      await Promise.all([
        checkinQuery.refetch(),
        queryClient.invalidateQueries({ queryKey: ['platforms'] }),
      ])
      notify(locale, {
        color: 'teal',
        message: t(locale, 'checkinBulkDone', String(logged)),
        autoClose: 6000,
      })
    } catch (error) {
      notify(locale, { color: 'red', message: errorMessage(error, locale) })
    } finally {
      setBulkBusy(false)
      setBulkNote(null)
    }
  }

  if (providersQuery.isPending) return <PageSkeleton rows={6} />
  if (providersQuery.isError && !providersQuery.data) {
    return <ErrorState onRetry={() => void providersQuery.refetch()} />
  }

  const total = providersQuery.data?.length ?? 0
  const lastSync = providersQuery.dataUpdatedAt
    ? new Date(providersQuery.dataUpdatedAt).toLocaleTimeString(locale === 'zh' ? 'zh-CN' : 'en-US', { hour12: false })
    : '—'

  return (
    <div className="console-page">
      <PageHeader
        kicker={<>{t(locale, 'kickerConfigure')} <span className="console-kicker-sep" /> {t(locale, 'kickerProviders')}</>}
        title={t(locale, 'platforms')}
        subtitle={`${total} ${t(locale, 'total')} · ${lastSync}`}
        actions={
          <>
            {showCheckin && (
              <Button
                size="xs"
                variant="default"
                leftSection={<IconGift size={14} />}
                loading={bulkBusy}
                onClick={() => void oneClick()}
              >
                {t(locale, 'checkinOneClick')}
              </Button>
            )}
            {showCheckin && (
              <Button
                size="xs"
                variant="default"
                leftSection={<IconAlarm size={14} />}
                onClick={() => setTasksOpen(true)}
              >
                {t(locale, 'checkinTasks')}
              </Button>
            )}
            <Button size="xs" variant="default" leftSection={<IconRefresh size={14} />} onClick={() => void providersQuery.refetch()}>
              {t(locale, 'refresh')}
            </Button>
            <Button size="xs" leftSection={<IconPlus size={14} />} onClick={() => setCreateOpen(true)}>
              {t(locale, 'addPlatform')}
            </Button>
          </>
        }
      />

      <div className="console-status-strip">
        <span className={`console-dot ${degraded > 0 ? 'warn' : 'ok'}`} />
        <strong>{t(locale, 'platforms')}</strong>
        <span>{total} {t(locale, 'total')}</span>
        {degraded > 0 && (
          <span className="console-issue warn"><span className="console-dot warn" />{degraded} degraded</span>
        )}
        {bulkNote && (
          <span className="console-issue"><span className="console-dot signal" />{bulkNote}</span>
        )}
        <span className="console-status-right">
          <TextInput
            size="xs"
            leftSection={<IconSearch size={13} />}
            placeholder={t(locale, 'searchProviders')}
            value={search}
            onChange={event => setSearch(event.currentTarget.value)}
            w={240}
          />
        </span>
      </div>

      <section className="console-panel">
        <div className="console-panel-head">
          <div className="console-panel-title">{t(locale, 'platformList')}</div>
          <span className="console-panel-sub">{range.toUpperCase()}</span>
        </div>
        {items.length > 0 ? (
          <div className="console-table-wrap console-table-scroll">
            <Table className="console-table">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>{t(locale, 'thName')}</Table.Th>
                  <Table.Th>{t(locale, 'thType')}</Table.Th>
                  <Table.Th>{t(locale, 'thBaseUrl')}</Table.Th>
                  <Table.Th>{t(locale, 'thCheck')}</Table.Th>
                  {showCheckin && <Table.Th>{t(locale, 'checkinColumn')}</Table.Th>}
                  <Table.Th className="num">{t(locale, 'models')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'thRefs')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'reqCount')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'successCountShort')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'avgShort')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'credits')}</Table.Th>
                  <Table.Th className="num" />
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {items.map(item => {
                  const { platform, stats, health } = item
                  const rate = stats?.success_rate ?? 0
                  const tone = successRateTone(rate, (stats?.total_requests ?? 0) > 0)
                  // 本次会话刚测过的结果优先，其余读库里的最后一次
                  const sessionCheck = tests[platform.id]
                  const lastOk = sessionCheck ? sessionCheck.ok : health?.last_ok ?? null
                  const checkTip = sessionCheck?.message ?? (
                    health && health.last_checked_at
                      ? t(locale, 'healthChecks', String(health.checks), String(health.failures), formatLogTime(health.last_checked_at, locale))
                        + (health.last_error ? ` · ${health.last_error}` : '')
                      : null
                  )
                  const presetMatch = platformPresets.find(p => p.name === platform.name)
                  const displayName = presetMatch ? getPresetName(presetMatch, locale) : platform.name
                  const checkinCapability = capabilityFor(platform.base_url)
                  const checkinAccount = checkinByPlatform.get(platform.id)
                  const checkinStatus = checkinAccount?.status
                  const checkinBusy = loginBusyId === platform.id || claimBusyId === platform.id
                  // 积分列显示台账剩余（随每笔请求扣减）；领取时用上游余额校准台账，
                  // 真实余额只进 tooltip，避免显示成一份不会动的快照
                  const creditShown = item.credit ? item.credit.remaining : null
                  const creditTooltip = item.credit
                    ? (item.credit.live_balance != null
                      ? t(
                          locale,
                          'creditLiveTooltip',
                          formatNumber(item.credit.live_balance),
                          formatNumber(item.credit.spent),
                          formatNumber(item.credit.budget_effective),
                          item.credit.balance_at ? formatLogTime(item.credit.balance_at, locale) : '—',
                        )
                      : `${t(locale, 'creditLocalTooltip', formatNumber(item.credit.spent), formatNumber(item.credit.budget_effective))} · ${t(locale, 'creditResetAt')} ${new Date(item.credit.reset_at).toLocaleString()}`)
                    : ''
                  return (
                    <Table.Tr key={platform.id} onClick={() => navigate(`/providers/${platform.id}`)}>
                      <Table.Td><span className="console-strong">{displayName}</span></Table.Td>
                      <Table.Td>{platformTypeLabel(platform.type)}</Table.Td>
                      <Table.Td>
                        <span className="console-mono muted" title={platform.base_url}>
                          {platform.base_url}
                        </span>
                      </Table.Td>
                      <Table.Td>
                        {lastOk == null ? (
                          <span className="muted">{t(locale, 'notTested')}</span>
                        ) : (
                          <Tooltip label={checkTip}>
                            <StatusPill tone={lastOk ? 'ok' : 'err'}>
                              {lastOk ? t(locale, 'testedOk') : t(locale, 'testedFail')}
                            </StatusPill>
                          </Tooltip>
                        )}
                      </Table.Td>
                      {showCheckin && (
                        <Table.Td onClick={event => event.stopPropagation()}>
                          {checkinCapability ? (
                            <div className="checkin-cell">
                              <span className={`console-dot ${checkinStatus ? CHECKIN_TONE[checkinStatus] : 'idle'}`} />
                              {checkinAccount ? (
                                <Tooltip label={checkinAccount.message || checkinAccount.label}>
                                  <span className="checkin-identity">
                                    {checkinAccount.identity || checkinAccount.label}
                                  </span>
                                </Tooltip>
                              ) : (
                                <span className="muted">{t(locale, 'checkinNotLoggedIn')}</span>
                              )}
                              <Button
                                size="compact-xs"
                                variant={checkinAccount && checkinStatus !== 'expired' ? 'default' : 'light'}
                                loading={checkinBusy}
                                onClick={() => {
                                  if (checkinAccount && checkinStatus !== 'expired') void claimPlatform(item)
                                  else void loginPlatform(item)
                                }}
                              >
                                {checkinAccount
                                  ? (checkinStatus === 'expired' ? t(locale, 'checkinRelogin') : t(locale, 'checkinClaim'))
                                  : t(locale, 'checkinLogin')}
                              </Button>
                            </div>
                          ) : (
                            <span className="muted">—</span>
                          )}
                        </Table.Td>
                      )}
                      <Table.Td className="num">{item.modelCount || '—'}</Table.Td>
                      <Table.Td className="num">
                        <span title={t(locale, 'providerReferenced', String(item.refBackends))}>
                          {item.refModels || '—'}
                        </span>
                      </Table.Td>
                      <Table.Td className="num">{formatNumber(stats?.total_requests ?? 0)}</Table.Td>
                      <Table.Td className="num">
                        {stats?.total_requests
                          ? <StatusPill tone={tone}>{`${rate.toFixed(1)}%`}</StatusPill>
                          : <span className="muted">—</span>}
                      </Table.Td>
                      <Table.Td className="num">{stats?.total_requests ? formatDuration(stats.avg_latency_ms) : '—'}</Table.Td>
                      <Table.Td className="num">
                        {item.credit && creditShown != null ? (
                          <Tooltip label={creditTooltip}>
                            <span className={creditShown <= 0 ? 'err' : creditShown <= item.credit.budget_effective * 0.2 ? 'warn' : ''}>
                              {`${formatNumber(creditShown)} / ${formatNumber(item.credit.budget_effective)}`}
                            </span>
                          </Tooltip>
                        ) : <span className="muted">—</span>}
                      </Table.Td>
                      <Table.Td className="num">
                        <Group gap={2} wrap="nowrap" justify="flex-end">
                          <Tooltip label={t(locale, 'testConnection')}>
                            <ActionIcon
                              size="sm"
                              variant="subtle"
                              color={lastOk === false ? 'red' : 'gray'}
                              loading={testingId === platform.id}
                              aria-label={t(locale, 'testConnection')}
                              onClick={event => { event.stopPropagation(); void runTest(item) }}
                            >
                              <IconPlugConnected size={15} />
                            </ActionIcon>
                          </Tooltip>
                          <Tooltip label={t(locale, 'delete')}>
                            <ActionIcon
                              size="sm"
                              variant="subtle"
                              color="red"
                              aria-label={t(locale, 'delete')}
                              onClick={event => { event.stopPropagation(); setDeleteTarget(item) }}
                            >
                              <IconTrash size={15} />
                            </ActionIcon>
                          </Tooltip>
                          <IconArrowRight size={14} />
                        </Group>
                      </Table.Td>
                    </Table.Tr>
                  )
                })}
              </Table.Tbody>
            </Table>
          </div>
        ) : (
          <EmptyState
            title={search ? t(locale, 'noMatchingRequests') : t(locale, 'noPlatforms')}
            description={search ? t(locale, 'clearFilters') : t(locale, 'platformDesc')}
            action={search ? (
              <Button size="xs" variant="default" onClick={() => setSearch('')}>{t(locale, 'clearFilters')}</Button>
            ) : (
              <Button size="xs" leftSection={<IconPlus size={14} />} onClick={() => setCreateOpen(true)}>
                {t(locale, 'addPlatform')}
              </Button>
            )}
          />
        )}
      </section>

      <ProviderCreateModal opened={createOpen} onClose={() => setCreateOpen(false)} />

      <CheckinTasksModal
        opened={tasksOpen}
        onClose={() => setTasksOpen(false)}
        tasks={checkinQuery.data?.tasks ?? []}
        accounts={checkinQuery.data?.accounts ?? []}
        options={taskOptions}
        onChanged={() => {
          void checkinQuery.refetch()
          // 领取结果会校准积分列
          void queryClient.invalidateQueries({ queryKey: ['platforms'] })
        }}
      />

      <Modal
        opened={Boolean(deleteTarget)}
        onClose={() => setDeleteTarget(null)}
        title={`${t(locale, 'delete')} · ${deleteTarget?.platform.name ?? ''}`}
        size="sm"
        closeButtonProps={{ 'aria-label': t(locale, 'close') }}
      >
        <div className="console-note warn">
          {deleteTarget
            ? (deleteTarget.refModels > 0
              ? t(locale, 'providerDeleteWarn', String(deleteTarget.refModels))
              : t(locale, 'providerDeleteSafe'))
            : ''}
        </div>
        {deleteTarget && deleteTarget.refModels > 0 && (
          <div className="console-inspector-field" style={{ paddingTop: 8 }}>
            <span className="k">{t(locale, 'impactLabel')}</span>
            <span className="v">{deleteTarget.refBackends} backends · {deleteTarget.modelCount} models</span>
          </div>
        )}
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setDeleteTarget(null)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" loading={saving} onClick={() => void remove()}>{t(locale, 'confirmDelete')}</Button>
        </div>
      </Modal>
    </div>
  )
}
