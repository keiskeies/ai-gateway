import axios from 'axios'
import type { ClientApplyBody, ClientApplyResult, ClientConfigFile, ClientConfigWrite, ClientPreview, ClientRestoreResult, ClientStatus, ExtraSpec } from './types/clients'

// Detect Tauri desktop mode
const isTauri = !!(window as any).__TAURI_INTERNALS__

// In Tauri mode, the WebView navigates to http://127.0.0.1:{port}
// so the frontend and API share the same origin — use relative paths.
// For standalone browser dev, Vite proxy handles /api -> localhost:1994.
let configuredPort = 1994

function buildBaseURL(): string {
  if (isTauri && window.location.protocol !== 'http:') {
    // Still on tauri:// protocol (shouldn't happen with new approach, but fallback)
    return `http://127.0.0.1:${configuredPort}/api`
  }
  // Same origin — use relative path
  return '/api'
}

function buildProxyBaseURL(): string {
  if (isTauri && window.location.protocol !== 'http:') {
    return `http://127.0.0.1:${configuredPort}`
  }
  // Same origin
  return ''
}

const api = axios.create({ baseURL: buildBaseURL() })

// 会话过期时由 App 切回登录页；登录/改密接口自身的 401 不触发
api.interceptors.response.use(
  response => response,
  error => {
    const status = error?.response?.status
    const url = String(error?.config?.url || '')
    if (status === 401 && !url.includes('/auth/')) {
      window.dispatchEvent(new Event('aigw:unauthorized'))
    }
    return Promise.reject(error)
  },
)

export interface AuthStatus {
  initialized: boolean
  authenticated: boolean
  username: string | null
  admin_port: number
}

// Fetch auth status (public) to discover the configured port on startup
export async function initConfig(): Promise<void> {
  if (!isTauri) return

  // If already on http://127.0.0.1, just use relative paths — no port discovery needed
  if (window.location.protocol === 'http:' && window.location.hostname === '127.0.0.1') {
    api.defaults.baseURL = '/api'
    return
  }

  // Fallback: still on tauri:// protocol, need to find the backend
  try {
    const resp = await fetch(`http://127.0.0.1:${configuredPort}/api/auth/status`)
    const data = await resp.json()
    if (data.admin_port && data.admin_port !== configuredPort) {
      configuredPort = data.admin_port
      api.defaults.baseURL = buildBaseURL()
    }
  } catch {
    const ports = [18080, 8080, 3000]
    for (const p of ports) {
      try {
        const resp = await fetch(`http://127.0.0.1:${p}/api/auth/status`)
        const data = await resp.json()
        if (data.admin_port) {
          configuredPort = data.admin_port
          api.defaults.baseURL = buildBaseURL()
          return
        }
      } catch { continue }
    }
  }
}

// Auth
export const getAuthStatus = (): Promise<AuthStatus> => api.get('/auth/status').then(r => r.data)
export const login = (username: string, password: string) =>
  api.post('/auth/login', { username, password }).then(r => r.data)
export const setupAdmin = (username: string, password: string) =>
  api.post('/auth/setup', { username, password }).then(r => r.data)
export const logout = () => api.post('/auth/logout').then(r => r.data)
export const changePassword = (currentPassword: string, newPassword: string) =>
  api.put('/auth/password', { current_password: currentPassword, new_password: newPassword }).then(r => r.data)

// Dynamic proxy base URL getter
export function proxyBaseURL(): string {
  return buildProxyBaseURL()
}

