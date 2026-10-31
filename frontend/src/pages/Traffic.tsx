import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Button,
  Group,
  Menu,
  Modal,
  Pagination,
  SegmentedControl,
  Select,
  Table,
  TextInput,
  Tooltip,
} from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconArrowDown, IconArrowUp, IconBookmark, IconCheck, IconColumns, IconCopy, IconDownload, IconFilterOff, IconRefresh, IconRepeat, IconSearch, IconStar, IconTrash } from '@tabler/icons-react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { exportTrafficCsv, getErrorStats, getModelSummaries, getRequestLogs, getRequestPayload, getRequestTrace, listApiKeys, listPlatforms, listProxies, replayRequest } from '../api'
import { rangeToParams, useAppState } from '../app/AppState'
import { useLive, usePollInterval } from '../app/LiveContext'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import type { Locale } from '../i18n'
import { PageHeader } from '../components/ui/PageHeader'
import { ColumnResizeHandle } from '../components/ColumnResizeHandle'
import { EmptyState, ErrorState, PageSkeleton, StaleBadge } from '../components/ui/States'
import { StatusPill } from '../components/ui/StatusPill'
import {
  describeView,
  filterParams,
  hasFilters,
  readDefaultViewName,
  readViews,
  sameView,
  saveDefaultViewName,
  saveViews,
  type TrafficView,
} from '../lib/views'
import {
  DEFAULT_COLUMN_IDS,
  readColumnIds,
  readColumnWidths,
  saveColumnIds,
  saveColumnWidths,
  trafficColumns,
  type TrafficColumn,
} from '../lib/trafficColumns'
import {
  formatCost,
  formatCostCoverage,
  formatDuration,
  formatNumber,
  httpTone,
} from '../lib/format'
import type {
  ApiKeyRow,
  ErrorStats,
  ModelSummaryRow,
  PlatformRow,
  ProxyRow,
  RequestLogItem,
  RequestLogPage,
  RequestPayload,
  RequestTrace,
} from '../types/stats'
import { Field } from '../components/Field'
import { RequestPayloadView, ResponsePayloadView } from '../components/PayloadViewer'

type StatusFilter = 'all' | 'success' | 'fail'

// 内置视图：不用保存也有两个常用切入点。慢请求的 15s 与延迟下拉里的 ≥15s 是同一个值，
// 所以套用后下拉会停在同一档，不会出现"菜单说慢请求、筛选器显示全部"。
const BUILTIN_VIEWS = (locale: Locale): TrafficView[] => [
  { name: t(locale, 'viewBuiltinErrors'), range: '1h', params: { status: 'fail' } },
  { name: t(locale, 'viewBuiltinSlow'), params: { lmin: '15000' } },
]

// 重放回来的正文只给 Inspector 一小块地方：长回答截断，并说清截了多少
const REPLAY_CLIP = 4000

