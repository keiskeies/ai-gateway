import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { proxyBaseURL } from '../api'
import { useAppState } from './AppState'

/**
 * 控制台的实时通道。
 *
 * 为什么不是"事件里带数据"：SSE 只当"该刷新了"的信使，前端仍然走原来的聚合接口取数。
 * 事件可以安全地丢（浏览器标签页被冻结、后端队列溢出），一次全量刷新就能追平，
 * 不需要补发历史，也不需要客户端维护序号。
 *
 * 为什么还留着轮询：连接断开时（`status === 'polling'`）页面继续 30s 轮询，
 * 实时是加速器而不是唯一通路——不能因为一条连接挂了控制台就不动了。
 */

export type LiveStatus = 'connecting' | 'live' | 'polling'

interface LiveValue {
  status: LiveStatus
  /** 已经发生但还没被看到的新请求数；由流量页的提示条消费 */
  pending: number
  markSeen: () => void
}

const LiveContext = createContext<LiveValue | null>(null)

/** 一批新请求到达后，这些聚合视图直接刷新——它们只是数字，跳动不打扰人 */
const REFRESH_KEYS = ['overview', 'models', 'platforms', 'traffic-models', 'traffic-filter-options', 'api-keys-usage', 'alerts']

/** 合并短时间内的连续事件：统计写入器每 500ms 可能就来一批，不该每批触发一轮全量刷新 */
const DEBOUNCE_MS = 1200

// 与 axios 同一套 origin 判断：浏览器里是同源 /api，Tauri 回退到 127.0.0.1:{port}
function eventsUrl(): string {
  return `${proxyBaseURL()}/api/events?topics=requests,platform_check,alerts`
}

export function LiveProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient()
  const { live } = useAppState()
  const [status, setStatus] = useState<LiveStatus>('connecting')
  const [pending, setPending] = useState(0)
  const debounce = useRef<number | null>(null)

  useEffect(() => {
    if (!live) {
      setStatus('connecting')
      return
    }
    const source = new EventSource(eventsUrl())

    const flush = () => {
      debounce.current = null
      REFRESH_KEYS.forEach(key => void queryClient.invalidateQueries({ queryKey: [key] }))
    }
    const scheduleFlush = () => {
      if (debounce.current !== null) return
      debounce.current = window.setTimeout(flush, DEBOUNCE_MS)
    }

    source.onopen = () => setStatus('live')
    // EventSource 自己会重连；这里只负责在断开的窗口里把页面切回轮询
    source.onerror = () => setStatus('polling')

    source.addEventListener('requests', event => {
      const data = JSON.parse((event as MessageEvent).data) as { count?: number; failures?: number }
      if (data.count) setPending(prev => Math.min(9999, prev + data.count!))
      scheduleFlush()
    })

    source.addEventListener('platform_check', event => {
      const data = JSON.parse((event as MessageEvent).data) as { platform_id?: string }
      void queryClient.invalidateQueries({ queryKey: ['platforms'] })
      if (data.platform_id) {
        void queryClient.invalidateQueries({ queryKey: ['platform-health', data.platform_id] })
      }
    })

    source.addEventListener('alerts', () => {
      // 服务端只在"真的新出现一条告警"时发这个信号，所以不必防抖；
      // 前端重拉也不会自激：下一次扫描 opened 就是 0
      void queryClient.invalidateQueries({ queryKey: ['alerts'] })
    })

    return () => {
      if (debounce.current !== null) window.clearTimeout(debounce.current)
      source.close()
    }
  }, [live, queryClient])

  const markSeen = useCallback(() => setPending(0), [])

  return <LiveContext.Provider value={{ status, pending, markSeen }}>{children}</LiveContext.Provider>
}

export function useLive(): LiveValue {
  const value = useContext(LiveContext)
  if (!value) throw new Error('useLive must be used inside LiveProvider')
  return value
}

/**
 * 页面轮询节奏：实时连接在的时候不轮询，断了或还没连上回到 30s 兜底。
 * `live` 关掉时一律不轮询（用户主动要静止画面）。
 */
export function usePollInterval(): number | false {
  const { live } = useAppState()
  const { status } = useLive()
  if (!live) return false
  return status === 'live' ? false : 30_000
}