// Platforms
export const listPlatforms = () => api.get('/platforms').then(r => r.data)
export const getPlatform = (id: string) => api.get(`/platforms/${id}`).then(r => r.data)
export const createPlatform = (data: any) => api.post('/platforms', data).then(r => r.data)
export const updatePlatform = (id: string, data: any) => api.put(`/platforms/${id}`, data).then(r => r.data)
export const deletePlatform = (id: string) => api.delete(`/platforms/${id}`)
export const listPresets = () => api.get('/platforms/presets').then(r => r.data)
export const fetchRemoteModels = (id: string) => api.get(`/platforms/${id}/remote-models`).then(r => r.data)
// 把远端列表落库：只补缺失行，已有行不动，也不删除
export const syncPlatformModels = (id: string) => api.post(`/platforms/${id}/models/sync`).then(r => r.data)
// 勾选导入：只建传上去的这些，口径与同步入库完全一致
export const importPlatformModels = (id: string, modelIds: string[]) =>
  api.post(`/platforms/${id}/models/import`, { model_ids: modelIds }).then(r => r.data)
// 批量改价 / 能力：字段不出现在 body 里＝不改，null＝清空，给值＝设值（与单行编辑同一套三态）
export const batchUpdatePlatformModels = (id: string, patch: Record<string, unknown>) =>
  api.put(`/platforms/${id}/models/batch`, patch).then(r => r.data)
// 定价表导出：三列（model_id + 两侧单价），正是导入吃的格式，改完可以直接传回来
export const exportPlatformPrices = (id: string) =>
  api.get(`/platforms/${id}/models/prices`, { responseType: 'blob' })
// 定价导入：只改本平台已有模型的单价，不建模型；每行仍是那套三态
export const importPlatformPrices = (id: string, prices: Record<string, unknown>[]) =>
  api.post(`/platforms/${id}/models/prices`, { prices }).then(r => r.data)
// 保存前测试：用还没落库的配置探测；platform_id 只在 Key 留空时用于取回库里已存的那份
export const testPlatformConfig = (data: { type: string; base_url: string; api_key?: string; platform_id?: string; custom_headers?: Record<string, string> }) =>
  api.post('/platforms/test', data).then(r => r.data)

// Models
export const listModels = () => api.get('/models').then(r => r.data)
export const createModel = (data: any) => api.post('/models', data).then(r => r.data)
export const updateModel = (id: string, data: any) => api.put(`/models/${id}`, data).then(r => r.data)
export const deleteModel = (id: string) => api.delete(`/models/${id}`)
export const testModelConnection = (id: string) => api.post(`/models/${id}/test`).then(r => r.data)

// Proxies
export const listProxies = () => api.get('/proxies').then(r => r.data)
export const getProxy = (id: string) => api.get(`/proxies/${id}`).then(r => r.data)
export const createProxy = (data: any) => api.post('/proxies', data).then(r => r.data)
export const updateProxy = (id: string, data: any) => api.put(`/proxies/${id}`, data).then(r => r.data)
export const deleteProxy = (id: string) => api.delete(`/proxies/${id}`)
// 删除确认用：该虚拟模型名下会被一并销毁的历史行数（不受时间范围影响）
export const getProxyHistoryImpact = (proxyId: string) => api.get(`/stats/proxy/${proxyId}/impact`).then(r => r.data)

// 通知中心：告警是读时扫描出来的当前状态，不是事件流水
export interface AlertItem {
  id: number
  dedupe_key: string
  kind: string
  severity: string
  ref_type?: string | null
  ref_id?: string | null
  code: string
  args: string[]
  first_seen: string
  last_seen: string
  read_at?: string | null
  resolved_at?: string | null
}
export const listAlerts = () =>
  api.get('/alerts').then(r => r.data as { items: AlertItem[]; unread: number })
export const readAlert = (key: string) =>
  api.post(`/alerts/${encodeURIComponent(key)}/read`).then(r => r.data as { unread: number })
export const readAllAlerts = () =>
  api.post('/alerts/read-all').then(r => r.data as { marked: number; unread: number })

// 整库快照：文件在数据库同级的 backups/ 里，接口只负责列 / 拍 / 取 / 删 / 还原
export interface BackupInfo {
  name: string
  size_bytes: number
  created_at: string
  schema_version: number | null
}
export interface BackupList {
  items: BackupInfo[]
  dir: string
  total_bytes: number
}
export interface RestoreResult {
  restored_from: string
  safety_backup: string
  needs_restart: boolean
  current_version: number
  backup_version: number | null
}
export const listBackups = () => api.get('/backups').then(r => r.data as BackupList)
export const createBackup = () => api.post('/backups').then(r => r.data as BackupInfo)
export const deleteBackup = (name: string) => api.delete(`/backups/${encodeURIComponent(name)}`)
export const restoreBackup = (name: string) =>
  api.post(`/backups/${encodeURIComponent(name)}/restore`).then(r => r.data as RestoreResult)