export default function Traffic() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { locale } = useAppContext()
  const { range, live, setRange } = useAppState()
  const pollInterval = usePollInterval()
  const { pending, markSeen } = useLive()
  const [params, setParams] = useSearchParams()
  const [search, setSearch] = useState(params.get('q') || '')
  const [selectedId, setSelectedId] = useState<number | null>(null)
  const [exporting, setExporting] = useState(false)

  const page = Math.max(1, Number(params.get('page')) || 1)
  const model = params.get('model') || ''
  const status = (params.get('status') as StatusFilter) || 'all'
  const platform = params.get('platform') || ''
  const proxy = params.get('proxy') || ''
  const apiKey = params.get('key') || ''
  const errorType = params.get('error') || ''
  const query = params.get('q') || ''
  const latencyMin = params.get('lmin') || ''
  const latencyMax = params.get('lmax') || ''
  // 排序也进 URL：分享链接要能还原"按延迟降序"这个视角，而不只是筛选
  const sort = params.get('sort') || ''
  const dir: 'asc' | 'desc' = params.get('dir') === 'asc' ? 'asc' : 'desc'

  // 一次调用改多个参数：setSearchParams 的 prev 取的是当前地址栏，同一 tick 里连调两次
  // 后者会覆盖前者（排序列被方向吃掉就是这个原因），所以多键改动必须合并成一次提交。
  const applyParams = (changes: Record<string, string>) => {
    setParams(prev => {
      const next = new URLSearchParams(prev)
      for (const [key, value] of Object.entries(changes)) {
        if (value) next.set(key, value)
        else next.delete(key)
      }
      if (Object.keys(changes).some(key => key !== 'page')) next.delete('page')
      return next
    }, { replace: true })
  }
  const updateParam = (key: string, value: string) => applyParams({ [key]: value })

  // 搜索走服务端筛选：换页后条件依然生效，不必把整表拉回前端再过滤
  useEffect(() => {
    const timer = setTimeout(() => {
      if (search !== query) updateParam('q', search)
    }, 350)
    return () => clearTimeout(timer)
  }, [search, query])

  // 地址栏改了条件（前进/后退、粘贴链接）时回写输入框，否则防抖会把旧关键词塞回去
  useEffect(() => {
    setSearch(query)
  }, [query])

  // 列表与导出共用同一份筛选参数：导出的就是屏幕上这批，而不是"屏幕上这批的前 50 条"
  const filters = useMemo(() => ({
    model: model || undefined,
    status: status === 'all' ? undefined : status,
    platform: platform || undefined,
    proxy: proxy || undefined,
    api_key: apiKey || undefined,
    error_type: errorType || undefined,
    q: query || undefined,
    latency_min: latencyMin ? Number(latencyMin) : undefined,
    latency_max: latencyMax ? Number(latencyMax) : undefined,
  }), [model, status, platform, proxy, apiKey, errorType, query, latencyMin, latencyMax])

  // 视图 = 筛选 + 时间范围，存在本机 localStorage；URL 仍是唯一的真相，
  // 所以套用视图只是把参数写回地址栏，分享链接永远比本机默认视图优先。
  const [views, setViews] = useState<TrafficView[]>(readViews)
  const [defaultName, setDefaultName] = useState(readDefaultViewName)
  const [saveOpen, setSaveOpen] = useState(false)
  const [viewName, setViewName] = useState('')

  const activeView = useMemo(
    () => views.find(view => sameView(view, range, params)) ?? null,
    [views, range, params],
  )
  // 内置视图不进 views：它不能被改名或删除，所以只借它的名字显示在按钮上
  const activeBuiltin = useMemo(
    () => BUILTIN_VIEWS(locale).find(view => sameView(view, range, params)) ?? null,
    [locale, range, params],
  )

  const applyView = (view: TrafficView) => {
    setParams(new URLSearchParams(view.params), { replace: true })
    if (view.range) setRange(view.range)
  }

  const persistViews = (next: TrafficView[]) => {
    saveViews(next)
    setViews(next)
  }

  const saveView = () => {
    const name = viewName.trim()
    if (!name) return
    const existed = views.some(item => item.name === name)
    persistViews([...views.filter(item => item.name !== name), { name, range, params: filterParams(params) }])
    setSaveOpen(false)
    notifications.show({ message: t(locale, existed ? 'viewUpdated' : 'viewSaved', name), color: 'green' })
  }

  const removeView = (name: string) => {
    persistViews(views.filter(item => item.name !== name))
    if (defaultName === name) {
      saveDefaultViewName('')
      setDefaultName('')
    }
    notifications.show({ message: t(locale, 'viewDeleted', name) })
  }

  const toggleDefault = (name: string) => {
    const next = defaultName === name ? '' : name
    saveDefaultViewName(next)
    setDefaultName(next)
    notifications.show({ message: next ? t(locale, 'viewDefaultSet', next) : t(locale, 'viewDefaultCleared') })
  }

  // 默认视图只在链接本身没带筛选时套用，否则等于把别人分享的筛选条件覆盖掉
  const defaultApplied = useRef(false)
  useEffect(() => {
    if (defaultApplied.current) return
    defaultApplied.current = true
    if (hasFilters(params)) return
    const view = readViews().find(item => item.name === readDefaultViewName())
    if (view) applyView(view)
  }, [])

  // 离开这一页就把计数清掉：它是给"正在看着这张表"的人准备的，不是未读消息数
  useEffect(() => () => markSeen(), [])

  // 列选择只影响这台浏览器；顺序永远是定义顺序，选择器只改显隐不改排序
  const [columnIds, setColumnIds] = useState<string[]>(readColumnIds)
  // 列宽同样只影响这台浏览器（§7.2：不进 URL）。没记宽的列由表格自己分。
  const [widths, setWidths] = useState<Record<string, number>>(readColumnWidths)
  const [previewWidth, setPreviewWidth] = useState<{ id: string; width: number } | null>(null)
  const columns = useMemo(() => trafficColumns(locale), [locale])
  const pickedColumns = useMemo(() => {
    const picked = columns.filter(column => columnIds.includes(column.id))
    return picked.length ? picked : columns.filter(column => DEFAULT_COLUMN_IDS.includes(column.id))
  }, [columns, columnIds])
  // 分享链接可能按这台浏览器没显示的列排序（?sort=cost）：那一列得出现，否则顺序没有出处。
  // 它只为这一次显示，不写进列选择。
  const visibleColumns = useMemo(() => {
    if (!columns.some(column => column.id === sort)) return pickedColumns
    return columns.filter(column => pickedColumns.includes(column) || column.id === sort)
  }, [columns, pickedColumns, sort])

  const toggleColumn = (id: string) => {
    const next = columnIds.includes(id) ? columnIds.filter(item => item !== id) : [...columnIds, id]
    if (!next.length) return
    saveColumnIds(next)
    setColumnIds(next)
  }

  const resetColumns = () => {
    saveColumnIds(DEFAULT_COLUMN_IDS)
    setColumnIds(DEFAULT_COLUMN_IDS)
    saveColumnWidths({})
    setWidths({})
    setPreviewWidth(null)
  }

  // 拖动时只改 previewWidth，松手才写 localStorage：一次拖动会上百次 move，
  // 每格都落盘没必要，也让"取消"（pointercancel）能干净地回到拖前。
  const commitWidth = (id: string, value: number) => {
    const next = { ...widths }
    if (value > 0) next[id] = value
    else delete next[id]
    saveColumnWidths(next)
    setWidths(next)
    setPreviewWidth(null)
  }

  // 点同一列翻方向，点新列按它的自然方向起（名称从 A、数值从大）；排序走服务端，
  // 因为一页只有 50 行，前端排序只会把"这一页最慢"冒充成"这段时间最慢"。
  const toggleSort = (column: TrafficColumn) => {
    if (sort === column.id) {
      updateParam('dir', dir === 'asc' ? 'desc' : 'asc')
      return
    }
    applyParams({ sort: column.id, dir: column.firstDir ?? 'desc' })
  }

  const logsQuery = useQuery({
    queryKey: ['traffic', range, page, model, status, platform, proxy, apiKey, errorType, query, latencyMin, latencyMax, sort, dir],
    queryFn: async (): Promise<RequestLogPage> => {
      const { from, to } = rangeToParams(range)
      return getRequestLogs({
        from,
        to,
        page,
        page_size: 50,
        ...filters,
        sort: sort || undefined,
        dir: sort ? dir : undefined,
      }) as Promise<RequestLogPage>
    },
    refetchInterval: pollInterval,
  })

  const modelsQuery = useQuery({
    queryKey: ['traffic-models', range],
    queryFn: async (): Promise<ModelSummaryRow[]> => {
      const { from, to } = rangeToParams(range)
      return getModelSummaries({ from, to }) as Promise<ModelSummaryRow[]>
    },
    staleTime: 60_000,
  })

  const data = logsQuery.data
  const modelOptions = useMemo(() => {
    const ids = Array.from(new Set((modelsQuery.data || []).map(item => item.model_id))).sort()
    return [
      { value: '__all__', label: t(locale, 'allModels') },
      ...ids.map(id => ({ value: id, label: id })),
    ]
  }, [modelsQuery.data, locale])

  // 筛选候选：平台/虚拟模型/密钥名单基本不变，错误类型跟着时间范围走
  const optionsQuery = useQuery({
    queryKey: ['traffic-filter-options', range],
    queryFn: async () => {
      const { from, to } = rangeToParams(range)
      const [platforms, proxies, keys, errors] = await Promise.all([
        listPlatforms() as Promise<PlatformRow[]>,
        listProxies() as Promise<ProxyRow[]>,
        listApiKeys() as Promise<ApiKeyRow[]>,
        getErrorStats({ from, to }) as Promise<ErrorStats>,
      ])
      return { platforms, proxies, keys, kinds: errors.errors ?? [] }
    },
    staleTime: 60_000,
  })

  // 总览并进流量页：状态带 / 待处理 / 首次配置都复用这里已有的查询，不再额外请求
  const summaries = modelsQuery.data ?? []
  const options = optionsQuery.data
  const topError = options?.kinds?.[0]
  const failedModel = summaries.find(row => row.requests > 0 && row.failed_requests > 0)
  const idleKey = options?.keys?.find(key => !key.last_used)
  const issueCount = (topError ? 1 : 0) + (failedModel ? 1 : 0) + (idleKey ? 1 : 0)
  const missingSetup = !!options && options.platforms.length === 0 && options.proxies.length === 0 && options.keys.length === 0

  // 全新安装第一次进控制台直接接入向导；跳过或完成后不再打断
  useEffect(() => {
    if (!options || localStorage.getItem('wizard')) return
    if (!missingSetup || (data?.total ?? 0) > 0) return
    localStorage.setItem('wizard', 'seen')
    navigate('/setup', { replace: true })
  }, [options, missingSetup, data?.total, navigate])

  const filterOptions = useMemo(() => {
    const withAll = (allLabel: string, items: { id: string; name: string }[]) => [
      { value: '__all__', label: allLabel },
      ...items.map(item => ({ value: item.id, label: item.name })),
    ]
    const options = optionsQuery.data
    return {
      platform: withAll(t(locale, 'allPlatforms'), options?.platforms ?? []),
      proxy: withAll(t(locale, 'allProxies'), options?.proxies ?? []),
      key: withAll(t(locale, 'allKeys'), options?.keys ?? []),
      error: [
        { value: '__all__', label: t(locale, 'allErrors') },
        ...(options?.kinds ?? []).map(kind => ({ value: kind.kind, label: `${kind.kind} (${kind.count})` })),
      ],
    }
  }, [optionsQuery.data, locale])

  // 一个下拉覆盖"找慢请求"和"只看快的"两类排查；边界含入由后端的 >= / <= 保证
  const latencyOptions = useMemo(() => [
    { value: '__all__', label: t(locale, 'allLatency') },
    { value: 'min:1000', label: '≥ 1s' },
    { value: 'min:3000', label: '≥ 3s' },
    { value: 'min:10000', label: '≥ 10s' },
    { value: 'min:15000', label: '≥ 15s' },
    { value: 'max:200', label: '≤ 200ms' },
    { value: 'max:500', label: '≤ 500ms' },
  ], [locale])
  const latencyValue = latencyMin ? `min:${latencyMin}` : latencyMax ? `max:${latencyMax}` : '__all__'
  const setLatency = (value: string | null) => {
    const [kind, ms] = (value ?? '__all__').split(':')
    applyParams({ lmin: kind === 'min' ? ms : '', lmax: kind === 'max' ? ms : '' })
  }

  useEffect(() => {
    if (!data?.items.length) {
      setSelectedId(null)
      return
    }
    if (!data.items.some(item => item.id === selectedId)) {
      setSelectedId(data.items[0].id)
    }
  }, [data, selectedId])

  // 搜索/筛选全部在服务端完成，避免"只筛当前 50 条"的错觉
  const rows = data?.items ?? []

  // 自动布局下列宽由内容决定，给它设宽度不作数（实测 min/max 也只能撑宽、不能收窄），
  // 所以拖拽要配固定布局。而固定布局会把没给宽度的列平均分——于是先按内容排一次、把每列
  // 的自然宽度量下来当基线，再切固定布局。列增减或有行之后重测；useLayoutEffect 在绘制前
  // 跑完，所以看不到"先平均再还原"的闪动。
  // 基线那一趟必须一格宽度都不写：带着上次存的列宽去量，量到的就是"上次的列宽"而不是自然
  // 宽度（实测：刷新后双击回到"自动宽度"，结果停在 252 而不是 192）。
  const tableWrap = useRef<HTMLDivElement>(null)
  const columnSignature = visibleColumns.map(column => column.id).join(',')
  const [naturalWidths, setNaturalWidths] = useState<Record<string, number>>({})
  useLayoutEffect(() => {
    setNaturalWidths({})
  }, [columnSignature])
  useLayoutEffect(() => {
    if (Object.keys(naturalWidths).length) return
    const measured: Record<string, number> = {}
    for (const th of Array.from(tableWrap.current?.querySelectorAll('th[data-col]') ?? [])) {
      const id = th.getAttribute('data-col')
      if (id) measured[id] = Math.round(th.getBoundingClientRect().width)
    }
    if (Object.keys(measured).length) setNaturalWidths(measured)
  }, [naturalWidths, columnSignature, rows.length])
  const baselineReady = Object.keys(naturalWidths).length > 0

  const selected = rows.find(item => item.id === selectedId) ?? rows[0] ?? null
  const totalPages = Math.max(1, Math.ceil((data?.total ?? 0) / (data?.page_size ?? 50)))

  // 重试轨迹只在选中行且有 request_id 时拉取；V16 之前的旧行没有轨迹可查，不发请求
  const traceId = selected && selected.request_id != null ? selected.id : null
  const traceQuery = useQuery({
    queryKey: ['request-trace', traceId],
    queryFn: () => getRequestTrace(traceId as number) as Promise<RequestTrace>,
    enabled: traceId != null,
    staleTime: 60_000,
  })
  const attempts = traceQuery.data?.attempts ?? []

  // 详情正文单独拉：列表接口不带大字段，点开哪一条才取哪一条
  const payloadQuery = useQuery({
    queryKey: ['request-payload', selected?.id],
    queryFn: () => getRequestPayload(selected!.id) as Promise<RequestPayload>,
    enabled: selected != null,
    staleTime: 60_000,
  })
  const payload = payloadQuery.data

  // 重放结果只挂在当前这条明细的 Inspector 里：正文可能含敏感内容，所以不写 localStorage，
  // 换一条请求就清掉，免得上一笔的回答挂在下一笔下面。
  const [replayOpen, setReplayOpen] = useState(false)
  const [replaying, setReplaying] = useState(false)
  const [replayResult, setReplayResult] = useState<{ id: number; status: number; body: string } | null>(null)

  const onReplay = async () => {
    if (!selected) return
    const id = selected.id
    setReplayOpen(false)
    setReplaying(true)
    const result = await replayRequest(id)
    setReplaying(false)
    setReplayResult({ id, status: result.status, body: result.body })
    const failed = result.networkFailure || result.status >= 400
    notifications.show({
      title: t(locale, 'replay'),
      message: result.networkFailure
        ? t(locale, 'replayNetworkFailed', result.body || String(result.status))
        : t(locale, failed ? 'replayFailed' : 'replayDone', String(result.status)),
      color: failed ? 'red' : 'green',
      icon: <IconRepeat size={18} />,
    })
    // 重放多出来的那条明细走实时通道的"有 N 条新请求"提示条进列表：统计是后台批量写库
    // （500ms 一刷），响应回来时那一行通常还没落库，硬刷一次只会刷出旧列表。
  }

  const exportFiltered = async () => {
    setExporting(true)
    try {
      const resp = await exportTrafficCsv({ ...rangeToParams(range), ...filters })
      const header = String(resp.headers?.['content-disposition'] ?? '')
      const name = /filename="?([^";]+)"?/.exec(header)?.[1] ?? 'aigw-traffic.csv'
      const url = URL.createObjectURL(resp.data as Blob)
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = name
      anchor.click()
      URL.revokeObjectURL(url)
      // 后端按行数上限截断，截了就要说：否则"这个月只有 5000 条请求"会被当成结论
      const rows = Number(resp.headers?.['x-export-rows'] ?? 0)
      const total = Number(resp.headers?.['x-export-total'] ?? 0)
      notifications.show(
        total > rows
          ? { message: t(locale, 'exportTruncated', String(rows), String(total)), color: 'yellow' }
          : { message: t(locale, 'exportRowsDone', String(rows)), color: 'green' },
      )
    } catch {
      notifications.show({ message: t(locale, 'exportFailed'), color: 'red' })
    } finally {
      setExporting(false)
    }
  }

  if (logsQuery.isPending) return <PageSkeleton rows={8} />
  if (logsQuery.isError && !data) {
    return <ErrorState onRetry={() => void logsQuery.refetch()} />
  }

  const lastSync = logsQuery.dataUpdatedAt
    ? new Date(logsQuery.dataUpdatedAt).toLocaleTimeString(locale === 'zh' ? 'zh-CN' : 'en-US', { hour12: false })
    : '—'

  return (
    <div className="console-page">
      {/* 运行状态带：实例 / 运行中 / 有没有事，一眼看完 */}
      <div className="console-status-strip">
        <span className="console-dot ok" />
        <strong>{t(locale, 'localInstance')}</strong>
        <span>{t(locale, 'running')}</span>
        <span className="console-status-sep">·</span>
        <span className={`console-issue${issueCount > 0 ? ' warn' : ''}`}>
          <span className={`console-dot ${issueCount > 0 ? 'warn' : 'ok'}`} />
          {issueCount > 0 ? t(locale, 'needsAttention', String(issueCount)) : t(locale, 'allHealthy')}
        </span>
      </div>

      <PageHeader
        kicker={<>{t(locale, 'kickerRun')} <span className="console-kicker-sep" /> {t(locale, 'kickerTraffic')}</>}
        title={t(locale, 'traffic')}
        subtitle={`${formatNumber(data?.total ?? 0)} ${t(locale, 'requests')} · ${range.toUpperCase()} · ${lastSync}`}
        actions={
          <>
            {logsQuery.isError && <StaleBadge updatedAt={logsQuery.dataUpdatedAt} />}
            <Button
              variant="default"
              size="xs"
              leftSection={<IconDownload size={14} />}
              loading={exporting}
              onClick={() => void exportFiltered()}
            >
              {t(locale, 'exportFiltered')}
            </Button>
            <Button variant="default" size="xs" leftSection={<IconRefresh size={14} />} onClick={() => void logsQuery.refetch()}>
              {t(locale, 'refresh')}
            </Button>
          </>
        }
      />

      {missingSetup && (
        <section className="console-panel console-setup">
          <div className="console-setup-copy">
            <div className="console-kicker">{t(locale, 'kickerGetStarted')}</div>
            <h2>{t(locale, 'gettingStartedTitle')}</h2>
            <p>{t(locale, 'setupDesc')}</p>
            <Button size="xs" mt="sm" onClick={() => navigate('/setup')}>{t(locale, 'wizardOpen')}</Button>
          </div>
          <div className="console-setup-steps">
            {[
              { done: (options?.platforms.length ?? 0) > 0, title: t(locale, 'stepPlatforms'), path: '/providers' },
              { done: (options?.proxies.length ?? 0) > 0, title: t(locale, 'stepModels'), path: '/models' },
              { done: (options?.keys.length ?? 0) > 0, title: t(locale, 'stepKeys'), path: '/keys' },
              { done: (data?.total ?? 0) > 0, title: t(locale, 'stepRequests'), path: '/playground' },
            ].map((step, index) => (
              <button
                key={step.path}
                type="button"
                className={`console-setup-step${step.done ? ' done' : ''}`}
                onClick={() => navigate(step.path)}
              >
                <span className="console-setup-index">0{index + 1}</span>
                <span>{step.title}</span>
                <span className={`console-dot ${step.done ? 'ok' : 'idle'}`} />
              </button>
            ))}
          </div>
        </section>
      )}

      {issueCount > 0 && (
        <section className="console-panel">
          <div className="console-panel-head">
            <div className="console-panel-title">{t(locale, 'attentionTitle')}</div>
            <StatusPill tone="warn">{t(locale, 'needsAttention', String(issueCount))}</StatusPill>
          </div>
          <div className="console-attention-list">
            {topError && (
              <button type="button" className="console-attention-item err" onClick={() => applyParams({ error: topError.kind })}>
                <span className="console-attention-bar" />
                <span className="console-attention-main">
                  <span className="console-attention-title">{topError.kind} · {topError.count}</span>
                  <span className="console-attention-meta">{t(locale, 'failures')} · {range.toUpperCase()}</span>
                </span>
                <span className="console-attention-chev">›</span>
              </button>
            )}
            {failedModel && (
              <button type="button" className="console-attention-item warn" onClick={() => applyParams({ model: failedModel.model_id })}>
                <span className="console-attention-bar" />
                <span className="console-attention-main">
                  <span className="console-attention-title">{failedModel.model_id} · {failedModel.failed_requests} {t(locale, 'failures')}</span>
                  <span className="console-attention-meta">{t(locale, 'successShort')} {((failedModel.success_requests / Math.max(1, failedModel.requests)) * 100).toFixed(1)}%</span>
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
          </div>
        </section>
      )}

      <div className="console-panel console-filterbar">
        <Menu shadow="md" width={280}>
          <Menu.Target>
            <Button size="xs" variant="default" leftSection={<IconBookmark size={13} />}>
              {(activeView ?? activeBuiltin)?.name || t(locale, 'viewsLabel')}
            </Button>
          </Menu.Target>
          <Menu.Dropdown>
            <Menu.Label>{t(locale, 'viewBuiltinLabel')}</Menu.Label>
            {BUILTIN_VIEWS(locale).map(view => (
              <Menu.Item key={view.name} onClick={() => applyView(view)}>{view.name}</Menu.Item>
            ))}
            <Menu.Label>{t(locale, 'viewSavedLabel')}</Menu.Label>
            {views.length === 0 ? (
              <Menu.Item disabled>{t(locale, 'viewEmpty')}</Menu.Item>
            ) : views.map(view => (
              <Menu.Item key={view.name} onClick={() => applyView(view)}>
                {view.name}{view.name === defaultName ? ` · ${t(locale, 'viewDefaultTag')}` : ''}
              </Menu.Item>
            ))}
            <Menu.Divider />
            <Menu.Item
              leftSection={<IconBookmark size={13} />}
              onClick={() => { setViewName((activeView ?? activeBuiltin)?.name || describeView(range, filterParams(params))); setSaveOpen(true) }}
            >
              {t(locale, 'viewSave')}
            </Menu.Item>
            <Menu.Item
              leftSection={<IconStar size={13} />}
              disabled={!activeView}
              onClick={() => activeView && toggleDefault(activeView.name)}
            >
              {activeView && defaultName === activeView.name ? t(locale, 'viewUnsetDefault') : t(locale, 'viewSetDefault')}
            </Menu.Item>
            <Menu.Item
              color="red"
              leftSection={<IconTrash size={13} />}
              disabled={!activeView}
              onClick={() => activeView && removeView(activeView.name)}
            >
              {t(locale, 'viewDelete')}
            </Menu.Item>
          </Menu.Dropdown>
        </Menu>
        <Select
          size="xs"
          w={190}
          value={model || '__all__'}
          data={modelOptions}
          onChange={value => updateParam('model', !value || value === '__all__' ? '' : value)}
          allowDeselect={false}
          aria-label={t(locale, 'model')}
        />
        <SegmentedControl
          size="xs"
          value={status}
          onChange={value => updateParam('status', value === 'all' ? '' : value)}
          data={[
            { value: 'all', label: t(locale, 'statusAll') },
            { value: 'success', label: t(locale, 'statusSuccess') },
            { value: 'fail', label: t(locale, 'statusFail') },
          ]}
        />
        <Select
          size="xs"
          w={150}
          value={proxy || '__all__'}
          data={filterOptions.proxy}
          onChange={value => updateParam('proxy', !value || value === '__all__' ? '' : value)}
          allowDeselect={false}
          aria-label={t(locale, 'proxy')}
        />
        <Select
          size="xs"
          w={150}
          value={platform || '__all__'}
          data={filterOptions.platform}
          onChange={value => updateParam('platform', !value || value === '__all__' ? '' : value)}
          allowDeselect={false}
          aria-label={t(locale, 'platform')}
        />
        <Select
          size="xs"
          w={140}
          value={apiKey || '__all__'}
          data={filterOptions.key}
          onChange={value => updateParam('key', !value || value === '__all__' ? '' : value)}
          allowDeselect={false}
          aria-label={t(locale, 'apiKeys')}
        />
        <Select
          size="xs"
          w={170}
          value={errorType || '__all__'}
          data={filterOptions.error}
          onChange={value => updateParam('error', !value || value === '__all__' ? '' : value)}
          allowDeselect={false}
          aria-label={t(locale, 'error')}
        />
        <Select
          size="xs"
          w={120}
          value={latencyValue}
          data={latencyOptions}
          onChange={setLatency}
          allowDeselect={false}
          aria-label={t(locale, 'latencyFilter')}
        />
        <TextInput
          size="xs"
          className="console-search"
          leftSection={<IconSearch size={14} />}
          placeholder={t(locale, 'searchTraffic')}
          value={search}
          onChange={event => setSearch(event.currentTarget.value)}
        />
        <Tooltip label={t(locale, 'clearFiltersHint')} w={260}>
          <Button
            size="xs"
            variant="subtle"
            leftSection={<IconFilterOff size={13} />}
            disabled={!hasFilters(params)}
            onClick={() => setParams(new URLSearchParams(), { replace: true })}
          >
            {t(locale, 'clearFilters')}
          </Button>
        </Tooltip>
        <span className="console-filter-meta">
          {live ? t(locale, 'liveTail') : t(locale, 'paused')} · {range.toUpperCase()}
        </span>
      </div>

      {pending > 0 && (
        <button
          type="button"
          className="console-livebar"
          onClick={() => {
            markSeen()
            // 新行只会在第 1 页显形，所以先看第 1 页，已经在第 1 页才手动重取。
            if (page !== 1) {
              updateParam('page', '1')
              return
            }
            // 必须是 refetchQueries 而不是 invalidate：进页面时那一次取数常常还在飞，
            // invalidate 会被并进那一次、拿回旧列表，而实时连接在的时候页面根本不轮询，
            // 被并掉的那一次就再也刷不出来了——提示条点下去毫无反应。
            void queryClient.refetchQueries({ queryKey: ['traffic'], type: 'active' })
          }}
        >
          <span className="console-dot ok" />
          {t(locale, 'newRequests', String(pending))}
        </button>
      )}

      <div className="console-traffic-layout">
        <section className="console-panel console-traffic-table">
          <div className="console-panel-head">
            <div className="console-panel-title">{t(locale, 'requests')}</div>
            <Group gap={10} wrap="nowrap">
              <Tooltip
                withArrow
                label={t(locale, 'costCoverageHint', String(data?.priced_requests ?? 0), String(data?.total ?? 0))}
              >
                <span className="console-panel-sub">
                  {formatNumber(data?.total ?? 0)} {t(locale, 'total')} · {t(locale, 'cost')} {formatCost(data?.total_cost)} · {t(locale, 'credits')} {data?.total_credits == null ? '—' : formatNumber(data.total_credits)} · {t(locale, 'costCoverage', formatCostCoverage(data?.priced_requests, data?.total))}
                </span>
              </Tooltip>
              <Menu shadow="md" width={200} position="bottom-end" closeOnItemClick={false}>
                <Menu.Target>
                  <Tooltip label={t(locale, 'columnsHint')}>
                    <Button size="compact-xs" variant="subtle" leftSection={<IconColumns size={13} />}>
                      {t(locale, 'columnsLabel')}
                    </Button>
                  </Tooltip>
                </Menu.Target>
                <Menu.Dropdown>
                  {columns.map(column => {
                    // 选择器只反映这台浏览器存的选择，不因排序列被临时补进表头而多勾
                    const on = pickedColumns.some(item => item.id === column.id)
                    return (
                      <Menu.Item
                        key={column.id}
                        // 只剩一列时不许再关，否则表格会退化成一条没有内容的框
                        disabled={on && pickedColumns.length === 1}
                        onClick={() => toggleColumn(column.id)}
                        leftSection={<IconCheck size={13} style={{ visibility: on ? 'visible' : 'hidden' }} />}
                      >
                        {t(locale, column.labelKey)}
                      </Menu.Item>
                    )
                  })}
                  <Menu.Divider />
                  <Menu.Item onClick={resetColumns}>{t(locale, 'columnsReset')}</Menu.Item>
                </Menu.Dropdown>
              </Menu>
            </Group>
          </div>
          {rows.length > 0 ? (
            <>
              <div className="console-table-wrap console-table-scroll" ref={tableWrap}>
                <Table
                  className={`console-table${baselineReady ? ' console-table-fixed' : ''}`}
                  // 固定布局 + width:100% 会把列压到看不清；按列数给个下限，超出就让容器横向滚动
                  style={{ minWidth: visibleColumns.length * 84 }}
                >
                  <Table.Thead>
                    <Table.Tr>
                      {visibleColumns.map(column => {
                        const pinned = widths[column.id]
                        const width = baselineReady
                          ? previewWidth?.id === column.id
                            ? previewWidth.width
                            : pinned ?? naturalWidths[column.id]
                          : undefined
                        return (
                          <Table.Th
                            key={column.id}
                            data-col={column.id}
                            className={column.num ? 'num' : undefined}
                            style={width ? { width } : undefined}
                          >
                            <button
                              type="button"
                              className="console-th-sort"
                              data-active={sort === column.id ? 'yes' : undefined}
                              title={t(locale, 'sortHint', t(locale, column.labelKey))}
                              onClick={() => toggleSort(column)}
                            >
                              {column.head}
                              {sort === column.id ? (dir === 'asc' ? <IconArrowUp size={10} stroke={2} /> : <IconArrowDown size={10} stroke={2} />) : null}
                            </button>
                            <ColumnResizeHandle
                              columnLabel={t(locale, column.labelKey)}
                              width={width}
                              pinned={pinned != null}
                              locale={locale}
                              onPreview={value => setPreviewWidth({ id: column.id, width: value })}
                              onCommit={value => commitWidth(column.id, value)}
                            />
                          </Table.Th>
                        )
                      })}
                    </Table.Tr>
                  </Table.Thead>
                  <Table.Tbody>
                    {rows.map(item => (
                      <Table.Tr
                        key={item.id}
                        className={selected?.id === item.id ? 'selected' : undefined}
                        onClick={() => setSelectedId(item.id)}
                        style={{ cursor: 'pointer' }}
                      >
                        {visibleColumns.map(column => (
                          <Table.Td
                            key={column.id}
                            className={column.cellClass?.(item) ?? (column.mono ? 'mono' : undefined)}
                          >
                            {column.cell(item)}
                          </Table.Td>
                        ))}
                      </Table.Tr>
                    ))}
                  </Table.Tbody>
                </Table>
              </div>
              <div className="console-pagination">
                <span>PAGE {page} / {totalPages} · 50 PER PAGE</span>
                <Pagination
                  size="xs"
                  total={totalPages}
                  value={page}
                  onChange={value => updateParam('page', String(value))}
                  getControlProps={part => (part === 'previous' || part === 'next' ? { 'aria-label': t(locale, part === 'previous' ? 'prevPage' : 'nextPage') } : {})}
                />
              </div>
            </>
          ) : (
            <EmptyState
              title={t(locale, data?.items.length ? 'noMatchingRequests' : 'noRequests')}
              description={t(locale, data?.items.length ? 'clearFilters' : 'noData')}
              action={data?.items.length ? (
                <Button variant="default" size="xs" onClick={() => setSearch('')}>
                  {t(locale, 'clearFilters')}
                </Button>
              ) : undefined}
            />
          )}
        </section>

        <aside className="console-panel console-inspector">
          {selected ? (
            <>
              <div className="console-inspector-head">
                <div>
                  <div className="console-inspector-kicker">{t(locale, 'requestLabel')}</div>
                  <div className="console-inspector-id">#{selected.id}</div>
                </div>
                <StatusPill tone={httpTone(selected.status_code)}>{selected.status_code}</StatusPill>
              </div>
              {selected.replayed_from != null && (
                <div className="console-replayed-from">{t(locale, 'replayedFrom', String(selected.replayed_from))}</div>
              )}
              <div className="console-meta-grid">
                <div><div className="console-meta-label">{t(locale, 'proxy')}</div><div className="console-meta-value">{selected.proxy_name}</div></div>
                <div><div className="console-meta-label">{t(locale, 'platform')}</div><div className="console-meta-value">{selected.platform_name}</div></div>
                <div><div className="console-meta-label">{t(locale, 'modelId')}</div><div className="console-meta-value mono">{selected.model_id}</div></div>
                <div><div className="console-meta-label">{t(locale, 'stream')}</div><div className="console-meta-value">{selected.is_stream ? 'stream' : 'non-stream'}</div></div>
                <div><div className="console-meta-label">{t(locale, 'apiKey')}</div><div className="console-meta-value">{selected.api_key_name || '—'}</div></div>
                <div><div className="console-meta-label">{t(locale, 'requestPath')}</div><div className="console-meta-value mono">{selected.path || '—'}</div></div>
                <div>
                  <div className="console-meta-label">
                    <Tooltip label={t(locale, 'creditsHint')}><span>{t(locale, 'credits')}</span></Tooltip>
                  </div>
                  <div className="console-meta-value mono">{selected.credits == null ? '—' : formatNumber(selected.credits)}</div>
                </div>
              </div>
              <div className="console-attempts">
                <div className="console-section-label">
                  <Tooltip label={t(locale, 'attemptsHint')}><span>{t(locale, 'attemptsTitle')}</span></Tooltip>
                </div>
                {selected.request_id == null ? (
                  <div className="console-attempts-note">{t(locale, 'attemptsUnavailable')}</div>
                ) : traceQuery.isError ? (
                  <div className="console-attempts-note">{t(locale, 'attemptsFailed')}</div>
                ) : traceQuery.isPending ? (
                  <div className="console-attempts-note">{t(locale, 'attemptsLoading')}</div>
                ) : attempts.length === 0 ? (
                  <div className="console-attempts-note">{t(locale, 'attemptsEmpty')}</div>
                ) : (
                  attempts.map(attempt => {
                    const failed = attempt.status_code == null || attempt.status_code >= 400
                    return (
                      <div key={attempt.id} className={failed ? 'console-attempt-row is-failed' : 'console-attempt-row'}>
                        <span className="console-attempt-no">{attempt.attempt_no}</span>
                        <span className="console-attempt-backend">
                          {attempt.platform_name ?? t(locale, 'attemptBackendMissing')}
                          {attempt.model_id ? ` · ${attempt.model_id}` : ''}
                        </span>
                        {attempt.forward_url && (
                          <span className="console-attempt-url" title={attempt.forward_url}>{attempt.forward_url}</span>
                        )}
                        <span className="console-attempt-status">{attempt.status_code ?? t(locale, 'errShort')}</span>
                        <span>{attempt.latency_ms == null ? '—' : formatDuration(attempt.latency_ms)}</span>
                        {attempt.error_message && (
                          <span className="console-attempt-msg">{attempt.error_message}</span>
                        )}
                        {attempt.nonce && (
                          <span className="console-attempt-nonce" title={t(locale, 'payloadNonceTitle')} data-e2e-attempt-nonce={attempt.nonce}>
                            {t(locale, 'attemptNonce', attempt.nonce, String(attempt.nonce.length))}
                          </span>
                        )}
                      </div>
                    )
                  })
                )}
              </div>
              <div className="console-payload">
                <div className="console-section-label">{t(locale, 'payloadTitle')}</div>
                {payloadQuery.isPending ? (
                  <div className="console-attempts-note">{t(locale, 'payloadLoading')}</div>
                ) : payloadQuery.isError ? (
                  <div className="console-attempts-note">{t(locale, 'payloadFailed')}</div>
                ) : payload ? (
                  <>
                    <details className="console-payload-block">
                      <summary>
                        {t(locale, 'requestHeaders')}
                        {!payload.request.headers_captured ? ` · ${t(locale, 'payloadNotCaptured')}` : ''}
                        {payload.request.headers_captured && payload.request.headers_truncated ? ` · ${t(locale, 'payloadTruncatedShort')}` : ''}
                      </summary>
                      {payload.request.headers ? (
                        <pre className="console-payload-body">{JSON.stringify(payload.request.headers, null, 2)}</pre>
                      ) : (
                        <div className="console-note">{t(locale, 'payloadNotCaptured')}</div>
                      )}
                      {payload.request.headers_truncated && <div className="console-note warn">{t(locale, 'payloadTruncated')}</div>}
                    </details>
                    <details className="console-payload-block" open>
                      <summary>
                        {t(locale, 'requestBody')}
                        {!payload.request.body_captured
                          ? ` · ${payload.request_capture_enabled ? t(locale, 'payloadNotRecordedShort') : t(locale, 'payloadCaptureOffShort')}`
                          : payload.request.body_truncated ? ` · ${t(locale, 'payloadTruncatedShort')}` : ''}
                      </summary>
                      <RequestPayloadView payload={payload} locale={locale} />
                    </details>
                    <details className="console-payload-block" open>
                      <summary>
                        {t(locale, 'responseBody')}
                        {!payload.response.captured
                          ? ` · ${payload.response_capture_enabled ? t(locale, 'payloadNotRecordedShort') : t(locale, 'payloadCaptureOffShort')}`
                          : payload.response.truncated ? ` · ${t(locale, 'payloadTruncatedShort')}` : ''}
                      </summary>
                      <ResponsePayloadView
                        response={payload.response}
                        captureEnabled={payload.response_capture_enabled}
                        locale={locale}
                      />
                    </details>
                  </>
                ) : null}
              </div>
              <div className="console-waterfall">
                <div className="console-section-label">{t(locale, 'timingLabel')}</div>
                {selected.ttft_ms != null ? (
                  <>
                    <div className="console-wf-row">
                      <span>{t(locale, 'ttftLabel')}</span>
                      <span className="console-wf-track"><span className="console-wf-bar" style={{ width: `${Math.min(100, (selected.ttft_ms / Math.max(1, selected.latency_ms)) * 100)}%` }} /></span>
                      <span>{formatDuration(selected.ttft_ms)}</span>
                    </div>
                    <div className="console-wf-row">
                      <span>{t(locale, 'genLabel')}</span>
                      <span className="console-wf-track"><span className="console-wf-bar" style={{ width: `${Math.min(100, (Math.max(0, selected.latency_ms - selected.ttft_ms) / Math.max(1, selected.latency_ms)) * 100)}%` }} /></span>
                      <span>{formatDuration(Math.max(0, selected.latency_ms - selected.ttft_ms))}</span>
                    </div>
                  </>
                ) : (
                  <div className="console-wf-row">
                    <span>{t(locale, 'upstreamLabel')}</span>
                    <span className="console-wf-track"><span className="console-wf-bar" style={{ width: '100%' }} /></span>
                    <span>{formatDuration(selected.latency_ms)}</span>
                  </div>
                )}
                <div className="console-timing-total">
                  <span>{t(locale, 'latency')}</span>
                  <strong>{formatDuration(selected.latency_ms)}</strong>
                </div>
              </div>
              <div className="console-meta-grid">
                <div><div className="console-meta-label">{t(locale, 'inputTokens')}</div><div className="console-meta-value mono">{formatNumber(selected.token_input ?? 0)}</div></div>
                <div><div className="console-meta-label">{t(locale, 'outputTokens')}</div><div className="console-meta-value mono">{formatNumber(selected.token_output ?? 0)}</div></div>
                <div><div className="console-meta-label">{t(locale, 'cacheRead')}</div><div className="console-meta-value mono">{formatNumber(selected.cache_read_tokens ?? 0)}</div></div>
                <div><div className="console-meta-label">{t(locale, 'freshInput')}</div><div className="console-meta-value mono">{formatNumber(selected.fresh_input_tokens ?? 0)}</div></div>
                <div style={{ gridColumn: '1 / -1' }}>
                  <div className="console-meta-label">
                    <Tooltip label={t(locale, 'costHint')}><span>{t(locale, 'costEstimate')}</span></Tooltip>
                  </div>
                  <div className="console-meta-value mono">{selected.cost == null ? t(locale, 'costUnpriced') : formatCost(selected.cost)}</div>
                </div>
              </div>
              {selected.error_type && (
                <div className="console-inspector-error">
                  <span className="console-section-label">{t(locale, 'error')}</span>
                  <span>{selected.error_type}</span>
                </div>
              )}
              <div className="console-inspector-actions">
                <Button
                  size="xs"
                  variant="default"
                  leftSection={<IconCopy size={13} />}
                  onClick={() => {
                    void navigator.clipboard.writeText(String(selected.id))
                    notifications.show({ message: t(locale, 'copied'), color: 'green' })
                  }}
                >
                  {t(locale, 'copy')} ID
                </Button>
                <Button size="xs" variant="default" onClick={() => navigate('/models')}>
                  {t(locale, 'openModel')}
                </Button>
                {selected.has_body ? (
                  <Button
                    size="xs"
                    variant="default"
                    leftSection={<IconRepeat size={13} />}
                    loading={replaying}
                    onClick={() => setReplayOpen(true)}
                  >
                    {t(locale, 'replay')}
                  </Button>
                ) : (
                  <Tooltip label={t(locale, 'replayNoBody')}>
                    <span>
                      <Button size="xs" variant="default" leftSection={<IconRepeat size={13} />} disabled>
                        {t(locale, 'replay')}
                      </Button>
                    </span>
                  </Tooltip>
                )}
              </div>
              {replayResult && replayResult.id === selected.id && (
                <div className="console-replay">
                  <div className="console-replay-head">
                    <span className="console-section-label">{t(locale, 'replayResultTitle')}</span>
                    <StatusPill tone={httpTone(replayResult.status)}>{replayResult.status}</StatusPill>
                  </div>
                  <div className="console-note">{t(locale, selected.is_stream ? 'replayStreamHint' : 'replayResultHint')}</div>
                  <pre className="console-replay-body">
                    {replayResult.body.length > REPLAY_CLIP
                      ? `${replayResult.body.slice(0, REPLAY_CLIP)}${t(locale, 'replayTruncated', String(replayResult.body.length - REPLAY_CLIP))}`
                      : replayResult.body}
                  </pre>
                </div>
              )}
            </>
          ) : (
            <EmptyState title={t(locale, 'noRequests')} description={t(locale, 'clearFilters')} />
          )}
        </aside>
      </div>

      <Modal opened={saveOpen} onClose={() => setSaveOpen(false)} title={t(locale, 'viewSaveTitle')} size="sm">
        <Field label={t(locale, 'viewName')}>
          <TextInput
            size="xs"
            value={viewName}
            onChange={event => setViewName(event.currentTarget.value)}
            onKeyDown={event => { if (event.key === 'Enter') void saveView() }}
            autoFocus
          />
        </Field>
        <div className="console-note" style={{ marginTop: 10 }}>{t(locale, 'viewSaveHint')}</div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setSaveOpen(false)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" disabled={!viewName.trim()} onClick={() => void saveView()}>{t(locale, 'viewSaveConfirm')}</Button>
        </div>
      </Modal>

      <Modal opened={replayOpen} onClose={() => setReplayOpen(false)} title={t(locale, 'replayConfirmTitle')} size="sm">
        <div className="console-note">{t(locale, 'replayConfirmBody')}</div>
        <div className="console-replay-target console-mono">
          #{selected?.id} · {selected?.model_id} · {selected?.is_stream ? 'stream' : 'non-stream'}
        </div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setReplayOpen(false)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" leftSection={<IconRepeat size={13} />} loading={replaying} onClick={() => void onReplay()}>
            {t(locale, 'replay')}
          </Button>
        </div>
      </Modal>
    </div>
  )
}
