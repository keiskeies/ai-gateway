import { useEffect, useMemo, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Autocomplete,
  Button,
  Modal,
  MultiSelect,
  NumberInput,
  Select,
  TextInput,
  Textarea,
  Tooltip,
} from '@mantine/core'
import { notifications } from '@mantine/notifications'
import {
  IconArrowLeft,
  IconCopy,
  IconEdit,
  IconPlus,
  IconRefresh,
  IconTrash,
} from '@tabler/icons-react'
import { useNavigate, useParams } from 'react-router-dom'
import {
  addBackend,
  createRoute,
  deleteBackend,
  deleteProxy,
  getProxy,
  getProxyHistoryImpact,
  getProxyStats,
  getSettings,
  listApiKeys,
  listBackends,
  listPlatforms,
  updateBackend,
  updateProxy,
  updateRoute,
} from '../api'
import { rangeToParams, useAppState } from '../app/AppState'
import { CAPABILITY_OPTIONS, getModelsForPlatform } from '../presets'
import { useAppContext } from '../ThemeContext'
import { t, type Locale, type TranslationKey } from '../i18n'
import { PageHeader } from '../components/ui/PageHeader'
import { EmptyState, ErrorState, PageSkeleton } from '../components/ui/States'
import { Readout } from '../components/ui/Readout'
import { StatusPill } from '../components/ui/StatusPill'
import { formatCompact, formatDuration, formatNumber, successRateTone } from '../lib/format'
import type { ApiKeyRow, PlatformRow, ProxyRow, ProxyStats } from '../types/stats'
import { Field } from '../components/Field'

interface BackendInfo {
  id: string
  route_id: string
  platform_id: string
  model_id: string
  weight: number
  priority: number
  max_concurrent?: number | null
  capabilities: string[]
  status: string
  param_overrides?: Record<string, unknown>
  unknown_params: string
  reasoning_style: string
  reasoning_echo: string
}

interface RouteInfo {
  id: string
  proxy_id: string
  lb_strategy: string
  retry_policy: { max_retries: number; retry_on_error: string[]; backoff_ms: number }
  fallback?: string | null
  default_max_tokens?: number | null
  backends: BackendInfo[]
}

interface ProxyDetailResponse {
  proxy: ProxyRow
  routes: RouteInfo[]
}

interface BackendDraft {
  platform_id: string
  model_id: string
  weight: number
  priority: number
  capabilities: string[]
  unknown_params: string
  reasoning_style: string
  reasoning_echo: string
  param_overrides: string
}

const emptyDraft: BackendDraft = {
  platform_id: '',
  model_id: '',
  weight: 1,
  priority: 0,
  capabilities: [],
  unknown_params: 'Auto',
  reasoning_style: 'Auto',
  reasoning_echo: 'Drop',
  param_overrides: '',
}

const LB_OPTIONS = [
  { value: 'RoundRobin', zh: '轮询', en: 'Round robin' },
  { value: 'WeightedRandom', zh: '加权随机', en: 'Weighted' },
  { value: 'LeastConnections', zh: '最少连接', en: 'Least connections' },
  { value: 'Priority', zh: '优先级', en: 'Priority' },
  { value: 'LatencyBased', zh: '延迟优先', en: 'Latency based' },
]

function strategyLabel(value: string | undefined, locale: 'zh' | 'en'): string {
  if (!value) return '—'
  return LB_OPTIONS.find(option => option.value === value)?.[locale] || value
}

const REASONING_OPTIONS = [
  { value: 'Auto', label: 'Auto' },
  { value: 'OpenAiEffort', label: 'OpenAI effort' },
  { value: 'AnthropicThinking', label: 'Anthropic thinking' },
  { value: 'OpenRouter', label: 'OpenRouter' },
  { value: 'Qwen', label: 'Qwen' },
  { value: 'Glm', label: 'GLM' },
  { value: 'VllmQwen', label: 'vLLM Qwen' },
  { value: 'Disabled', label: 'Disabled' },
]

const ECHO_OPTIONS = [
  { value: 'Drop', label: 'Drop' },
  { value: 'Keep', label: 'Keep' },
  { value: 'Auto', label: 'Auto' },
]

const UNKNOWN_OPTIONS = [
  { value: 'Auto', label: 'Auto' },
  { value: 'Drop', label: 'Drop' },
  { value: 'Passthrough', label: 'Passthrough' },
]