export const downloadBackup = (name: string) =>
  api.get(`/backups/${encodeURIComponent(name)}`, { responseType: 'blob' })

// Routes
export const listRoutes = (proxyId: string) => api.get(`/proxies/${proxyId}/routes`).then(r => r.data)
// 一次拉取全部路由 + 后端，替代按虚拟模型逐个请求
export const listAllRoutes = () => api.get('/routes').then(r => r.data)
export const createRoute = (proxyId: string, data: any) => api.post(`/proxies/${proxyId}/routes`, data).then(r => r.data)
export const updateRoute = (id: string, data: any) => api.put(`/routes/${id}`, data).then(r => r.data)
export const deleteRoute = (id: string) => api.delete(`/routes/${id}`)
export const listBackends = (routeId: string) => api.get(`/routes/${routeId}/backends`).then(r => r.data)
export const addBackend = (routeId: string, data: any) => api.post(`/routes/${routeId}/backends`, data).then(r => r.data)
export const updateBackend = (id: string, data: any) => api.put(`/backends/${id}`, data).then(r => r.data)
export const deleteBackend = (id: string) => api.delete(`/backends/${id}`)

// API Keys
export const listApiKeys = () => api.get('/api-keys').then(r => r.data)
export const createApiKey = (data: any) => api.post('/api-keys', data).then(r => r.data)
// 三态字段：显式给 null 才是"清空"，不给＝保持原值。编辑弹窗按"整份表单提交"处理，所以总是给全。
export interface ApiKeyUpdatePayload {
  name?: string
  enabled?: boolean
  proxy_id?: string | null
  expires_at?: string | null
  quota_requests?: number | null
  quota_tokens?: number | null
  quota_cost?: number | null
  quota_window?: string | null
}
export const updateApiKey = (id: string, data: ApiKeyUpdatePayload) => api.put(`/api-keys/${id}`, data).then(r => r.data)
export const rotateApiKey = (id: string) => api.post(`/api-keys/${id}/rotate`).then(r => r.data)
export const deleteApiKey = (id: string) => api.delete(`/api-keys/${id}`)
export type ApiKeyBatchAction = 'enable' | 'disable' | 'delete'
// 批量只支持三种动作，返回 affected / missing：界面上的数字要有出处
export const batchApiKeys = (ids: string[], action: ApiKeyBatchAction) =>
  api.post('/api-keys/batch', { ids, action }).then(r => r.data as { affected: number; missing: string[] })

// Stats
// 概览页统计通用时间范围（RFC3339 UTC，[from, to)）
export interface RangeOpts { from?: string; to?: string }
export const getOverview = (opts?: RangeOpts) => api.get('/stats/overview', { params: opts }).then(r => r.data)
export const getProxyStats = (proxyId: string, opts?: RangeOpts) => api.get(`/stats/proxy/${proxyId}`, { params: opts }).then(r => r.data)
// 一次拉取全部虚拟模型的时段用量，替代逐个查询
export const getAllProxyStats = (opts?: RangeOpts) => api.get('/stats/proxies', { params: opts }).then(r => r.data)
export const getPlatformStats = (platformId: string, opts?: RangeOpts) => api.get(`/stats/platform/${platformId}`, { params: opts }).then(r => r.data)
// 一次拉取全部平台的时段用量，替代按平台逐个查询（每个平台原本 5 条 SQL）
export const getAllPlatformStats = (opts?: RangeOpts) => api.get('/stats/platforms', { params: opts }).then(r => r.data)
// 一次拉取全部平台的连通健康汇总（时段内检测/失败次数 + 最后一次结果）
export const getAllPlatformHealth = (opts?: RangeOpts) => api.get('/stats/platforms/health', { params: opts }).then(r => r.data)
// 单个平台最近 50 次检测历史（倒序）
export const getPlatformHealth = (platformId: string) => api.get(`/stats/platform/${platformId}/health`).then(r => r.data)
export const getBackendStats = (backendId: string) => api.get(`/stats/backend/${backendId}`).then(r => r.data)
export const getModelTimeSeries = (
  granularity: 'hour' | 'day' | 'month' | 'year',
  opts?: { from?: string; to?: string; limit?: number }
) =>
  api.get('/stats/model-timeseries', { params: { granularity, ...opts } }).then(r => r.data)
