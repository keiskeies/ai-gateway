import { useEffect, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ActionIcon, Button, Checkbox, Group, Modal, MultiSelect, PasswordInput, Select, Table, TextInput, Tooltip } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import {
  IconArrowLeft,
  IconCopy,
  IconDownload,
  IconExternalLink,
  IconPencil,
  IconPlus,
  IconPlugConnected,
  IconRefresh,
  IconTrash,
  IconUpload,
  IconX,
} from '@tabler/icons-react'
import { useNavigate, useParams } from 'react-router-dom'
import { batchUpdatePlatformModels, deletePlatform, exportPlatformPrices, getPlatform, getPlatformCredits, getPlatformHealth, getPlatformStats, adjustPlatformCredit, resetPlatformCredit, listAllRoutes, listModels, listProxies, syncPlatformModels, testPlatformConfig, updateModel, updatePlatform } from '../api'
import { rangeToParams, useAppState } from '../app/AppState'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import type { Locale } from '../i18n'
import { CAPABILITY_OPTIONS, getCapabilityLabel } from '../presets'
import { PageHeader } from '../components/ui/PageHeader'
import { ImportModelsModal } from '../components/ImportModelsModal'
import { ImportPricesModal } from '../components/ImportPricesModal'
import { EmptyState, ErrorState, PageSkeleton } from '../components/ui/States'
import { Readout } from '../components/ui/Readout'
import { StatusPill } from '../components/ui/StatusPill'
import { formatCompact, formatCost, formatCostCoverage, formatDuration, formatLogTime, formatNumber, successRateTone } from '../lib/format'
import { PLATFORM_TYPE_LABELS, platformTypeLabel } from '../lib/platform'
import type { PlatformCreditUsage, PlatformDetail, PlatformHealthRecord, PlatformStats, ProxyRow, RouteRow } from '../types/stats'
import { Field } from '../components/Field'

interface LinkedModel {
  proxy: ProxyRow
  strategy: string
  backends: { id: string; platform_id?: string; model_id?: string; weight?: number }[]
}

// 已同步进库的平台模型：远端列表只给 ID，价格/上下文/能力/显示名都要在这里人工补
interface SyncedModel {
  id: string
  platform_id: string
  model_id: string
  display_name: string
  max_tokens: number
  context_window: number
  input_price: number | null
  output_price: number | null
  credit_price: number | null
  capabilities: string[]
  status: string
}

interface ModelDraft {
  display_name: string
  max_tokens: string
  context_window: string
  input_price: string
  output_price: string
  credit_price: string
  capabilities: string[]
  status: string
}

// 批量编辑：每个字段带一个开关，关着＝这个字段不进 body（后端＝不改），
// 开着且留空＝发 null＝清空。没有"留空到底是改还是不改"的第三种可能。
interface BatchDraft {
  input_price_on: boolean
  input_price: string
  output_price_on: boolean
  output_price: string
  credit_price_on: boolean
  credit_price: string
  capabilities_on: boolean
  capabilities: string[]
}

const emptyBatch = (): BatchDraft => ({
  input_price_on: false,
  input_price: '',
  output_price_on: false,
  output_price: '',
  credit_price_on: false,
  credit_price: '',
  capabilities_on: false,
  capabilities: [],
})

const STATUS_DATA = (locale: Locale) => [
  { value: 'Active', label: t(locale, 'modelStatusActive') },
  { value: 'Disabled', label: t(locale, 'modelStatusDisabled') },
]

const CAP_DATA = (locale: Locale) => CAPABILITY_OPTIONS.map(cap => ({
  value: cap.value,
  label: locale === 'zh' ? cap.labelZh : cap.labelEn,
}))

const modelDraftFrom = (row: SyncedModel): ModelDraft => ({
  display_name: row.display_name,
  max_tokens: String(row.max_tokens),
  context_window: String(row.context_window),
  input_price: row.input_price == null ? '' : String(row.input_price),
  output_price: row.output_price == null ? '' : String(row.output_price),
  credit_price: row.credit_price == null ? '' : String(row.credit_price),
  capabilities: row.capabilities,
  status: row.status,
})

// 数字字段：空 = 清空（落 NULL），非数字/负数 = 无效（保存前拦下，绝不静默当成"没改"）
const parseNumberField = (raw: string): number | null => {
  const trimmed = raw.trim()
  if (!trimmed) return null
  return Number(trimmed)
}

const isInvalidPrice = (raw: string): boolean => {
  const value = parseNumberField(raw)
  return value !== null && (!Number.isFinite(value) || value < 0)
}

