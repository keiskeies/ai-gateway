import type { ReactNode } from 'react'
import { Tooltip } from '@mantine/core'
import { IconRepeat } from '@tabler/icons-react'
import { StatusPill } from '../components/ui/StatusPill'
import { t, type Locale, type TranslationKey } from '../i18n'
import { formatCompact, formatCost, formatDuration, formatLogTime, httpTone } from './format'
import type { RequestLogItem } from '../types/stats'

export interface TrafficColumn {
  id: string
  labelKey: TranslationKey
  num?: boolean
  mono?: boolean
  /** 第一次点表头时的方向：名称类从 A 起，数值类从大起 */
  firstDir?: 'asc' | 'desc'
  head: ReactNode
  cell: (item: RequestLogItem) => ReactNode
  cellClass?: (item: RequestLogItem) => string | undefined
}

const dash = <span className="muted">—</span>
const tokenOrDash = (value: number | null) => (value == null ? dash : formatCompact(value))

// 列的定义顺序就是表头顺序，选择器只改显隐不改顺序——重排列会让"记住的列"变成"记不住的列"。
export const trafficColumns = (locale: Locale): TrafficColumn[] => [
  { id: 'time', labelKey: 'time', mono: true, head: t(locale, 'time'), cell: item => formatLogTime(item.created_at, locale) },
  {
    id: 'model',
    labelKey: 'model',
    firstDir: 'asc',
    head: t(locale, 'model'),
    // 重放出来的那笔要在列表里认得出来：同一模型、同一时间窗，只有来源不同
    cell: item => (
      <span className="console-strong">
        {item.model_id}
        {item.replayed_from != null && (
          <Tooltip label={t(locale, 'replayedFrom', String(item.replayed_from))}>
            <span className="console-replay-mark"><IconRepeat size={11} stroke={2} /></span>
          </Tooltip>
        )}
      </span>
    ),
  },
  { id: 'proxy', labelKey: 'proxy', firstDir: 'asc', head: t(locale, 'proxy'), cell: item => item.proxy_name || dash },
  { id: 'platform', labelKey: 'platform', firstDir: 'asc', head: t(locale, 'platform'), cell: item => item.platform_name },
  { id: 'apiKey', labelKey: 'apiKey', firstDir: 'asc', head: t(locale, 'apiKey'), cell: item => item.api_key_name || dash },
  { id: 'status', labelKey: 'status', head: 'STATUS', cell: item => <StatusPill tone={httpTone(item.status_code)}>{item.status_code}</StatusPill> },
  {
    id: 'attempts',
    labelKey: 'attemptsCol',
    num: true,
    head: <Tooltip label={t(locale, 'attemptsColHint')}><span>{t(locale, 'attemptsCol')}</span></Tooltip>,
    cell: item => (item.attempts == null ? dash : String(item.attempts)),
    // 大于 1 说明这一笔被重试或换过后端——状态可能仍是 200，但链路不干净
    cellClass: item => (item.attempts != null && item.attempts > 1 ? 'warn' : undefined),
  },
  { id: 'latency', labelKey: 'latency', num: true, head: t(locale, 'latency'), cell: item => formatDuration(item.latency_ms) },
  { id: 'ttft', labelKey: 'ttft', num: true, head: t(locale, 'ttft'), cell: item => (item.ttft_ms == null ? dash : formatDuration(item.ttft_ms)) },
  { id: 'tokens', labelKey: 'tokens', num: true, head: t(locale, 'tokens'), cell: item => formatCompact((item.token_input ?? 0) + (item.token_output ?? 0)) },
  { id: 'freshInput', labelKey: 'freshInput', num: true, head: t(locale, 'freshInput'), cell: item => tokenOrDash(item.fresh_input_tokens) },
  { id: 'cacheRead', labelKey: 'cacheRead', num: true, head: t(locale, 'cacheRead'), cell: item => tokenOrDash(item.cache_read_tokens) },
  { id: 'cacheWrite', labelKey: 'cacheWrite', num: true, head: t(locale, 'cacheWrite'), cell: item => tokenOrDash(item.cache_write_tokens) },
  { id: 'speed', labelKey: 'outputSpeed', num: true, head: t(locale, 'outputSpeed'), cell: item => (item.output_speed_tps == null ? dash : `${item.output_speed_tps.toFixed(1)} t/s`) },
  { id: 'stream', labelKey: 'stream', firstDir: 'asc', head: t(locale, 'stream'), cell: item => (item.is_stream ? 'stream' : 'non-stream') },
  {
    id: 'cost',
    labelKey: 'cost',
    num: true,
    head: <Tooltip label={t(locale, 'costHint')}><span>{t(locale, 'cost')}</span></Tooltip>,
    cell: item => (item.cost == null ? dash : formatCost(item.cost)),
  },
  {
    id: 'error',
    labelKey: 'error',
    firstDir: 'asc',
    head: t(locale, 'error'),
    cell: item => item.error_type || dash,
    cellClass: item => (item.error_type ? 'err' : 'muted'),
  },
]

// 默认列就是 §7.2 那一组：成本默认关掉（覆盖率低时整列都是 —），面板头部仍然给总成本与覆盖率。
export const DEFAULT_COLUMN_IDS = ['time', 'model', 'platform', 'status', 'latency', 'ttft', 'tokens', 'error']

const COLUMNS_KEY = 'traffic-columns'

// 存的是 id 列表，旧版本残留或手改过的值只会让列变少，不会白屏
export function readColumnIds(): string[] {
  try {
    const raw = JSON.parse(localStorage.getItem(COLUMNS_KEY) || 'null')
    return Array.isArray(raw) && raw.every(id => typeof id === 'string') ? raw : DEFAULT_COLUMN_IDS
  } catch {
    return DEFAULT_COLUMN_IDS
  }
}

export function saveColumnIds(ids: string[]) {
  localStorage.setItem(COLUMNS_KEY, JSON.stringify(ids))
}

export const MIN_COLUMN_WIDTH = 56
export const MAX_COLUMN_WIDTH = 480

const WIDTHS_KEY = 'traffic-column-widths'

// 列宽只影响这台浏览器，所以不进 URL（见 §7.2"不进 URL 的"）。没记宽的列交给表格自己分。
export function readColumnWidths(): Record<string, number> {
  try {
    const raw = JSON.parse(localStorage.getItem(WIDTHS_KEY) || 'null')
    if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return {}
    const kept: Array<[string, number]> = []
    for (const [id, value] of Object.entries(raw as Record<string, unknown>)) {
      if (typeof value === 'number' && value >= MIN_COLUMN_WIDTH && value <= MAX_COLUMN_WIDTH) kept.push([id, value])
    }
    return Object.fromEntries(kept)
  } catch {
    return {}
  }
}

export function saveColumnWidths(widths: Record<string, number>) {
  localStorage.setItem(WIDTHS_KEY, JSON.stringify(widths))
}