export const getErrorStats = (opts?: RangeOpts & { model?: string }) => api.get('/stats/errors', { params: opts }).then(r => r.data)
export const getHeatmap = (opts?: RangeOpts) => api.get('/stats/heatmap', { params: opts }).then(r => r.data)
export const getLatencyPercentiles = (opts?: RangeOpts) => api.get('/stats/latency', { params: opts }).then(r => r.data)
// 模型维度汇总（时间范围内每模型全量统计：请求/失败/token/延时/首字/速度）
export const getModelSummaries = (opts?: RangeOpts) => api.get('/stats/model-summaries', { params: opts }).then(r => r.data)
// 请求明细分页列表
export type RequestLogParams = RangeOpts & {
  model?: string
  status?: 'success' | 'fail'
  platform?: string
  proxy?: string
  api_key?: string
  error_type?: string
  q?: string
  latency_min?: number
  latency_max?: number
  page?: number
  page_size?: number
  /** 排序列（前端列 id）；后端只认白名单，其余值回落时间倒序 */
  sort?: string
  dir?: 'asc' | 'desc'
}
export const getRequestLogs = (opts?: RequestLogParams) => api.get('/stats/requests', { params: opts }).then(r => r.data)
// 按当前筛选导出 CSV：文件由服务端生成，所以"导出这批"和"屏幕上这批"用的是同一条 WHERE。
// 响应头 x-export-rows / x-export-total 用来判断是否被行数上限截断。
// 排序被 Omit 掉是刻意的：导出文件永远按时间倒序，同一份筛选两次导出才能逐行比对。
export const exportTrafficCsv = (
  opts: Omit<RequestLogParams, 'page' | 'page_size' | 'sort' | 'dir'> & { limit?: number }
) => api.get('/stats/requests/export', { params: opts, responseType: 'blob' })
// 单次请求的重试/fallback 轨迹；id 是明细行的 request_stats.id
export const getRequestTrace = (id: number) => api.get(`/stats/requests/${id}/trace`).then(r => r.data)
// 清理悬挂统计行：dry_run 只报数不删，确认弹窗要先给出行数再说"不可恢复"
export const cleanupStats = (opts?: { dry_run?: boolean }) => api.post('/stats/cleanup', opts ?? {}).then(r => r.data)
// 按年龄清理请求明细与轨迹：同样先预演，确认后才真删
export const purgeStats = (opts: { older_than_days: number; dry_run?: boolean }) => api.post('/stats/purge', opts).then(r => r.data)
// 数据库体检：路径 / 体积 / 各表行数 / 明细时间跨度
export const getDbInfo = () => api.get('/stats/db').then(r => r.data)
// 正文采集关掉后把已落库的提示词抹掉；统计与轨迹不动
export const deleteRequestBodies = () => api.delete('/stats/bodies').then(r => r.data)

export interface ReplayResult {
  status: number
  /** 上游回什么就是什么：非流式是一段 JSON，流式是 SSE 事件流原文 */
  body: string
  /** 上游状态码但请求没跑完（超时、断连）时为 true */
  networkFailure: boolean
}