// 枚举值在屏上也要说当前语言：值是后端契约，标签是给人看的
const REASONING_LABEL_KEYS: Record<string, TranslationKey> = {
  Auto: 'autoValue',
  OpenAiEffort: 'reasoningStyleOpenAiEffort',
  AnthropicThinking: 'reasoningStyleAnthropic',
  OpenRouter: 'reasoningStyleOpenRouter',
  Qwen: 'reasoningStyleQwen',
  Glm: 'reasoningStyleGlm',
  VllmQwen: 'reasoningStyleVllmQwen',
  Disabled: 'modelStatusDisabled',
}
const ECHO_LABEL_KEYS: Record<string, TranslationKey> = { Drop: 'dropValue', Keep: 'keepValue', Auto: 'autoValue' }
const UNKNOWN_LABEL_KEYS: Record<string, TranslationKey> = {
  Auto: 'autoValue',
  Drop: 'dropValue',
  Passthrough: 'passthroughValue',
}

function optionLabel(keys: Record<string, TranslationKey>, value: string, locale: Locale): string {
  const key = keys[value]
  return key ? t(locale, key) : value
}

const CAP_OPTIONS = CAPABILITY_OPTIONS.map(cap => ({
  value: cap.value,
  label: cap.labelEn,
}))

function backendTone(backend: BackendInfo): 'ok' | 'warn' | 'idle' {
  if (backend.status === 'Disabled') return 'idle'
  return 'ok'
}

