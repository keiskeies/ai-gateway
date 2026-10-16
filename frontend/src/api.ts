import axios from 'axios'

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

// Fetch settings to get the actual configured port on startup
export async function initConfig(): Promise<void> {
  if (!isTauri) return

  // If already on http://127.0.0.1, just use relative paths — no port discovery needed
  if (window.location.protocol === 'http:' && window.location.hostname === '127.0.0.1') {
    api.defaults.baseURL = '/api'
    return
  }

  // Fallback: still on tauri:// protocol, need to find the backend
  try {
    const resp = await fetch(`http://127.0.0.1:${configuredPort}/api/settings`)
    const data = await resp.json()
    if (data.admin_port && data.admin_port !== configuredPort) {
      configuredPort = data.admin_port
      api.defaults.baseURL = buildBaseURL()
    }
  } catch {
    const ports = [18080, 8080, 3000]
    for (const p of ports) {
      try {
        const resp = await fetch(`http://127.0.0.1:${p}/api/settings`)
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

// Routes
export const listRoutes = (proxyId: string) => api.get(`/proxies/${proxyId}/routes`).then(r => r.data)
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
export const deleteApiKey = (id: string) => api.delete(`/api-keys/${id}`)

// Stats
// 概览页统计通用时间范围（RFC3339 UTC，[from, to)）
export interface RangeOpts { from?: string; to?: string }
export const getOverview = (opts?: RangeOpts) => api.get('/stats/overview', { params: opts }).then(r => r.data)
export const getProxyStats = (proxyId: string, opts?: RangeOpts) => api.get(`/stats/proxy/${proxyId}`, { params: opts }).then(r => r.data)
export const getPlatformStats = (platformId: string, opts?: RangeOpts) => api.get(`/stats/platform/${platformId}`, { params: opts }).then(r => r.data)
export const getBackendStats = (backendId: string) => api.get(`/stats/backend/${backendId}`).then(r => r.data)
export const getModelTimeSeries = (
  granularity: 'hour' | 'day' | 'month' | 'year',
  opts?: { from?: string; to?: string; limit?: number }
) =>
  api.get('/stats/model-timeseries', { params: { granularity, ...opts } }).then(r => r.data)
export const getErrorStats = (opts?: RangeOpts) => api.get('/stats/errors', { params: opts }).then(r => r.data)
export const getHeatmap = (opts?: RangeOpts) => api.get('/stats/heatmap', { params: opts }).then(r => r.data)
export const getLatencyPercentiles = (opts?: RangeOpts) => api.get('/stats/latency', { params: opts }).then(r => r.data)
// 模型维度汇总（时间范围内每模型全量统计：请求/失败/token/延时/首字/速度）
export const getModelSummaries = (opts?: RangeOpts) => api.get('/stats/model-summaries', { params: opts }).then(r => r.data)
// 请求明细分页列表
export const getRequestLogs = (
  opts?: RangeOpts & { model?: string; status?: 'success' | 'fail'; page?: number; page_size?: number }
) => api.get('/stats/requests', { params: opts }).then(r => r.data)
export const cleanupStats = () => api.post('/stats/cleanup').then(r => r.data)

// Settings
export const getSettings = () => api.get('/settings').then(r => r.data)
export const updateSettings = (data: any) => api.put('/settings', data).then(r => r.data)
