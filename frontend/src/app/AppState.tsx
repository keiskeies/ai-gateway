import { createContext, useContext, useState, type ReactNode } from 'react'
import type { TranslationKey } from '../i18n'

export type TimeRangeKey = '1h' | '24h' | '7d' | '30d'

export const TIME_RANGES: { key: TimeRangeKey; labelKey: TranslationKey }[] = [
  { key: '1h', labelKey: 'range1h' },
  { key: '24h', labelKey: 'range24h' },
  { key: '7d', labelKey: 'range7d' },
  { key: '30d', labelKey: 'range30d' },
]

interface AppStateValue {
  range: TimeRangeKey
  setRange: (range: TimeRangeKey) => void
  live: boolean
  setLive: (live: boolean) => void
}

const AppStateContext = createContext<AppStateValue | null>(null)

export function AppStateProvider({ children }: { children: ReactNode }) {
  const [range, setRange] = useState<TimeRangeKey>('24h')
  const [live, setLive] = useState(true)

  return (
    <AppStateContext.Provider value={{ range, setRange, live, setLive }}>
      {children}
    </AppStateContext.Provider>
  )
}

export function useAppState(): AppStateValue {
  const value = useContext(AppStateContext)
  if (!value) throw new Error('useAppState must be used inside AppStateProvider')
  return value
}

/** 计算当前时间窗口；每次查询时调用，保证刷新拿到最新窗口。 */
export function rangeToParams(range: TimeRangeKey): { from: string; to: string } {
  const to = new Date()
  const spanMs = {
    '1h': 60 * 60 * 1000,
    '24h': 24 * 60 * 60 * 1000,
    '7d': 7 * 24 * 60 * 60 * 1000,
    '30d': 30 * 24 * 60 * 60 * 1000,
  }[range]
  const from = new Date(to.getTime() - spanMs)
  return { from: from.toISOString(), to: to.toISOString() }
}