/// 重放按文本收正文：流式响应是一段 SSE，交给 JSON 解析器会在半路失败。
/// 上游非 2xx 也照样返回结果而不是抛错——"重放又失败了"本身就是要看的结果。
export async function replayRequest(id: number): Promise<ReplayResult> {
  try {
    const res = await api.post(`/stats/requests/${id}/replay`, null, {
      responseType: 'text',
      transformResponse: [(raw: unknown) => raw],
    })
    return { status: res.status, body: String(res.data ?? ''), networkFailure: false }
  } catch (error) {
    const res = axios.isAxiosError(error) ? error.response : undefined
    if (!res) return { status: 0, body: String(axios.isAxiosError(error) ? error.message : error), networkFailure: true }
    return { status: res.status, body: String(res.data ?? ''), networkFailure: false }
  }
}
// 密钥维度用量：列表一次聚合，详情页按模型拆分
export const getApiKeyUsage = (opts?: RangeOpts) => api.get('/stats/api-keys', { params: opts }).then(r => r.data)
export const getApiKeyModelUsage = (id: string, opts?: RangeOpts) => api.get(`/stats/api-keys/${id}`, { params: opts }).then(r => r.data)

// Settings
export const getSettings = () => api.get('/settings').then(r => r.data)
export const updateSettings = (data: any) => api.put('/settings', data).then(r => r.data)

// 配置迁移：导出是只读快照（默认不带上游密钥），导入先看预演再落库
export interface TablePlan {
  add: number
  overwrite: number
  skip: number
  drop: number
}
export interface ConfigIssue {
  code: string
  args: string[]
}
export interface ImportPlan {
  ok: boolean
  mode: 'merge' | 'replace'
  source_version: string
  source_exported_at: string
  platforms: TablePlan
  models: TablePlan
  proxies: TablePlan
  routes: TablePlan
  backends: TablePlan
  api_keys: TablePlan
  warnings: ConfigIssue[]
  errors: ConfigIssue[]
}
export interface ImportResult {
  mode: 'merge' | 'replace'
  platforms: TablePlan
  models: TablePlan
  proxies: TablePlan
  routes: TablePlan
  backends: TablePlan
  api_keys: TablePlan
  warnings: ConfigIssue[]
}
export type ImportModeName = 'merge' | 'replace'

export const exportConfig = (secrets: boolean) =>
  api.get('/config/export', {
    params: secrets ? { secrets: '1' } : undefined,
    responseType: 'blob',
  })
export const planConfigImport = (doc: unknown, mode: ImportModeName): Promise<ImportPlan> =>
  api.post('/config/import/plan', doc, { params: { mode } }).then(r => r.data)
export const applyConfigImport = (doc: unknown, mode: ImportModeName): Promise<ImportResult> =>
  api.post('/config/import', doc, { params: { mode } }).then(r => r.data)

// 客户端接入：检测 / 预览 / 应用 / 撤销 / 恢复备份
export const listClients = (): Promise<ClientStatus[]> => api.get('/clients').then(r => r.data)
export const getClientExtrasSpec = async (): Promise<Record<string, ExtraSpec[]>> => {
  const pairs = await api.get<Array<[string, ExtraSpec[]]>>('/clients/extras-spec').then(r => r.data)
  return Object.fromEntries(pairs)
}
export const previewClient = (id: string, body: ClientApplyBody): Promise<ClientPreview> =>
  api.post(`/clients/${id}/preview`, body).then(r => r.data)
export const applyClient = (id: string, body: ClientApplyBody): Promise<ClientApplyResult> =>
  api.post(`/clients/${id}/apply`, body).then(r => r.data)
export const restoreClient = (id: string): Promise<ClientRestoreResult> =>
  api.post(`/clients/${id}/restore`).then(r => r.data)
export const getClientConfig = (id: string): Promise<ClientConfigFile> =>
  api.get(`/clients/${id}/config`).then(r => r.data)
export const putClientConfig = (id: string, content: string): Promise<ClientConfigWrite> =>
  api.put(`/clients/${id}/config`, { content }).then(r => r.data)
export const restoreClientBackup = (id: string): Promise<ClientRestoreResult> =>
  api.post(`/clients/${id}/restore-backup`).then(r => r.data)
