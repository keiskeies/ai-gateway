import type { Locale } from '../i18n'

export function formatNumber(n?: number | null): string {
  return (n ?? 0).toLocaleString()
}

export function formatCompact(n?: number | null): string {
  const value = n ?? 0
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`
  return value.toString()
}

export function formatDuration(ms?: number | null): string {
  if (ms == null) return '—'
  if (ms < 1000) return `${Math.round(ms)}ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(2)}s`
  return `${(ms / 60_000).toFixed(1)}m`
}

export function formatLogTime(iso: string, locale: Locale): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  const time = date.toLocaleTimeString(locale === 'zh' ? 'zh-CN' : 'en-US', { hour12: false })
  const sameDay = date.toDateString() === new Date().toDateString()
  if (sameDay) return time
  return `${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')} ${time}`
}

/// 成本估算：单价按"每 100 万 token"记录，币种跟随模型行里填的数字，这里不加任何货币符号。
/// null 一律渲染成 —（"没定价"和"成本为 0"是两件事，不能都显示 0）。
export function formatCost(value?: number | null): string {
  if (value == null || !Number.isFinite(value)) return '—'
  if (value === 0) return '0'
  const abs = Math.abs(value)
  const maximumFractionDigits = abs >= 100 ? 2 : abs >= 1 ? 3 : abs >= 0.01 ? 4 : 6
  return value.toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits })
}

/// 成本覆盖率：只有已定价的请求才算得出成本，所以合计数必须带上"这个数覆盖了多少请求"
/// 有已定价请求却不到 1% 时显示 <1%，不能显示 0%——那会和同一行的成本数字自相矛盾。
export function formatCostCoverage(priced?: number | null, total?: number | null): string {
  if (!total || total <= 0) return '—'
  const value = priced ?? 0
  if (value <= 0) return '0%'
  const percent = Math.min(100, Math.round((value / total) * 100))
  return percent > 0 ? `${percent}%` : '<1%'
}

export function deltaPercent(current?: number, previous?: number): number | null {  // 小样本或异常跳变会让百分比失去意义，宁可不显示。
  if (previous === undefined || previous < 10) return null
  const value = ((current ?? 0) - previous) / previous * 100
  if (!Number.isFinite(value) || Math.abs(value) > 999) return null
  return value
}

export type StatusTone = 'ok' | 'warn' | 'err' | 'idle'

export function successRateTone(rate: number, hasTraffic = true): StatusTone {
  if (!hasTraffic) return 'idle'
  if (rate >= 99) return 'ok'
  // 90% 以下才视为故障；90–99% 为降级。
  if (rate >= 90) return 'warn'
  return 'err'
}

export function httpTone(status: number): StatusTone {
  if (status >= 200 && status < 300) return 'ok'
  if (status === 429) return 'warn'
  return 'err'
}
