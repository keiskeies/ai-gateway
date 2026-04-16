import axios from 'axios'

// Detect Tauri desktop mode
const isTauri = !!(window as any).__TAURI_INTERNALS__

// In Tauri desktop mode, frontend is loaded from tauri:// protocol
// We must use 127.0.0.1 (not localhost) to avoid mixed-content blocking
let configuredPort = 1994

function buildBaseURL(): string {
  if (isTauri) {
    return `http://127.0.0.1:${configuredPort}/api`
  }
  return '/api'
}

function buildProxyBaseURL(): string {
  if (isTauri) {
    return `http://127.0.0.1:${configuredPort}`
  }
  return ''
}

const api = axios.create({ baseURL: buildBaseURL() })

// Fetch settings to get the actual configured port on startup
export async function initConfig(): Promise<void> {
  if (!isTauri) return
  try {
    const resp = await fetch(`http://127.0.0.1:${configuredPort}/api/settings`)
    const data = await resp.json()
    if (data.admin_port && data.admin_port !== configuredPort) {
      configuredPort = data.admin_port
      api.defaults.baseURL = buildBaseURL()
    }
  } catch {
    // If default port fails, try common alternatives
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

// Models
export const listModels = () => api.get('/models').then(r => r.data)
export const createModel = (data: any) => api.post('/models', data).then(r => r.data)
export const updateModel = (id: string, data: any) => api.put(`/models/${id}`, data).then(r => r.data)
export const deleteModel = (id: string) => api.delete(`/models/${id}`)

// Proxies
export const listProxies = () => api.get('/proxies').then(r => r.data)
export const getProxy = (id: string) => api.get(`/proxies/${id}`).then(r => r.data)
export const createProxy = (data: any) => api.post('/proxies', data).then(r => r.data)
export const updateProxy = (id: string, data: any) => api.put(`/proxies/${id}`, data).then(r => r.data)
export const deleteProxy = (id: string) => api.delete(`/proxies/${id}`)
export const startProxy = (id: string) => api.post(`/proxies/${id}/start`).then(r => r.data)
export const stopProxy = (id: string) => api.post(`/proxies/${id}/stop`).then(r => r.data)

// Routes
export const listRoutes = (proxyId: string) => api.get(`/proxies/${proxyId}/routes`).then(r => r.data)
export const createRoute = (proxyId: string, data: any) => api.post(`/proxies/${proxyId}/routes`, data).then(r => r.data)
export const updateRoute = (id: string, data: any) => api.put(`/routes/${id}`, data).then(r => r.data)
export const deleteRoute = (id: string) => api.delete(`/routes/${id}`)
export const listBackends = (routeId: string) => api.get(`/routes/${routeId}/backends`).then(r => r.data)
export const addBackend = (routeId: string, data: any) => api.post(`/routes/${routeId}/backends`, data).then(r => r.data)
export const updateBackend = (id: string, data: any) => api.put(`/backends/${id}`, data).then(r => r.data)
export const deleteBackend = (id: string) => api.delete(`/backends/${id}`)

// Stats
export const getOverview = () => api.get('/stats/overview').then(r => r.data)

// Settings
export const getSettings = () => api.get('/settings').then(r => r.data)
export const updateSettings = (data: any) => api.put('/settings', data).then(r => r.data)