export default function ModelDetail() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { locale } = useAppContext()
  const { range } = useAppState()
  const [tab, setTab] = useState<'overview' | 'routing' | 'usage' | 'settings'>('routing')
  const [selectedBackendId, setSelectedBackendId] = useState<string | null>(null)
  const [backendModalOpen, setBackendModalOpen] = useState(false)
  const [editingBackend, setEditingBackend] = useState<BackendInfo | null>(null)
  const [draft, setDraft] = useState<BackendDraft>(emptyDraft)
  const [deleteBackendTarget, setDeleteBackendTarget] = useState<BackendInfo | null>(null)
  const [deleteModelOpen, setDeleteModelOpen] = useState(false)
  const [saving, setSaving] = useState(false)
  const [rename, setRename] = useState('')

  const modelQuery = useQuery({
    queryKey: ['model', id],
    queryFn: async (): Promise<ProxyDetailResponse> => {
      const detail = await getProxy(id!) as { proxy: ProxyRow; routes: RouteInfo[] }
      const routes = await Promise.all((detail.routes || []).map(async route => ({
        ...route,
        backends: route.backends?.length ? route.backends : await listBackends(route.id) as BackendInfo[],
      })))
      return { proxy: detail.proxy, routes }
    },
    enabled: Boolean(id),
  })

  const statsQuery = useQuery({
    queryKey: ['model-stats', id, range],
    queryFn: async (): Promise<ProxyStats> => getProxyStats(id!, rangeToParams(range)) as Promise<ProxyStats>,
    enabled: Boolean(id),
  })

  // 删除确认要报出条数：这是按 proxy_id 的一次计数，只在真的要删时才查
  const impactQuery = useQuery({
    queryKey: ['model-history-impact', id],
    queryFn: () => getProxyHistoryImpact(id!) as Promise<{ requests: number; attempts: number }>,
    enabled: Boolean(id) && deleteModelOpen,
  })

  const platformsQuery = useQuery({
    queryKey: ['platforms'],
    queryFn: async (): Promise<PlatformRow[]> => listPlatforms() as Promise<PlatformRow[]>,
    staleTime: 60_000,
  })

  const apiKeysQuery = useQuery({
    queryKey: ['api-keys'],
    queryFn: async (): Promise<ApiKeyRow[]> => listApiKeys() as Promise<ApiKeyRow[]>,
    staleTime: 60_000,
  })

  const settingsQuery = useQuery({
    queryKey: ['settings'],
    queryFn: () => getSettings(),
    staleTime: 60_000,
  })

  const detail = modelQuery.data
  const proxy = detail?.proxy
  const routes = detail?.routes ?? []
  const route = routes[0]
  const backends = route?.backends ?? []
  const selectedBackend = backends.find(backend => backend.id === selectedBackendId) || backends[0] || null
  const platforms = platformsQuery.data ?? []
  const apiKeys = apiKeysQuery.data ?? []
  const stats = statsQuery.data
  const impact = impactQuery.data
  const adminPort = settingsQuery.data?.admin_port ?? 1994

  const [routeDraft, setRouteDraft] = useState({
    strategy: 'RoundRobin',
    retries: 2,
    backoff: 500,
    maxTokens: 4096,
  })

  useEffect(() => {
    setTab('routing')
    setBackendModalOpen(false)
    setDeleteBackendTarget(null)
    setDeleteModelOpen(false)
  }, [id])

  useEffect(() => {
    if (!detail) return
    setRename(detail.proxy.name)
    const firstRoute = detail.routes[0]
    setRouteDraft(firstRoute ? {
      strategy: firstRoute.lb_strategy || 'RoundRobin',
      retries: firstRoute.retry_policy?.max_retries ?? 2,
      backoff: firstRoute.retry_policy?.backoff_ms ?? 500,
      maxTokens: firstRoute.default_max_tokens ?? 4096,
    } : {
      strategy: 'RoundRobin',
      retries: 2,
      backoff: 500,
      maxTokens: 4096,
    })
    setSelectedBackendId(current => (
      current && firstRoute?.backends?.some(backend => backend.id === current)
        ? current
        : firstRoute?.backends?.[0]?.id ?? null
    ))
  }, [detail])

  const modelSuggestions = useMemo(() => {
    const platform = platforms.find(item => item.id === draft.platform_id)
    if (!platform) return []
    return getModelsForPlatform(platform.name).map(model => model.model_id)
  }, [platforms, draft.platform_id])

  const invalidate = async () => {
    await queryClient.invalidateQueries({ queryKey: ['model', id] })
    await queryClient.invalidateQueries({ queryKey: ['models'] })
    await queryClient.invalidateQueries({ queryKey: ['model-stats', id] })
  }

  const openAddBackend = () => {
    setEditingBackend(null)
    setDraft(emptyDraft)
    setBackendModalOpen(true)
  }

  const openEditBackend = (backend: BackendInfo) => {
    setEditingBackend(backend)
    setDraft({
      platform_id: backend.platform_id,
      model_id: backend.model_id,
      weight: backend.weight,
      priority: backend.priority,
      capabilities: backend.capabilities || [],
      unknown_params: backend.unknown_params || 'Auto',
      reasoning_style: backend.reasoning_style || 'Auto',
      reasoning_echo: backend.reasoning_echo || 'Drop',
      param_overrides: backend.param_overrides && Object.keys(backend.param_overrides).length
        ? JSON.stringify(backend.param_overrides, null, 2)
        : '',
    })
    setBackendModalOpen(true)
  }

  const saveBackend = async () => {
    if (!proxy) return
    if (!draft.platform_id || !draft.model_id.trim()) {
      notifications.show({ message: t(locale, 'createFailed'), color: 'red' })
      return
    }
    let overrides: Record<string, unknown> = {}
    const overrideText = draft.param_overrides.trim()
    if (overrideText) {
      try {
        const parsed = JSON.parse(overrideText)
        if (typeof parsed !== 'object' || Array.isArray(parsed) || parsed === null) throw new Error('invalid')
        overrides = parsed
      } catch {
        notifications.show({ message: t(locale, 'paramOverridesInvalid'), color: 'red' })
        return
      }
    }
    const payload = {
      platform_id: draft.platform_id,
      model_id: draft.model_id.trim(),
      weight: draft.weight,
      priority: draft.priority,
      capabilities: draft.capabilities,
      unknown_params: draft.unknown_params,
      reasoning_style: draft.reasoning_style,
      reasoning_echo: draft.reasoning_echo,
      param_overrides: overrides,
    }
    setSaving(true)
    try {
      if (editingBackend) {
        const sameUpstream = editingBackend.platform_id === payload.platform_id
          && editingBackend.model_id === payload.model_id
        if (sameUpstream) {
          await updateBackend(editingBackend.id, {
            weight: payload.weight,
            priority: payload.priority,
            capabilities: payload.capabilities,
            unknown_params: payload.unknown_params,
            reasoning_style: payload.reasoning_style,
            reasoning_echo: payload.reasoning_echo,
            param_overrides: payload.param_overrides,
          })
        } else {
          await deleteBackend(editingBackend.id)
          if (route) await addBackend(route.id, payload)
          else await createRoute(proxy.id, { lb_strategy: 'RoundRobin', default_max_tokens: 4096, backends: [payload] })
        }
      } else if (route) {
        await addBackend(route.id, payload)
      } else {
        await createRoute(proxy.id, { lb_strategy: 'RoundRobin', default_max_tokens: 4096, backends: [payload] })
      }
      await invalidate()
      setBackendModalOpen(false)
      notifications.show({ message: t(locale, 'updateSuccess'), color: 'green' })
    } catch {
      notifications.show({ message: t(locale, 'updateFailed'), color: 'red' })
    } finally {
      setSaving(false)
    }
  }

  const saveRoute = async () => {
    if (!route) return
    setSaving(true)
    try {
      await updateRoute(route.id, {
        lb_strategy: routeDraft.strategy,
        retry_policy: {
          max_retries: routeDraft.retries,
          retry_on_error: ['RateLimit', 'ServerError', 'Timeout'],
          backoff_ms: routeDraft.backoff,
        },
        default_max_tokens: routeDraft.maxTokens,
      })
      await invalidate()
      notifications.show({ message: t(locale, 'updateSuccess'), color: 'green' })
    } catch {
      notifications.show({ message: t(locale, 'updateFailed'), color: 'red' })
    } finally {
      setSaving(false)
    }
  }

  const createEmptyRoute = async () => {
    if (!proxy) return
    setSaving(true)
    try {
      await createRoute(proxy.id, {
        lb_strategy: 'RoundRobin',
        default_max_tokens: 4096,
        backends: [],
      })
      await invalidate()
    } catch {
      notifications.show({ message: t(locale, 'createFailed'), color: 'red' })
    } finally {
      setSaving(false)
    }
  }

  const deleteSelectedBackend = async () => {
    if (!deleteBackendTarget) return
    setSaving(true)
    try {
      await deleteBackend(deleteBackendTarget.id)
      setDeleteBackendTarget(null)
      await invalidate()
      notifications.show({ message: t(locale, 'deleteSuccess'), color: 'green' })
    } catch {
      notifications.show({ message: t(locale, 'updateFailed'), color: 'red' })
    } finally {
      setSaving(false)
    }
  }

  const saveName = async () => {
    if (!proxy || !rename.trim() || rename.trim() === proxy.name) return
    setSaving(true)
    try {
      await updateProxy(proxy.id, { name: rename.trim() })
      await invalidate()
      notifications.show({ message: t(locale, 'updateSuccess'), color: 'green' })
    } catch {
      notifications.show({ message: t(locale, 'updateFailed'), color: 'red' })
    } finally {
      setSaving(false)
    }
  }

  const removeModel = async () => {
    if (!proxy) return
    setSaving(true)
    try {
      const result = await deleteProxy(proxy.id)
      const disabled = Number(result.data?.disabled_keys ?? 0)
      const purged = impact?.requests ?? 0
      navigate('/models')
      const parts = [
        disabled > 0 ? t(locale, 'keysDisabledWithModel', String(disabled)) : '',
        purged > 0 ? t(locale, 'historyPurgedWithModel', String(purged)) : '',
      ].filter(Boolean)
      notifications.show({
        title: t(locale, 'deleteSuccess'),
        message: parts.join(' · '),
        color: parts.length > 0 ? 'amber' : 'green',
      })
    } catch {
      notifications.show({ message: t(locale, 'updateFailed'), color: 'red' })
    } finally {
      setSaving(false)
    }
  }

  if (modelQuery.isPending) return <PageSkeleton rows={6} />
  if (modelQuery.isError || !detail || !proxy) return <ErrorState onRetry={() => void modelQuery.refetch()} />

  const boundKeys = apiKeys.filter(key => key.proxy_id === proxy.id)
  const statsTone = successRateTone(stats?.success_rate ?? 0, (stats?.total_requests ?? 0) > 0)
  const totalTokens = (stats?.total_token_input ?? 0) + (stats?.total_token_output ?? 0)
  const weightTotal = backends.reduce((sum, backend) => sum + Math.max(1, backend.weight), 0) || 1

  return (
    <div className="console-page">
      <PageHeader
        kicker={<>{t(locale, 'kickerConfigure')} <span className="console-kicker-sep" /> {t(locale, 'kickerVirtualModel')}</>}
        title={proxy.name}
        mono
        subtitle={t(locale, 'modelSubtitle', String(backends.length), strategyLabel(route?.lb_strategy, locale))}
        actions={
          <>
            <Button size="xs" variant="default" leftSection={<IconArrowLeft size={14} />} onClick={() => navigate('/models')}>
              {t(locale, 'backToList')}
            </Button>
            <Button size="xs" variant="default" leftSection={<IconCopy size={14} />} onClick={() => {
              void navigator.clipboard.writeText(proxy.name)
              notifications.show({ message: t(locale, 'copied'), color: 'green' })
            }}>
              {t(locale, 'copyId')}
            </Button>
            <Button size="xs" variant="default" leftSection={<IconRefresh size={14} />} onClick={() => void invalidate()}>
              {t(locale, 'refresh')}
            </Button>
          </>
        }
      />

      <div className="console-tabs" role="tablist">
        {([
          ['overview', t(locale, 'tabOverview')],
          ['routing', t(locale, 'tabRouting')],
          ['usage', t(locale, 'tabUsage')],
          ['settings', t(locale, 'tabSettings')],
        ] as const).map(([key, label]) => (
          <button
            key={key}
            type="button"
            role="tab"
            aria-selected={tab === key}
            className={`console-tab${tab === key ? ' active' : ''}`}
            onClick={() => setTab(key)}
          >
            {label}
          </button>
        ))}
      </div>

      {tab === 'overview' && (
        <div className="console-page">
          <div className="console-readouts">
            <Readout label={t(locale, 'requestsLabel')} value={formatNumber(stats?.total_requests ?? 0)} meta={<span>{range.toUpperCase()}</span>} />
            <Readout
              label={t(locale, 'successLabel')}
              value={(stats?.success_rate ?? 0).toFixed(1)}
              unit="%"
              meta={<StatusPill tone={statsTone}>{t(locale, statsTone === 'ok' ? 'healthHealthy' : statsTone === 'warn' ? 'healthDegraded' : statsTone === 'err' ? 'healthFailing' : 'healthIdle')}</StatusPill>}
            />
            <Readout label="AVG LATENCY" value={stats?.total_requests ? formatDuration(stats.avg_latency_ms) : '—'} meta={<span>{strategyLabel(route?.lb_strategy, locale)}</span>} />
            <Readout label="TOKENS" value={formatCompact(totalTokens)} meta={<span>{formatCompact(stats?.total_token_input ?? 0)} in · {formatCompact(stats?.total_token_output ?? 0)} out</span>} />
          </div>
          <section className="console-panel">
            <div className="console-panel-head"><div className="console-panel-title">{t(locale, 'modelInfo')}</div><span className="console-panel-sub">{t(locale, 'publicId')}</span></div>
            <div className="console-panel-body">
              <div className="console-info-grid">
                <div><span className="console-meta-label">{t(locale, 'modelId')}</span><strong className="console-mono">{proxy.name}</strong></div>
                <div><span className="console-meta-label">{t(locale, 'createdAt')}</span><strong className="console-mono">{proxy.created_at ? new Date(proxy.created_at).toLocaleString() : '—'}</strong></div>
                <div><span className="console-meta-label">{t(locale, 'backends')}</span><strong>{backends.length}</strong></div>
                <div><span className="console-meta-label">{t(locale, 'capabilities')}</span><span className="console-cap-list">{(proxy.capabilities || []).length ? (proxy.capabilities || []).map(cap => <span className="console-cap" key={cap}>{cap}</span>) : '—'}</span></div>
              </div>
            </div>
          </section>
        </div>
      )}

      {tab === 'routing' && (
        <>
          <div className="console-model-layout">
            <aside className="console-panel console-route-config">
              <div className="console-panel-head"><div className="console-panel-title">{t(locale, 'routeConfig')}</div><span className="console-panel-sub">{t(locale, 'routeLabel')}</span></div>
              <div className="console-panel-body">
                {route ? (
                  <>
                    <Field label={t(locale, 'strategy')}>
                      <Select
                        size="xs"
                        data={LB_OPTIONS.map(option => ({ value: option.value, label: option[locale] }))}
                        value={routeDraft.strategy}
                        onChange={value => setRouteDraft(prev => ({ ...prev, strategy: value || 'RoundRobin' }))}
                        allowDeselect={false}
                      />
                    </Field>
                    <Field label={t(locale, 'retryAttempts')}>
                      <NumberInput size="xs" min={0} max={10} value={routeDraft.retries} onChange={value => setRouteDraft(prev => ({ ...prev, retries: Number(value) || 0 }))} />
                    </Field>
                    <Field label={t(locale, 'backoffMs')}>
                      <NumberInput size="xs" min={100} value={routeDraft.backoff} onChange={value => setRouteDraft(prev => ({ ...prev, backoff: Number(value) || 100 }))} />
                    </Field>
                    <Field label={t(locale, 'defaultMaxTokens')}>
                      <NumberInput size="xs" min={1} value={routeDraft.maxTokens} onChange={value => setRouteDraft(prev => ({ ...prev, maxTokens: Number(value) || 1 }))} />
                    </Field>
                    <div className="console-field">
                      <label>{t(locale, 'weightDistribution')}</label>
                      <div className="console-weight-preview">
                        {backends.map((backend, index) => (
                          <span key={backend.id} style={{ flex: Math.max(1, backend.weight), opacity: 1 - index * 0.14 }} />
                        ))}
                      </div>
                      <div className="console-weight-labels">{backends.map(backend => <span key={backend.id}>w{backend.weight}</span>)}</div>
                    </div>
                    <div className="console-route-actions">
                      <Button size="xs" loading={saving} onClick={() => void saveRoute()}>{t(locale, 'saveRoute')}</Button>
                      <Button size="xs" variant="default" onClick={() => setRouteDraft({
                        strategy: route.lb_strategy || 'RoundRobin',
                        retries: route.retry_policy?.max_retries ?? 2,
                        backoff: route.retry_policy?.backoff_ms ?? 500,
                        maxTokens: route.default_max_tokens ?? 4096,
                      })}>{t(locale, 'discardChanges')}</Button>
                    </div>
                  </>
                ) : (
                  <EmptyState
                    title={t(locale, 'noRouteYet')}
                    description={t(locale, 'noRouteYetDesc')}
                    action={<Button size="xs" loading={saving} onClick={() => void createEmptyRoute()}>{t(locale, 'createRoute')}</Button>}
                  />
                )}
              </div>
            </aside>

            <section className="console-panel console-patch">
              <div className="console-panel-head">
                <div className="console-panel-title">{t(locale, 'routeSpine')}</div>
                <span className="console-panel-sub">{t(locale, 'backendsCount', String(backends.length))}</span>
              </div>
              <div className="console-panel-body">
                {route ? (
                  <>
                    <div className="console-patch-canvas">
                      <div className="console-route-node">
                        <div className="console-route-kicker">{t(locale, 'routeLabel')}</div>
                        <div className="console-route-name">{strategyLabel(route.lb_strategy, locale)}</div>
                        <div className="console-route-meta">
                          {routeDraft.retries} {t(locale, 'retriesShort')} · {routeDraft.backoff}ms<br />
                          {t(locale, 'maxTokensShort')} {routeDraft.maxTokens}<br />
                          {t(locale, 'backendsCount', String(backends.length))}
                        </div>
                      </div>
                      <div className="console-patch-bus" />
                      <div className="console-backend-stack">
                        {backends.map((backend, index) => {
                          const platform = platforms.find(item => item.id === backend.platform_id)
                          return (
                            <button
                              type="button"
                              key={backend.id}
                              className={`console-backend-card${selectedBackend?.id === backend.id ? ' selected' : ''}${backend.status === 'Disabled' ? ' degraded' : ''}`}
                              onClick={() => setSelectedBackendId(backend.id)}
                            >
                              <span className="idx">{String(index + 1).padStart(2, '0')}</span>
                              <span className="main">
                                <span className="title">{platform?.name || backend.platform_id}</span>
                                <span className="sub">{backend.model_id}</span>
                              </span>
                              <span className="weight">w{backend.weight}</span>
                              <span className={`console-dot ${backendTone(backend)}`} />
                            </button>
                          )
                        })}
                        <button type="button" className="console-add-socket" onClick={openAddBackend}>
                          <IconPlus size={13} /> {t(locale, 'addBackend').toUpperCase()}
                        </button>
                      </div>
                    </div>
                    <div className="console-patch-foot">
                      <span>{t(locale, 'weightTotalValue', String(weightTotal))} · {t(locale, 'weightTotalHint')}</span>
                      <span>{t(locale, 'weightedSignal')}</span>
                    </div>
                  </>
                ) : (
                  <EmptyState
                    title={t(locale, 'noBackendYet')}
                    description={t(locale, 'noBackendYetDesc')}
                    action={<Button size="xs" leftSection={<IconPlus size={14} />} onClick={openAddBackend}>{t(locale, 'addBackend')}</Button>}
                  />
                )}
              </div>
            </section>

            <aside className="console-panel console-backend-inspector">
              <div className="console-panel-head">
                <div className="console-panel-title">{selectedBackend ? t(locale, 'backendDetail') : t(locale, 'backends')}</div>
                {selectedBackend && <StatusPill tone={backendTone(selectedBackend)}>{selectedBackend.status === 'Active' ? t(locale, 'modelStatusActive') : t(locale, 'modelStatusDisabled')}</StatusPill>}
              </div>
              {selectedBackend ? (
                <div className="console-panel-body">
                  <div className="console-inspector-field"><span className="k">{t(locale, 'providerLabel')}</span><span className="v">{platforms.find(p => p.id === selectedBackend.platform_id)?.name || selectedBackend.platform_id}</span></div>
                  <div className="console-inspector-field"><span className="k">{t(locale, 'model')}</span><span className="v">{selectedBackend.model_id}</span></div>
                  <div className="console-inspector-field"><span className="k">{t(locale, 'weightLabel')}</span><span className="v">{selectedBackend.weight}</span></div>
                  <div className="console-inspector-field"><span className="k">{t(locale, 'priorityLabel')}</span><span className="v">{selectedBackend.priority}</span></div>
                  <div className="console-inspector-field"><span className="k">{t(locale, 'capabilities')}</span><span className="v"><span className="console-cap-list">{(selectedBackend.capabilities || []).length ? selectedBackend.capabilities.map(cap => <span className="console-cap" key={cap}>{cap}</span>) : '—'}</span></span></div>
                  <div className="console-inspector-field"><span className="k">{t(locale, 'reasoningLabel')}</span><span className="v">{optionLabel(REASONING_LABEL_KEYS, selectedBackend.reasoning_style, locale)}</span></div>
                  <div className="console-inspector-field"><span className="k">{t(locale, 'echoLabel')}</span><span className="v">{optionLabel(ECHO_LABEL_KEYS, selectedBackend.reasoning_echo, locale)}</span></div>
                  <div className="console-inspector-field"><span className="k">{t(locale, 'unknownParamsLabel')}</span><span className="v">{optionLabel(UNKNOWN_LABEL_KEYS, selectedBackend.unknown_params, locale)}</span></div>
                  {selectedBackend.param_overrides && Object.keys(selectedBackend.param_overrides).length > 0 && (
                    <div className="console-advanced-row">
                      <span>{t(locale, 'paramOverridesLabel')}</span>
                      <span className="console-mono">{JSON.stringify(selectedBackend.param_overrides)}</span>
                    </div>
                  )}
                  <div className="console-inspector-actions">
                    <Button size="xs" variant="default" leftSection={<IconEdit size={13} />} onClick={() => openEditBackend(selectedBackend)}>{t(locale, 'edit')}</Button>
                    <Button size="xs" variant="default" color="red" leftSection={<IconTrash size={13} />} onClick={() => setDeleteBackendTarget(selectedBackend)}>{t(locale, 'delete')}</Button>
                  </div>
                </div>
              ) : (
                <EmptyState title={t(locale, 'selectBackend')} description={t(locale, 'selectBackendDesc')} />
              )}
            </aside>
          </div>
        </>
      )}

      {tab === 'usage' && (
        <div className="console-grid-main">
          <section className="console-panel">
            <div className="console-panel-head"><div className="console-panel-title">{t(locale, 'openaiCurl')}</div><span className="console-panel-sub">cURL</span></div>
            <div className="console-panel-body">
              <pre className="code-block">{`curl http://localhost:${adminPort}/v1/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer <your-api-key>" \\
  --data '{
    "model": "${proxy.name}",
    "messages": [{"role": "user", "content": "Hello"}],
    "max_tokens": 100
  }'`}</pre>
            </div>
          </section>
          <section className="console-panel">
            <div className="console-panel-head"><div className="console-panel-title">{t(locale, 'anthropicCurl')}</div><span className="console-panel-sub">cURL</span></div>
            <div className="console-panel-body">
              <pre className="code-block">{`curl http://localhost:${adminPort}/v1/messages \\
  -H "Content-Type: application/json" \\
  -H "anthropic-version: 2023-06-01" \\
  -H "x-api-key: <your-api-key>" \\
  --data '{
    "model": "${proxy.name}",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hello"}]
  }'`}</pre>
            </div>
          </section>
        </div>
      )}

      {tab === 'settings' && (
        <div className="console-settings-grid">
          <section className="console-panel">
            <div className="console-panel-head"><div className="console-panel-title">{t(locale, 'modelIdentity')}</div><span className="console-panel-sub">{t(locale, 'publicId')}</span></div>
            <div className="console-panel-body">
              <Field label={t(locale, 'modelId')}>
                <TextInput
                  size="xs"
                  value={rename}
                  onChange={event => setRename(event.currentTarget.value)}
                  rightSection={<Button size="compact-xs" variant="subtle" loading={saving} onClick={() => void saveName()}>{t(locale, 'save')}</Button>}
                />
              </Field>
              <div className="console-note">
                {t(locale, 'renameModelHint')}
              </div>
            </div>
          </section>
          <section className="console-panel">
            <div className="console-panel-head"><div className="console-panel-title">{t(locale, 'dangerZone')}</div><span className="console-panel-sub">{t(locale, 'irreversible')}</span></div>
            <div className="console-panel-body">
              <div className="console-note warn">
                {t(locale, 'boundKeysNote', String(boundKeys.length))}
              </div>
              <div className="console-note warn">
                {t(locale, 'deleteHistoryLead')}
              </div>
              <div className="console-route-actions">
                <Button size="xs" color="red" leftSection={<IconTrash size={13} />} onClick={() => setDeleteModelOpen(true)}>{t(locale, 'deleteVirtualModel')}</Button>
              </div>
            </div>
          </section>
        </div>
      )}

      <Modal
        opened={backendModalOpen}
        onClose={() => setBackendModalOpen(false)}
        title={editingBackend ? t(locale, 'editBackend') : t(locale, 'addBackend')}
        size="lg"
        closeOnClickOutside={false}
      >
        <div className="console-form-grid">
          <Field label="PLATFORM">
            <Select
              size="xs"
              data={platforms.map(platform => ({ value: platform.id, label: platform.name }))}
              value={draft.platform_id}
              onChange={value => setDraft(prev => ({ ...prev, platform_id: value || '', model_id: '' }))}
              placeholder={t(locale, 'selectPlatform')}
              searchable
            />
          </Field>
          <Field label={t(locale, 'modelId')}>
            <Autocomplete
              size="xs"
              data={modelSuggestions}
              value={draft.model_id}
              onChange={value => setDraft(prev => ({ ...prev, model_id: value }))}
              placeholder={t(locale, 'selectOrTypeModelId')}
            />
          </Field>
          <Field label={t(locale, 'weightLabel')}>
            <NumberInput size="xs" min={1} value={draft.weight} onChange={value => setDraft(prev => ({ ...prev, weight: Number(value) || 1 }))} />
          </Field>
          <Field label={t(locale, 'priorityLabel')}>
            <NumberInput size="xs" min={0} value={draft.priority} onChange={value => setDraft(prev => ({ ...prev, priority: Number(value) || 0 }))} />
          </Field>
          <Field full label={t(locale, 'capabilities')}>
            <MultiSelect size="xs" data={CAP_OPTIONS} value={draft.capabilities} onChange={value => setDraft(prev => ({ ...prev, capabilities: value }))} />
          </Field>
          <Field label={t(locale, 'reasoningStyleLabel')}>
            <Select size="xs" data={REASONING_OPTIONS.map(o => ({ ...o, label: optionLabel(REASONING_LABEL_KEYS, o.value, locale) }))} value={draft.reasoning_style} onChange={value => setDraft(prev => ({ ...prev, reasoning_style: value || 'Auto' }))} allowDeselect={false} />
          </Field>
          <Field label={t(locale, 'reasoningEchoLabel')}>
            <Select size="xs" data={ECHO_OPTIONS.map(o => ({ ...o, label: optionLabel(ECHO_LABEL_KEYS, o.value, locale) }))} value={draft.reasoning_echo} onChange={value => setDraft(prev => ({ ...prev, reasoning_echo: value || 'Drop' }))} allowDeselect={false} />
          </Field>
          <Field label={t(locale, 'unknownParamsLabel')}>
            <Select size="xs" data={UNKNOWN_OPTIONS.map(o => ({ ...o, label: optionLabel(UNKNOWN_LABEL_KEYS, o.value, locale) }))} value={draft.unknown_params} onChange={value => setDraft(prev => ({ ...prev, unknown_params: value || 'Auto' }))} allowDeselect={false} />
          </Field>
          <Field full label={t(locale, 'paramOverridesLabel')}>
            <Textarea
              size="xs"
              autosize
              minRows={3}
              maxRows={8}
              value={draft.param_overrides}
              onChange={event => setDraft(prev => ({ ...prev, param_overrides: event.currentTarget.value }))}
              placeholder='{"temperature": 0.2, "max_tokens": 8192}'
            />
          </Field>
        </div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setBackendModalOpen(false)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" loading={saving} onClick={() => void saveBackend()}>{t(locale, 'save')}</Button>
        </div>
      </Modal>

      <Modal
        opened={Boolean(deleteBackendTarget)}
        onClose={() => setDeleteBackendTarget(null)}
        title={t(locale, 'deleteBackendHeading')}
        size="sm"
      >
        <div className="console-note warn">{t(locale, 'deleteBackendWarning')}</div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setDeleteBackendTarget(null)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" loading={saving} onClick={() => void deleteSelectedBackend()}>{t(locale, 'delete')}</Button>
        </div>
      </Modal>

      <Modal
        opened={deleteModelOpen}
        onClose={() => setDeleteModelOpen(false)}
        title={t(locale, 'deleteVirtualModel')}
        size="sm"
      >
        <div className="console-note warn">
          {boundKeys.length > 0
            ? t(locale, 'boundKeysWarn', String(boundKeys.length))
            : t(locale, 'deleteModelIrreversible')}
        </div>
        <div className="console-note warn">
          {impact === undefined
            ? t(locale, 'deleteHistoryChecking')
            : impact.requests > 0
              ? t(locale, 'deleteHistoryImpact', String(impact.requests), String(impact.attempts))
              : t(locale, 'deleteHistoryEmpty')}
        </div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setDeleteModelOpen(false)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" loading={saving} onClick={() => void removeModel()}>{t(locale, 'confirmDelete')}</Button>
        </div>
      </Modal>
    </div>
  )
}
