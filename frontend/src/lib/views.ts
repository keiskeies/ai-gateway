import type { TimeRangeKey } from '../app/AppState'

// 视图存的是筛选 + 时间范围，不存 page：套用视图永远从第 1 页开始。
export interface TrafficView {
  name: string
  range?: TimeRangeKey
  params: Record<string, string>
}

const VIEWS_KEY = 'traffic-views'
const DEFAULT_KEY = 'traffic-view-default'

// URL 里代表筛选的参数；page 不算筛选
export const FILTER_KEYS = ['model', 'status', 'proxy', 'platform', 'key', 'error', 'q', 'lmin', 'lmax']

export const filterParams = (params: URLSearchParams): Record<string, string> => {
  const out: Record<string, string> = {}
  FILTER_KEYS.forEach(key => {
    const value = params.get(key)
    if (value) out[key] = value
  })
  return out
}

export const hasFilters = (params: URLSearchParams): boolean => FILTER_KEYS.some(key => Boolean(params.get(key)))

// localStorage 是外部输入面：手改过的坏 JSON 不该让整页白屏
export function readViews(): TrafficView[] {
  try {
    const raw = JSON.parse(localStorage.getItem(VIEWS_KEY) || '[]')
    if (!Array.isArray(raw)) return []
    return raw.filter((item: any) => typeof item?.name === 'string' && typeof item?.params === 'object' && item.params !== null)
  } catch {
    return []
  }
}

export function saveViews(views: TrafficView[]) {
  localStorage.setItem(VIEWS_KEY, JSON.stringify(views))
}

export const readDefaultViewName = (): string => localStorage.getItem(DEFAULT_KEY) || ''

export function saveDefaultViewName(name: string) {
  if (name) localStorage.setItem(DEFAULT_KEY, name)
  else localStorage.removeItem(DEFAULT_KEY)
}

// 改过任何一个筛选或时间范围就不再"是这个视图"，否则菜单上的名字会骗人
export const sameView = (view: TrafficView, range: TimeRangeKey, params: URLSearchParams): boolean =>
  (view.range === undefined || view.range === range)
  && JSON.stringify(Object.entries(view.params).sort()) === JSON.stringify(Object.entries(filterParams(params)).sort())

export const describeView = (range: TimeRangeKey, params: Record<string, string>): string =>
  [range.toUpperCase(), ...Object.entries(params).map(([key, value]) => `${key}=${value}`)].join(' · ')