// max_tokens / context_window 这两列不允许 NULL，所以空值也算无效
const isInvalidTokens = (raw: string): boolean => {
  const value = parseNumberField(raw)
  return value === null || !Number.isFinite(value) || value < 1
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

interface PlatformDraft {
  name: string
  type: string
  base_url: string
  api_key: string
  organization: string
  credit_budget: string
  credit_window: string
}

const emptyDraft: PlatformDraft = { name: '', type: 'OpenAI', base_url: '', api_key: '', organization: '', credit_budget: '', credit_window: 'day' }

const draftFrom = (platform: PlatformDetail): PlatformDraft => ({
  name: platform.name,
  type: platform.type,
  base_url: platform.base_url,
  api_key: '',
  organization: platform.organization || '',
  credit_budget: platform.credit_budget == null ? '' : String(platform.credit_budget),
  credit_window: platform.credit_window || 'day',
})

interface HeaderRow {
  name: string
  value: string
}

// custom_headers 是扁平 JSON object；转成可编辑的行，非字符串值按字符串显示
const headerRowsFrom = (platform: PlatformDetail): HeaderRow[] =>
  Object.entries((platform.custom_headers as Record<string, unknown>) ?? {}).map(([name, value]) => ({
    name,
    value: value == null ? '' : String(value),
  }))

// 空行名丢弃；HTTP 头名大小写不敏感，所以按小写比较去重
const headersToPayload = (rows: HeaderRow[]): Record<string, string> => {
  const out: Record<string, string> = {}
  for (const row of rows) {
    const name = row.name.trim()
    if (name) out[name] = row.value.trim()
  }
  return out
}

const sameHeaders = (a: Record<string, string>, b: Record<string, string>): boolean => {
  const keys = (x: Record<string, string>) => Object.keys(x).sort().map(k => `${k.toLowerCase()}=${x[k]}`).join('\n')
  return keys(a) === keys(b)
}

const typeOptions = Object.entries(PLATFORM_TYPE_LABELS).map(([value, label]) => ({ value, label }))

const creditWindowOptions = (locale: Locale) => [
  { value: 'day', label: t(locale, 'creditWindowDay') },
  { value: 'week', label: t(locale, 'creditWindowWeek') },
  { value: 'month', label: t(locale, 'creditWindowMonth') },
]

export default function ProviderDetail() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { locale } = useAppContext()
  const { range } = useAppState()
  const [testing, setTesting] = useState(false)
  const [remoteModels, setRemoteModels] = useState<string[] | null>(null)
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string; at: number } | null>(null)
  const [draft, setDraft] = useState<PlatformDraft>(emptyDraft)
  // 转发时附加的自定义 Header：可编辑行，保存时压成扁平 JSON object
  const [headerRows, setHeaderRows] = useState<HeaderRow[]>([])
  const [saving, setSaving] = useState(false)
  const [syncing, setSyncing] = useState(false)
  const [importOpen, setImportOpen] = useState(false)
  const [pricesOpen, setPricesOpen] = useState(false)
  const [exporting, setExporting] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  // 已同步模型的行内编辑：一次只开一行
  const [editTarget, setEditTarget] = useState<SyncedModel | null>(null)
  const [modelDraft, setModelDraft] = useState<ModelDraft | null>(null)
  // 批量编辑：选中的是 models 表行 id，跨刷新保留，但会被下面的 effect 剪掉已不存在的行
  const [selected, setSelected] = useState<string[]>([])
  const [batchDraft, setBatchDraft] = useState<BatchDraft | null>(null)
  const [batchSaving, setBatchSaving] = useState(false)
  const [creditOpen, setCreditOpen] = useState(false)
  const [creditDelta, setCreditDelta] = useState('')
  const [creditNote, setCreditNote] = useState('')
  const [creditSaving, setCreditSaving] = useState(false)

  const platformQuery = useQuery({
    queryKey: ['platform', id],
    queryFn: async (): Promise<PlatformDetail> => getPlatform(id!) as Promise<PlatformDetail>,
    enabled: Boolean(id),
  })

  const statsQuery = useQuery({
    queryKey: ['platform-stats', id, range],
    queryFn: async (): Promise<PlatformStats> => getPlatformStats(id!, rangeToParams(range)) as Promise<PlatformStats>,
    enabled: Boolean(id),
  })

  const creditsQuery = useQuery({
    queryKey: ['platform-credits'],
    queryFn: async (): Promise<PlatformCreditUsage[]> => getPlatformCredits() as Promise<PlatformCreditUsage[]>,
  })
  const creditUsage = creditsQuery.data?.find(row => row.platform_id === id) ?? null

  // 连通检测历史：每次测试连接由后端落一行，刷新页面后仍看得到
  const healthQuery = useQuery({
    queryKey: ['platform-health', id],
    queryFn: async (): Promise<PlatformHealthRecord[]> => getPlatformHealth(id!) as Promise<PlatformHealthRecord[]>,
    enabled: Boolean(id),
  })

  const linkedQuery = useQuery({
    queryKey: ['platform-linked-models', id],
    queryFn: async (): Promise<LinkedModel[]> => {
      const [proxies, routes] = await Promise.all([
        listProxies() as Promise<ProxyRow[]>,
        listAllRoutes().catch(() => []) as Promise<RouteRow[]>,
      ])
      return proxies
        .map(proxy => {
          const route = routes.find(item => item.proxy_id === proxy.id)
          return {
            proxy,
            strategy: route?.lb_strategy ?? '—',
            backends: (route?.backends ?? []).filter(backend => backend.platform_id === id),
          }
        })
        .filter(item => item.backends.length > 0)
    },
    enabled: Boolean(id),
  })

  useEffect(() => {
    if (platformQuery.data) {
      setDraft(draftFrom(platformQuery.data))
      setHeaderRows(headerRowsFrom(platformQuery.data))
    }
  }, [platformQuery.data])

  // 本平台已入库的模型：一次 GET /api/models 再按平台过滤，不给每个平台各发一次请求
  const syncedQuery = useQuery({
    queryKey: ['platform-models', id],
    queryFn: async (): Promise<SyncedModel[]> => {
      const rows = (await listModels()) as SyncedModel[]
      return rows.filter(row => row.platform_id === id)
    },
    enabled: Boolean(id),
  })

  // 列表刷新后剪掉已经不存在的行：留着一条被删掉的 id，批量保存会被后端当成"不属于本平台"挡回来
  useEffect(() => {
    const rows = syncedQuery.data
    if (!rows) return
    setSelected(prev => (prev.length ? prev.filter(id => rows.some(row => row.id === id)) : prev))
  }, [syncedQuery.data])

  const toggleSelected = (rowId: string) =>    setSelected(prev => (prev.includes(rowId) ? prev.filter(value => value !== rowId) : [...prev, rowId]))

  const batchTouched = Boolean(batchDraft)
    && (batchDraft!.input_price_on || batchDraft!.output_price_on || batchDraft!.credit_price_on || batchDraft!.capabilities_on)
  const batchInvalid = Boolean(batchDraft)
    && (isInvalidPrice(batchDraft!.input_price) || isInvalidPrice(batchDraft!.output_price) || isInvalidPrice(batchDraft!.credit_price))

  const saveBatch = async () => {
    if (!id || !batchDraft || !batchTouched || batchInvalid) return
    setBatchSaving(true)
    try {
      const result = await batchUpdatePlatformModels(id, {
        model_ids: selected,
        // 开关关着＝这个字段整个不出现在 body 里＝后端保持原值；开着且留空＝发 null＝清空
        ...(batchDraft.input_price_on ? { input_price: parseNumberField(batchDraft.input_price) } : {}),
        ...(batchDraft.output_price_on ? { output_price: parseNumberField(batchDraft.output_price) } : {}),
        ...(batchDraft.credit_price_on ? { credit_price: parseNumberField(batchDraft.credit_price) } : {}),
        ...(batchDraft.capabilities_on ? { capabilities: batchDraft.capabilities } : {}),
      })
      await queryClient.invalidateQueries({ queryKey: ['platform-models', id] })
      // 虚拟模型的能力徽标是这些行 capabilities 的交集，改能力必须一起刷
      await queryClient.invalidateQueries({ queryKey: ['models'] })
      await queryClient.invalidateQueries({ queryKey: ['platform-linked-models', id] })
      setSelected([])
      setBatchDraft(null)
      notifications.show({
        message: t(locale, 'batchModelsDone', String(result?.updated ?? selected.length)),
        color: 'green',
      })
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'updateFailed'),
        message: error?.response?.data?.error?.message || '',
        color: 'red',
      })
    } finally {
      setBatchSaving(false)
    }
  }

  // 用表单里的当前值测试（还没保存的那份），Key 留空时后端用库里的凭据，Key 本身不回传
  const testConnection = async () => {
    if (!id || !draft.base_url.trim()) return
    setTesting(true)
    try {
      const result = await testPlatformConfig({
        type: draft.type,
        base_url: draft.base_url.trim(),
        api_key: draft.api_key.trim() || undefined,
        platform_id: id,
        custom_headers: headersDraft,
      })
      const models = (result?.models ?? []).map((model: { id: string }) => model.id)
      const latency = Math.round(result?.latency_ms ?? 0)
      const message = result?.ok
        ? (models.length > 0
            ? t(locale, 'testOkModels', String(models.length), String(latency))
            : t(locale, 'testOkNoList', String(latency)))
        : (result?.error || t(locale, 'testFailedGeneric'))
      setRemoteModels(result?.ok ? models : null)
      setTestResult({ ok: Boolean(result?.ok), message, at: Date.now() })
      notifications.show({ message, color: result?.ok ? 'green' : 'red' })
      void queryClient.invalidateQueries({ queryKey: ['platform-health', id] })
    } catch (error: any) {
      const message = error?.response?.data?.error?.message || t(locale, 'testFailedGeneric')
      setRemoteModels(null)
      setTestResult({ ok: false, message, at: Date.now() })
      notifications.show({ message, color: 'red' })
    } finally {
      setTesting(false)
    }
  }

  // 同步入库只补远端有而库里没有的模型，已有行（价格/能力/显示名）不动，也不删除
  const runSync = async () => {
    if (!id) return
    setSyncing(true)
    try {
      const result = await syncPlatformModels(id)
      if (result?.ok) {
        notifications.show({
          message: t(locale, 'syncModelsDone', String(result.created ?? 0), String(result.existing ?? 0)),
          color: 'green',
        })
        await queryClient.invalidateQueries({ queryKey: ['platform-linked-models', id] })
        await queryClient.invalidateQueries({ queryKey: ['platforms'] })
        await queryClient.invalidateQueries({ queryKey: ['platform', id] })
        await queryClient.invalidateQueries({ queryKey: ['platform-models', id] })
      } else {
        notifications.show({ message: t(locale, 'syncModelsFailed', result?.error || ''), color: 'red' })
      }
    } catch (error: any) {
      notifications.show({
        message: t(locale, 'syncModelsFailed', error?.response?.data?.error?.message || ''),
        color: 'red',
      })
    } finally {
      setSyncing(false)
    }
  }

  // 导出本平台当前的定价表：三列，正是导入吃的格式。改完数字再传回来就行，
  // 不必先猜列名再手工拼一份文件。
  const exportPrices = async () => {
    if (!id) return
    setExporting(true)
    try {
      const resp = await exportPlatformPrices(id)
      const url = URL.createObjectURL(resp.data as Blob)
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = `aigw-prices-${platformQuery.data?.name ?? id}.csv`
      anchor.click()
      URL.revokeObjectURL(url)
      notifications.show({
        message: t(locale, 'pricesExported', String(resp.headers?.['x-export-rows'] ?? synced.length)),
        color: 'green',
      })
    } catch {
      notifications.show({ message: t(locale, 'pricesExportFailed'), color: 'red' })
    } finally {
      setExporting(false)
    }
  }

  const openModelEditor = (row: SyncedModel) => {
    setEditTarget(row)
    setModelDraft(modelDraftFrom(row))
  }

  const closeModelEditor = () => {
    setEditTarget(null)
    setModelDraft(null)
  }

  const modelDirty = Boolean(editTarget && modelDraft)
    && JSON.stringify(modelDraftFrom(editTarget!)) !== JSON.stringify(modelDraft)

  const modelInvalid = Boolean(modelDraft) && (
    !modelDraft!.display_name.trim()
    || isInvalidTokens(modelDraft!.max_tokens)
    || isInvalidTokens(modelDraft!.context_window)
    || isInvalidPrice(modelDraft!.input_price)
    || isInvalidPrice(modelDraft!.output_price)
    || isInvalidPrice(modelDraft!.credit_price)
  )

  const saveModel = async () => {
    if (!editTarget || !modelDraft || modelInvalid) return
    setSaving(true)
    try {
      await updateModel(editTarget.id, {
        display_name: modelDraft.display_name.trim(),
        max_tokens: Number(modelDraft.max_tokens),
        context_window: Number(modelDraft.context_window),
        // 空串走 null＝清空；字段始终整份提交，所以后端"缺省＝不改"的三态不会被前端误触发
        input_price: parseNumberField(modelDraft.input_price),
        output_price: parseNumberField(modelDraft.output_price),
        credit_price: parseNumberField(modelDraft.credit_price),
        capabilities: modelDraft.capabilities,
        status: modelDraft.status,
      })
      await queryClient.invalidateQueries({ queryKey: ['platform-models', id] })
      // 虚拟模型的能力徽标就是这些行 capabilities 的交集，改完要一起刷
      await queryClient.invalidateQueries({ queryKey: ['models'] })
      closeModelEditor()
      notifications.show({ message: t(locale, 'updateSuccess'), color: 'green' })
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'updateFailed'),
        message: error?.response?.data?.error?.message || '',
        color: 'red',
      })
    } finally {
      setSaving(false)
    }
  }

  const refreshAll = async () => {
    await queryClient.invalidateQueries({ queryKey: ['platform', id] })
    await queryClient.invalidateQueries({ queryKey: ['platform-stats', id] })
    await queryClient.invalidateQueries({ queryKey: ['platform-linked-models', id] })
    await queryClient.invalidateQueries({ queryKey: ['platform-models', id] })
    await queryClient.invalidateQueries({ queryKey: ['platform-health', id] })
  }

  const current = platformQuery.data
  const headersDraft = headersToPayload(headerRows)
  const headerNames = headerRows.map(row => row.name.trim().toLowerCase()).filter(Boolean)
  const dupHeader = headerNames.length !== new Set(headerNames).size
  const creditBudgetInvalid = draft.credit_budget.trim() !== ''
    && (!Number.isFinite(Number(draft.credit_budget)) || Number(draft.credit_budget) <= 0)
  const dirty = Boolean(current) && (
    draft.name !== current!.name
    || draft.type !== current!.type
    || draft.base_url !== current!.base_url
    || (draft.organization || '') !== (current!.organization || '')
    || draft.api_key !== ''
    || draft.credit_budget !== (current!.credit_budget == null ? '' : String(current!.credit_budget))
    || (draft.credit_budget.trim() !== '' && draft.credit_window !== (current!.credit_window || 'day'))
    || !sameHeaders(headersDraft, headersToPayload(headerRowsFrom(current!)))
  )
  const invalid = !draft.name.trim() || !draft.base_url.trim() || dupHeader || creditBudgetInvalid
  const creditWindowLabel = current?.credit_window === 'week'
    ? t(locale, 'creditWindowWeek')
    : current?.credit_window === 'month'
      ? t(locale, 'creditWindowMonth')
      : t(locale, 'creditWindowDay')

  const setField = <K extends keyof PlatformDraft>(key: K, value: PlatformDraft[K]) => {
    setDraft(prev => ({ ...prev, [key]: value }))
  }

  const setHeaderRow = (index: number, patch: Partial<HeaderRow>) => {
    setHeaderRows(prev => prev.map((row, i) => (i === index ? { ...row, ...patch } : row)))
  }

  const resetDraft = () => {
    if (current) {
      setDraft(draftFrom(current))
      setHeaderRows(headerRowsFrom(current))
    }
  }

  const save = async () => {
    if (!id || invalid) return
    setSaving(true)
    try {
      const payload: Record<string, unknown> = {
        name: draft.name.trim(),
        type: draft.type,
        base_url: draft.base_url.trim(),
        organization: draft.organization.trim(),
        custom_headers: headersDraft,
        // 空预算 = 关闭积分；窗口只在有预算时提交，避免留下“有窗口没预算”的半套配置
        credit_budget: draft.credit_budget.trim() === '' ? null : Number(draft.credit_budget),
        credit_window: draft.credit_budget.trim() === '' ? null : draft.credit_window,
      }
      // 后端用 Option 区分“不改”和“改成空”，留空表示保留现有密钥
      if (draft.api_key) payload.api_key = draft.api_key.trim()
      await updatePlatform(id, payload)
      notifications.show({ message: t(locale, 'updateSuccess'), color: 'green' })
      await queryClient.invalidateQueries({ queryKey: ['platform', id] })
      await queryClient.invalidateQueries({ queryKey: ['platforms'] })
      await queryClient.invalidateQueries({ queryKey: ['platform-credits'] })
      await queryClient.invalidateQueries({ queryKey: ['platform-linked-models', id] })
      await queryClient.invalidateQueries({ queryKey: ['models'] })
      await queryClient.invalidateQueries({ queryKey: ['overview'] })
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'updateFailed'),
        message: error?.response?.data?.error?.message || '',
        color: 'red',
      })
    } finally {
      setSaving(false)
    }
  }

  const onAdjustCredit = async () => {
    if (!id) return
    const delta = Number(creditDelta)
    if (!Number.isFinite(delta) || delta === 0 || Math.abs(delta) > 1_000_000) return
    setCreditSaving(true)
    try {
      await adjustPlatformCredit(id, delta, creditNote.trim() || undefined)
      await queryClient.invalidateQueries({ queryKey: ['platform-credits'] })
      setCreditOpen(false)
      setCreditDelta('')
      setCreditNote('')
      notifications.show({ message: t(locale, 'creditAdjustDone'), color: 'green' })
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'updateFailed'),
        message: error?.response?.data?.error?.message || '',
        color: 'red',
      })
    } finally {
      setCreditSaving(false)
    }
  }

  const onResetCredit = async () => {
    if (!id) return
    setCreditSaving(true)
    try {
      await resetPlatformCredit(id)
      await queryClient.invalidateQueries({ queryKey: ['platform-credits'] })
      setCreditOpen(false)
      notifications.show({ message: t(locale, 'creditResetDone'), color: 'green' })
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'updateFailed'),
        message: error?.response?.data?.error?.message || '',
        color: 'red',
      })
    } finally {
      setCreditSaving(false)
    }
  }

  const remove = async () => {
    if (!id) return
    setSaving(true)
    try {
      await deletePlatform(id)
      notifications.show({ message: t(locale, 'deleteSuccess'), color: 'green' })
      await queryClient.invalidateQueries({ queryKey: ['platforms'] })
      await queryClient.invalidateQueries({ queryKey: ['models'] })
      await queryClient.invalidateQueries({ queryKey: ['overview'] })
      navigate('/providers')
    } catch (error: any) {
      notifications.show({
        message: error?.response?.data?.error?.message || t(locale, 'updateFailed'),
        color: 'red',
      })
      setSaving(false)
      setDeleteOpen(false)
    }
  }

  if (platformQuery.isPending) return <PageSkeleton rows={5} />
  if (platformQuery.isError || !platformQuery.data) return <ErrorState onRetry={() => void platformQuery.refetch()} />

  const platform = platformQuery.data
  const stats = statsQuery.data
  const linked = linkedQuery.data ?? []
  const health = healthQuery.data ?? []
  const synced = syncedQuery.data ?? []
  const allSelected = synced.length > 0 && selected.length === synced.length
  const toggleAllSelected = () => setSelected(allSelected ? [] : synced.map(row => row.id))
  const tone = successRateTone(stats?.success_rate ?? 0, (stats?.total_requests ?? 0) > 0)
  const totalTokens = (stats?.total_token_input ?? 0) + (stats?.total_token_output ?? 0)
  const lastTest = testResult ? new Date(testResult.at).toLocaleTimeString(locale === 'zh' ? 'zh-CN' : 'en-US', { hour12: false }) : '—'
  const typeLabel = platformTypeLabel(platform.type)

  return (
    <div className="console-page">
      <PageHeader
        kicker={<>{t(locale, 'kickerConfigure')} <span className="console-kicker-sep" /> {t(locale, 'kickerProvider')}</>}
        title={platform.name}
        subtitle={`${typeLabel} · ${platform.base_url}`}
        actions={
          <>
            <Button size="xs" variant="default" leftSection={<IconArrowLeft size={14} />} onClick={() => navigate('/providers')}>
              {t(locale, 'backToList')}
            </Button>
            <Tooltip label={t(locale, 'testUsesDraft')}>
              <Button size="xs" leftSection={<IconPlugConnected size={14} />} loading={testing} disabled={!draft.base_url.trim()} onClick={() => void testConnection()}>
                {t(locale, 'testConfig')}
              </Button>
            </Tooltip>
          </>
        }
      />

      <div className="console-status-strip">
        <span className={`console-dot ${testResult ? (testResult.ok ? 'ok' : 'err') : 'idle'}`} />
        <strong>{typeLabel}</strong>
        <span className="console-status-sep">·</span>
        <span>{platform.base_url}</span>
        <span className="console-status-right">
          <span>{t(locale, 'lastTest')} {lastTest}</span>
          <Button size="compact-xs" variant="subtle" leftSection={<IconRefresh size={12} />} onClick={() => void refreshAll()}>
            {t(locale, 'refresh')}
          </Button>
        </span>
      </div>

      <div className="console-readouts">
        <Readout label={t(locale, 'requestsLabel')} value={formatNumber(stats?.total_requests ?? 0)} meta={<span>{range.toUpperCase()}</span>} />
        <Readout
          label={t(locale, 'successLabel')}
          value={(stats?.success_rate ?? 0).toFixed(1)}
          unit="%"
          meta={<StatusPill tone={tone}>{t(locale, tone === 'ok' ? 'healthHealthy' : tone === 'warn' ? 'healthDegraded' : tone === 'err' ? 'healthFailing' : 'healthIdle')}</StatusPill>}
        />
        <Readout label={t(locale, 'avgLatencyLabel')} value={stats?.total_requests ? formatDuration(stats.avg_latency_ms) : '—'} meta={<span>{platform.status === 'Active' ? t(locale, 'modelStatusActive') : t(locale, 'modelStatusDisabled')}</span>} />
        <Readout label={t(locale, 'tokensLabel')} value={formatCompact(totalTokens)} meta={<><span>{formatCompact(stats?.total_token_input ?? 0)} {t(locale, 'inShort')} · {formatCompact(stats?.total_token_output ?? 0)} {t(locale, 'outShort')}</span><Tooltip withArrow label={t(locale, 'costCoverageHint', String(stats?.priced_requests ?? 0), String(stats?.total_requests ?? 0))}><span>{t(locale, 'cost')} {formatCost(stats?.total_cost)} · {formatCostCoverage(stats?.priced_requests, stats?.total_requests)}</span></Tooltip></>} />
      </div>

      <div className="console-provider-grid">
        <section className="console-panel">
          <div className="console-panel-head">
            <div className="console-panel-title">{t(locale, 'providerCreds')}</div>
            <span className="console-panel-sub">{dirty ? t(locale, 'unsaved') : platform.status === 'Active' ? t(locale, 'modelStatusActive') : t(locale, 'modelStatusDisabled')}</span>
          </div>
          <div className="console-panel-body">
            <div className="console-form-grid">
              <Field label={t(locale, 'platformName').toUpperCase()}>
                <TextInput size="xs" value={draft.name} onChange={event => setField('name', event.currentTarget.value)} />
              </Field>
              <Field label={t(locale, 'platformType').toUpperCase()}>
                <Select size="xs" data={typeOptions} value={draft.type} onChange={value => setField('type', value || 'OpenAI')} allowDeselect={false} />
              </Field>
              <Field full label={t(locale, 'baseUrl').toUpperCase()}>
                <TextInput
                  size="xs"
                  classNames={{ input: 'console-mono' }}
                  value={draft.base_url}
                  onChange={event => setField('base_url', event.currentTarget.value)}
                  placeholder="https://api.openai.com/v1"
                />
              </Field>
              <Field label={t(locale, 'apiKey').toUpperCase()}>
                <PasswordInput
                  size="xs"
                  value={draft.api_key}
                  onChange={event => setField('api_key', event.currentTarget.value)}
                  placeholder={platform.api_key || t(locale, 'keyNotSet')}
                  description={t(locale, 'keyKeepHint')}
                />
              </Field>
              <Field label={t(locale, 'organization').toUpperCase()}>
                <TextInput size="xs" value={draft.organization} onChange={event => setField('organization', event.currentTarget.value)} placeholder="org-xxx" />
              </Field>
              <Field label={t(locale, 'creditBudget').toUpperCase()} unit={t(locale, 'creditUnit')}>
                <TextInput
                  size="xs"
                  inputMode="decimal"
                  value={draft.credit_budget}
                  onChange={event => setField('credit_budget', event.currentTarget.value)}
                  placeholder="250"
                />
              </Field>
              <Field label={t(locale, 'creditWindow').toUpperCase()}>
                <Select
                  size="xs"
                  data={creditWindowOptions(locale)}
                  value={draft.credit_window}
                  onChange={value => setField('credit_window', value || 'day')}
                  disabled={!draft.credit_budget.trim()}
                  allowDeselect={false}
                />
              </Field>
              <div className="console-field console-form-full">
                <div className="console-note">{t(locale, 'creditBudgetHint')}</div>
                {creditBudgetInvalid && <div className="console-note warn">{t(locale, 'creditBudgetInvalid')}</div>}
              </div>
            </div>
            <div className="console-field console-form-full">
              <label>{t(locale, 'customHeaders').toUpperCase()}</label>
              {headerRows.map((row, index) => (
                <Group key={index} gap={6} wrap="nowrap" mb={6}>
                  <TextInput
                    size="xs"
                    w={220}
                    classNames={{ input: 'console-mono' }}
                    value={row.name}
                    onChange={event => setHeaderRow(index, { name: event.currentTarget.value })}
                    placeholder="X-User-Id"
                  />
                  <TextInput
                    size="xs"
                    style={{ flex: 1 }}
                    classNames={{ input: 'console-mono' }}
                    value={row.value}
                    onChange={event => setHeaderRow(index, { value: event.currentTarget.value })}
                    placeholder="value"
                  />
                  <ActionIcon
                    size="sm"
                    variant="subtle"
                    color="red"
                    aria-label={t(locale, 'removeHeaderRow')}
                    onClick={() => setHeaderRows(prev => prev.filter((_, i) => i !== index))}
                  >
                    <IconX size={14} />
                  </ActionIcon>
                </Group>
              ))}
              <div className="console-route-actions">
                <Button
                  size="compact-xs"
                  variant="subtle"
                  leftSection={<IconPlus size={12} />}
                  onClick={() => setHeaderRows(prev => [...prev, { name: '', value: '' }])}
                >
                  {t(locale, 'addHeaderRow')}
                </Button>
              </div>
              {dupHeader && <div className="console-note warn">{t(locale, 'headerDup')}</div>}
              <div className="console-note">{t(locale, 'customHeadersHint')}</div>
            </div>
            <div className="console-inspector-field"><span className="k">{t(locale, 'createdAt')}</span><span className="v mono">{new Date(platform.created_at).toLocaleString()}</span></div>
            <div className="console-route-actions">
              <Button size="xs" variant="default" leftSection={<IconCopy size={13} />} onClick={() => {
                void navigator.clipboard.writeText(draft.base_url)
                notifications.show({ message: t(locale, 'copied'), color: 'green' })
              }}>{t(locale, 'copyBaseUrl')}</Button>
              <Button size="xs" variant="default" disabled={!dirty} onClick={resetDraft}>{t(locale, 'discardChanges')}</Button>
              <Button size="xs" loading={saving} disabled={!dirty || invalid} onClick={() => void save()}>{t(locale, 'save')}</Button>
            </div>
          </div>
        </section>

        <section className="console-panel">
          <div className="console-panel-head">
            <div className="console-panel-title">{t(locale, 'creditUsageTitle')}</div>
            <span className="console-panel-sub">
              {platform.credit_budget == null ? t(locale, 'creditNotEnabled') : creditWindowLabel}
            </span>
          </div>
          <div className="console-panel-body">
            {platform.credit_budget == null ? (
              <div className="console-note">{t(locale, 'creditNotEnabledDesc')}</div>
            ) : (
              <>
                <div className="console-meta-grid">
                  <div>
                    <div className="console-meta-label">{t(locale, 'creditRemaining')}</div>
                    <div className="console-meta-value mono">
                      {creditUsage ? `${formatNumber(creditUsage.remaining)} / ${formatNumber(creditUsage.budget)}` : '—'}
                    </div>
                  </div>
                  <div>
                    <div className="console-meta-label">{t(locale, 'creditSpent')}</div>
                    <div className="console-meta-value mono">{creditUsage ? formatNumber(creditUsage.spent) : '—'}</div>
                  </div>
                  <div>
                    <div className="console-meta-label">{t(locale, 'creditResetAt')}</div>
                    <div className="console-meta-value mono">
                      {creditUsage?.reset_at
                        ? new Date(creditUsage.reset_at).toLocaleString(locale === 'zh' ? 'zh-CN' : 'en-US', { hour12: false })
                        : '—'}
                    </div>
                  </div>
                  <div>
                    <div className="console-meta-label">{t(locale, 'creditUnpriced')}</div>
                    <div className="console-meta-value mono">{formatNumber(creditUsage?.unpriced_models ?? 0)}</div>
                  </div>
                </div>
                {creditUsage?.exhausted && <div className="console-note warn">{t(locale, 'creditExhaustedHint')}</div>}
                <div className="console-route-actions">
                  <Button size="xs" variant="default" onClick={() => setCreditOpen(true)}>{t(locale, 'creditAdjust')}</Button>
                  <Button size="xs" variant="default" leftSection={<IconRefresh size={13} />} loading={creditsQuery.isFetching} onClick={() => void creditsQuery.refetch()}>
                    {t(locale, 'refresh')}
                  </Button>
                </div>
              </>
            )}
          </div>
        </section>

        <section className="console-panel">
          <div className="console-panel-head">
            <div className="console-panel-title">{t(locale, 'remoteModelsTitle')}</div>
            <span className="console-panel-sub">{remoteModels ? t(locale, 'modelsCount', String(remoteModels.length)) : t(locale, 'notSynced')}</span>
          </div>
          {testing ? (
            <div className="console-empty"><span>{t(locale, 'connectingPlatform')}</span></div>
          ) : remoteModels ? (
            remoteModels.length > 0 ? (
              <div className="console-table-wrap" style={{ maxHeight: 340 }}>
                <Table className="console-table">
                  <Table.Thead><Table.Tr><Table.Th>{t(locale, 'modelId')}</Table.Th><Table.Th /></Table.Tr></Table.Thead>
                  <Table.Tbody>
                    {remoteModels.map(model => (
                      <Table.Tr key={model}>
                        <Table.Td><span className="console-mono">{model}</span></Table.Td>
                        <Table.Td className="num">
                          <Tooltip label={t(locale, 'copy')}>
                            <Button size="compact-xs" variant="subtle" leftSection={<IconCopy size={12} />} onClick={() => {
                              void navigator.clipboard.writeText(model)
                              notifications.show({ message: t(locale, 'copied'), color: 'green' })
                            }}>{t(locale, 'copy')}</Button>
                          </Tooltip>
                        </Table.Td>
                      </Table.Tr>
                    ))}
                  </Table.Tbody>
                </Table>
              </div>
            ) : (
              <EmptyState title={t(locale, 'noRemoteModels')} description={t(locale, 'noRemoteModelsDesc')} />
            )
          ) : testResult && !testResult.ok ? (
            <EmptyState title={t(locale, 'connectionFailed')} description={testResult.message} action={<Button size="xs" variant="default" onClick={() => void testConnection()}>{t(locale, 'retry')}</Button>} />
          ) : (
            <EmptyState
              title={t(locale, 'noTestYet')}
              description={t(locale, 'noTestYetDesc')}
              action={<Button size="xs" leftSection={<IconPlugConnected size={14} />} onClick={() => void testConnection()}>{t(locale, 'testConfig')}</Button>}
            />
          )}
          <div className="console-panel-foot">
            <span>{testResult?.message || '—'}</span>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <Tooltip label={t(locale, 'syncModelsHint')} w={320}>
                <Button size="compact-xs" variant="default" loading={syncing} onClick={() => void runSync()}>{t(locale, 'syncModels')}</Button>
              </Tooltip>
              <Button size="compact-xs" variant="subtle" onClick={() => void testConnection()}>{t(locale, 'refresh')}</Button>
            </div>
          </div>
        </section>
      </div>

      <section className="console-panel">
        <div className="console-panel-head">
          <div className="console-panel-title">{t(locale, 'syncedModelsTitle')}</div>
          <span className="console-panel-sub">{t(locale, 'rowsCount', String(synced.length))}</span>
        </div>
        {selected.length > 0 && (
          <div className="console-batch-bar">
            <span>{t(locale, 'batchSelected', String(selected.length))}</span>
            <Group gap={6}>
              <Button size="compact-xs" leftSection={<IconPencil size={12} />} onClick={() => setBatchDraft(emptyBatch())}>{t(locale, 'batchEdit')}</Button>
              <Button size="compact-xs" variant="subtle" onClick={() => setSelected([])}>{t(locale, 'batchClear')}</Button>
            </Group>
          </div>
        )}
        {syncedQuery.isPending ? (
          <div className="console-empty"><span>{t(locale, 'loading')}</span></div>
        ) : synced.length > 0 ? (
          <div className="console-table-wrap console-table-scroll" style={{ maxHeight: 420 }}>
            <Table className="console-table">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th className="console-col-check">
                    <Checkbox
                      size="xs"
                      aria-label={t(locale, 'batchSelectAll')}
                      checked={allSelected}
                      indeterminate={selected.length > 0 && !allSelected}
                      onChange={toggleAllSelected}
                    />
                  </Table.Th>
                  <Table.Th>{t(locale, 'modelId')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'thCtx')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'thMax')}</Table.Th>
                  <Table.Th className="num">
                    <Tooltip withArrow w={300} label={t(locale, 'modelPriceColTip')}>
                      <span>{t(locale, 'modelPriceCol')}</span>
                    </Tooltip>
                  </Table.Th>
                  <Table.Th className="num">
                    <Tooltip withArrow w={240} label={t(locale, 'creditPriceHint')}>
                      <span>{t(locale, 'thCreditPrice')}</span>
                    </Tooltip>
                  </Table.Th>
                  <Table.Th>{t(locale, 'thCaps')}</Table.Th>
                  <Table.Th>{t(locale, 'status')}</Table.Th>
                  <Table.Th />
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {synced.map(row => (
                  <Table.Tr key={row.id}>
                    <Table.Td className="console-col-check">
                      <Checkbox
                        size="xs"
                        aria-label={row.model_id}
                        checked={selected.includes(row.id)}
                        onChange={() => toggleSelected(row.id)}
                      />
                    </Table.Td>
                    <Table.Td>
                      <div className="console-strong console-mono">{row.model_id}</div>
                      {row.display_name !== row.model_id && <div className="muted">{row.display_name}</div>}
                    </Table.Td>
                    <Table.Td className="num">{formatNumber(row.context_window)}</Table.Td>
                    <Table.Td className="num">{formatNumber(row.max_tokens)}</Table.Td>
                    <Table.Td className="num">
                      <span className={row.input_price == null && row.output_price == null ? 'muted' : ''}>
                        {`${row.input_price ?? '—'} / ${row.output_price ?? '—'}`}
                      </span>
                    </Table.Td>
                    <Table.Td className="num">
                      <span className={row.credit_price == null ? 'muted' : ''}>{row.credit_price ?? '—'}</span>
                    </Table.Td>
                    <Table.Td>
                      {row.capabilities.length > 0 ? (
                        <span className="console-cap-list">
                          {row.capabilities.map(cap => (
                            <span className="console-cap" key={cap}>{getCapabilityLabel(cap, locale)}</span>
                          ))}
                        </span>
                      ) : (
                        <span className="muted">{t(locale, 'modelCapabilitiesEmpty')}</span>
                      )}
                    </Table.Td>
                    <Table.Td>
                      <StatusPill tone={row.status === 'Active' ? 'ok' : 'idle'}>
                        {row.status === 'Active' ? t(locale, 'modelStatusActive') : t(locale, 'modelStatusDisabled')}
                      </StatusPill>
                    </Table.Td>
                    <Table.Td className="num">
                      <Tooltip label={t(locale, 'editSyncedModel')}>
                        <ActionIcon size="sm" variant="subtle" onClick={() => openModelEditor(row)}>
                          <IconPencil size={14} />
                        </ActionIcon>
                      </Tooltip>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </div>
        ) : (
          <EmptyState
            title={t(locale, 'syncedNoneTitle')}
            description={t(locale, 'syncedNoneDesc')}
            action={
              <Group gap={8}>
                <Button size="xs" leftSection={<IconDownload size={13} />} onClick={() => setImportOpen(true)}>{t(locale, 'importModels')}</Button>
                <Button size="xs" variant="default" loading={syncing} onClick={() => void runSync()}>{t(locale, 'syncModels')}</Button>
              </Group>
            }
          />
        )}
        <div className="console-panel-foot">
          <span>{t(locale, 'syncedModelsFoot')}</span>
          <Group gap={6}>
            <Tooltip label={t(locale, 'pricesExportHint')} w={300}>
              <Button
                size="compact-xs"
                variant="default"
                leftSection={<IconDownload size={12} />}
                loading={exporting}
                disabled={synced.length === 0}
                onClick={() => void exportPrices()}
              >
                {t(locale, 'pricesExport')}
              </Button>
            </Tooltip>
            <Tooltip label={t(locale, 'pricesImportHint')} w={320}>
              <Button size="compact-xs" leftSection={<IconUpload size={12} />} onClick={() => setPricesOpen(true)}>
                {t(locale, 'pricesImport')}
              </Button>
            </Tooltip>
            <Tooltip label={t(locale, 'importModelsHint')} w={320}>
              <Button size="compact-xs" leftSection={<IconUpload size={12} />} onClick={() => setImportOpen(true)}>
                {t(locale, 'importModels')}
              </Button>
            </Tooltip>
          </Group>
        </div>
      </section>

      <section className="console-panel">
        <div className="console-panel-head">
          <div className="console-panel-title">{t(locale, 'linkedModelsTitle')}</div>
          <span className="console-panel-sub">{t(locale, 'modelsCount', String(linked.length))}</span>
        </div>
        {linked.length > 0 ? (
          <div className="console-table-wrap">
            <Table className="console-table">
              <Table.Thead><Table.Tr><Table.Th>{t(locale, 'modelId')}</Table.Th><Table.Th>{t(locale, 'backends')}</Table.Th><Table.Th>{t(locale, 'strategy')}</Table.Th><Table.Th /></Table.Tr></Table.Thead>
              <Table.Tbody>
                {linked.map(item => (
                  <Table.Tr key={item.proxy.id} onClick={() => navigate(`/models/${item.proxy.id}`)} style={{ cursor: 'pointer' }}>
                    <Table.Td><span className="console-strong console-mono">{item.proxy.name}</span></Table.Td>
                    <Table.Td>{item.backends.map(backend => backend.model_id).filter(Boolean).join(' · ')}</Table.Td>
                    <Table.Td>{strategyLabel(item.strategy, locale)}</Table.Td>
                    <Table.Td className="num"><IconExternalLink size={14} /></Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </div>
        ) : (
          <EmptyState title={t(locale, 'linkedNoneTitle')} description={t(locale, 'linkedNoneDesc')} />
        )}
      </section>

      <section className="console-panel">
        <div className="console-panel-head">
          <div className="console-panel-title">{t(locale, 'healthHistory')}</div>
          <span className="console-panel-sub">{health.length} {t(locale, 'checksLabel')}</span>
        </div>
        {health.length > 0 ? (
          <div className="console-table-wrap console-table-scroll">
            <Table className="console-table">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>{t(locale, 'thTime')}</Table.Th>
                  <Table.Th>{t(locale, 'thResult')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'thLatency')}</Table.Th>
                  <Table.Th className="num">{t(locale, 'models')}</Table.Th>
                  <Table.Th>{t(locale, 'thDetail')}</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {health.slice(0, 20).map(row => (
                  <Table.Tr key={row.id}>
                    <Table.Td><span className="console-mono muted">{formatLogTime(row.created_at, locale)}</span></Table.Td>
                    <Table.Td>
                      <StatusPill tone={row.ok ? 'ok' : 'err'}>
                        {row.ok ? t(locale, 'testedOk') : t(locale, 'testedFail')}
                      </StatusPill>
                    </Table.Td>
                    <Table.Td className="num">{formatDuration(row.latency_ms)}</Table.Td>
                    <Table.Td className="num">{row.model_count || '—'}</Table.Td>
                    <Table.Td>
                      <span className={row.ok ? 'muted' : 'err'}>{row.error || '—'}</span>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </div>
        ) : (
          <EmptyState
            title={t(locale, 'healthNone')}
            description={t(locale, 'healthNoneDesc')}
            action={<Button size="xs" leftSection={<IconPlugConnected size={14} />} onClick={() => void testConnection()}>{t(locale, 'testConfig')}</Button>}
          />
        )}
        <div className="console-panel-foot">
          <span>{t(locale, 'healthHistoryDesc')}</span>
        </div>
      </section>

      <section className="console-panel">
        <div className="console-panel-head"><div className="console-panel-title">{t(locale, 'dangerZone')}</div><span className="console-panel-sub">{t(locale, 'irreversible')}</span></div>
        <div className="console-panel-body">
          <div className="console-note warn">
            {linkedQuery.isPending
              ? t(locale, 'dangerLead')
              : linked.length > 0
                ? t(locale, 'providerDeleteWarn', String(linked.length))
                : t(locale, 'providerDeleteSafe')}
          </div>
          <div className="console-route-actions">
            <Button size="xs" color="red" variant="default" leftSection={<IconTrash size={13} />} onClick={() => setDeleteOpen(true)}>{t(locale, 'delete')}</Button>
          </div>
        </div>
      </section>

      <Modal opened={creditOpen} onClose={() => setCreditOpen(false)} title={t(locale, 'creditAdjustTitle')} size="sm">
        <div className="console-note">{t(locale, 'creditAdjustHint')}</div>
        <Field label={t(locale, 'creditAdjustDelta')}>
          <TextInput size="xs" inputMode="decimal" value={creditDelta} onChange={event => setCreditDelta(event.currentTarget.value)} placeholder="-5" />
        </Field>
        <Field label={t(locale, 'creditAdjustNote')}>
          <TextInput size="xs" value={creditNote} onChange={event => setCreditNote(event.currentTarget.value)} placeholder={t(locale, 'creditAdjustNotePlaceholder')} />
        </Field>
        <div className="console-note warn">{t(locale, 'creditResetHint')}</div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" color="red" loading={creditSaving} onClick={() => void onResetCredit()}>
            {t(locale, 'creditReset')}
          </Button>
          <Button
            size="xs"
            loading={creditSaving}
            disabled={!creditDelta.trim() || !Number.isFinite(Number(creditDelta)) || Number(creditDelta) === 0}
            onClick={() => void onAdjustCredit()}
          >
            {t(locale, 'save')}
          </Button>
        </div>
      </Modal>

      <Modal
        opened={Boolean(editTarget && modelDraft)}
        onClose={closeModelEditor}
        title={editTarget ? `${t(locale, 'editSyncedModel')} · ${editTarget.model_id}` : t(locale, 'editSyncedModel')}
        size="lg"
      >
        {modelDraft && (
          <div className="console-form-grid">
            <Field full label={t(locale, 'modelDisplayName')}>
              <TextInput size="xs" value={modelDraft.display_name} onChange={event => setModelDraft(prev => prev && { ...prev, display_name: event.currentTarget.value })} />
            </Field>
            <Field label={t(locale, 'modelContextWindow')}>
              <TextInput size="xs" inputMode="numeric" value={modelDraft.context_window} onChange={event => setModelDraft(prev => prev && { ...prev, context_window: event.currentTarget.value })} />
            </Field>
            <Field label={t(locale, 'modelMaxTokens')}>
              <TextInput size="xs" inputMode="numeric" value={modelDraft.max_tokens} onChange={event => setModelDraft(prev => prev && { ...prev, max_tokens: event.currentTarget.value })} />
            </Field>
            <Field label={t(locale, 'modelInputPrice')} unit={t(locale, 'modelPriceUnit')}>
              <TextInput size="xs" inputMode="decimal" value={modelDraft.input_price} onChange={event => setModelDraft(prev => prev && { ...prev, input_price: event.currentTarget.value })} />
            </Field>
            <Field label={t(locale, 'modelOutputPrice')} unit={t(locale, 'modelPriceUnit')}>
              <TextInput size="xs" inputMode="decimal" value={modelDraft.output_price} onChange={event => setModelDraft(prev => prev && { ...prev, output_price: event.currentTarget.value })} />
            </Field>
            <Field label={t(locale, 'modelCreditPrice')} unit={t(locale, 'creditUnit')}>
              <TextInput size="xs" inputMode="decimal" value={modelDraft.credit_price} onChange={event => setModelDraft(prev => prev && { ...prev, credit_price: event.currentTarget.value })} />
            </Field>
            <Field full label={t(locale, 'modelCapabilities')}>
              <MultiSelect size="xs" data={CAP_DATA(locale)} value={modelDraft.capabilities} onChange={value => setModelDraft(prev => prev && { ...prev, capabilities: value })} />
            </Field>
            <Field label={t(locale, 'modelStatus')}>
              <Select size="xs" data={STATUS_DATA(locale)} value={modelDraft.status} onChange={value => value && setModelDraft(prev => prev && { ...prev, status: value })} />
            </Field>
            <div className="console-field console-form-full">
              <div className="console-note">{t(locale, 'modelPriceHint')}</div>
              {modelInvalid && <div className="console-note warn">{t(locale, 'modelNumberInvalid')}</div>}
            </div>
            <div className="console-inspector-actions console-form-full">
              <Button size="xs" variant="default" onClick={closeModelEditor}>{t(locale, 'cancel')}</Button>
              <Button size="xs" loading={saving} disabled={!modelDirty || modelInvalid} onClick={() => void saveModel()}>{t(locale, 'save')}</Button>
            </div>
          </div>
        )}
      </Modal>

      <Modal opened={Boolean(batchDraft)} onClose={() => setBatchDraft(null)} title={t(locale, 'batchEditTitle')} size="md">
        {batchDraft && (
          <div className="console-form-grid">
            <div className="console-field console-form-full">
              <div className="console-note">{t(locale, 'batchEditDesc', String(selected.length))}</div>
            </div>
            <div className="console-field">
              <Checkbox
                size="xs"
                checked={batchDraft.input_price_on}
                onChange={event => setBatchDraft(prev => prev && { ...prev, input_price_on: event.currentTarget.checked })}
                label={`${t(locale, 'modelInputPrice')} · ${t(locale, 'modelPriceUnit')}`}
              />
              <TextInput size="xs" inputMode="decimal" disabled={!batchDraft.input_price_on} value={batchDraft.input_price} onChange={event => setBatchDraft(prev => prev && { ...prev, input_price: event.currentTarget.value })} />
            </div>
            <div className="console-field">
              <Checkbox
                size="xs"
                checked={batchDraft.output_price_on}
                onChange={event => setBatchDraft(prev => prev && { ...prev, output_price_on: event.currentTarget.checked })}
                label={`${t(locale, 'modelOutputPrice')} · ${t(locale, 'modelPriceUnit')}`}
              />
              <TextInput size="xs" inputMode="decimal" disabled={!batchDraft.output_price_on} value={batchDraft.output_price} onChange={event => setBatchDraft(prev => prev && { ...prev, output_price: event.currentTarget.value })} />
            </div>
            <div className="console-field">
              <Checkbox
                size="xs"
                checked={batchDraft.credit_price_on}
                onChange={event => setBatchDraft(prev => prev && { ...prev, credit_price_on: event.currentTarget.checked })}
                label={`${t(locale, 'modelCreditPrice')} · ${t(locale, 'creditUnit')}`}
              />
              <TextInput size="xs" inputMode="decimal" disabled={!batchDraft.credit_price_on} value={batchDraft.credit_price} onChange={event => setBatchDraft(prev => prev && { ...prev, credit_price: event.currentTarget.value })} />
            </div>
            <div className="console-field console-form-full">
              <Checkbox
                size="xs"
                checked={batchDraft.capabilities_on}
                onChange={event => setBatchDraft(prev => prev && { ...prev, capabilities_on: event.currentTarget.checked })}
                label={t(locale, 'modelCapabilities')}
              />
              <MultiSelect size="xs" data={CAP_DATA(locale)} disabled={!batchDraft.capabilities_on} value={batchDraft.capabilities} onChange={value => setBatchDraft(prev => prev && { ...prev, capabilities: value })} />
            </div>
            <div className="console-field console-form-full">
              <div className="console-note">{t(locale, 'batchEditHint')}</div>
              {batchInvalid && <div className="console-note warn">{t(locale, 'modelNumberInvalid')}</div>}
            </div>
            <div className="console-inspector-actions console-form-full">
              <Button size="xs" variant="default" onClick={() => setBatchDraft(null)}>{t(locale, 'cancel')}</Button>
              <Button size="xs" loading={batchSaving} disabled={!batchTouched || batchInvalid} onClick={() => void saveBatch()}>
                {t(locale, 'batchApply', String(selected.length))}
              </Button>
            </div>
          </div>
        )}
      </Modal>

      <Modal opened={deleteOpen} onClose={() => setDeleteOpen(false)} title={`${t(locale, 'delete')} · ${platform.name}`} size="sm">
        <div className="console-note warn">
          {linked.length > 0
            ? t(locale, 'providerDeleteWarn', String(linked.length))
            : t(locale, 'deletePlatformConfirm')}
        </div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setDeleteOpen(false)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" loading={saving} onClick={() => void remove()}>{t(locale, 'confirmDelete')}</Button>
        </div>
      </Modal>

      {id && (
        <ImportModelsModal
          platformId={id}
          knownModelIds={synced.map(row => row.model_id)}
          opened={importOpen}
          onClose={() => setImportOpen(false)}
        />
      )}
      {id && (
        <ImportPricesModal
          platformId={id}
          models={synced}
          opened={pricesOpen}
          onClose={() => setPricesOpen(false)}
        />
      )}
    </div>
  )
}
