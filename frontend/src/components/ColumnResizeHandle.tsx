import { useEffect, useRef, useState } from 'react'
import { t, type Locale } from '../i18n'
import { MAX_COLUMN_WIDTH, MIN_COLUMN_WIDTH } from '../lib/trafficColumns'

const KEY_STEP = 16

// 列宽手柄。它是表头排序按钮的兄弟节点，所以拖动永远不会误触排序。
// `onCommit(0)` 表示"回到自动宽度"，键盘与双击都走这一条。
export function ColumnResizeHandle({
  columnLabel,
  width,
  pinned,
  locale,
  onPreview,
  onCommit,
}: {
  columnLabel: string
  width?: number
  /** 这一列是不是被用户钉过宽——没钉时读起来是"自动宽度"，尽管它当下也有实际像素宽 */
  pinned: boolean
  locale: Locale
  onPreview: (width: number) => void
  onCommit: (width: number) => void
}) {
  const origin = useRef<{ x: number; width: number } | null>(null)
  const self = useRef<HTMLSpanElement>(null)
  const [dragging, setDragging] = useState(false)
  // role=separator 的可聚焦手柄必须报出当前值；没钉宽的列从所在 th 上量一次
  const [current, setCurrent] = useState<number | null>(width ?? null)

  useEffect(() => {
    if (width != null) {
      setCurrent(width)
      return
    }
    const th = self.current?.closest('th')
    setCurrent(th ? Math.round(th.getBoundingClientRect().width) : null)
  }, [width])

  // 没设过宽的列不知道自己多宽，从所在的 th 上量一次——键盘第一次按方向键也有正确起点。
  const measuredFrom = (node: Element | null) => {
    if (width) return width
    const th = node?.closest('th')
    return th?.getBoundingClientRect().width ?? MIN_COLUMN_WIDTH
  }

  const clamp = (value: number) => Math.min(MAX_COLUMN_WIDTH, Math.max(MIN_COLUMN_WIDTH, Math.round(value)))

  const commit = (value: number) => {
    if (value) setCurrent(value)
    onCommit(value)
  }

  const fromPointer = (event: { clientX: number }) => {
    const start = origin.current
    if (!start) return null
    return clamp(start.width + (event.clientX - start.x))
  }

  return (
    <span
      ref={self}
      className="console-th-resize"
      role="separator"
      aria-orientation="vertical"
      tabIndex={0}
      data-dragging={dragging ? 'yes' : undefined}
      aria-label={t(locale, 'columnResizeAria', columnLabel)}
      title={t(locale, 'columnResizeHint')}
      aria-valuenow={current ?? MIN_COLUMN_WIDTH}
      aria-valuemin={MIN_COLUMN_WIDTH}
      aria-valuemax={MAX_COLUMN_WIDTH}
      aria-valuetext={pinned && width ? `${width}px` : t(locale, 'columnResizeAuto')}
      onPointerDown={event => {
        event.preventDefault()
        event.currentTarget.setPointerCapture(event.pointerId)
        origin.current = { x: event.clientX, width: measuredFrom(event.currentTarget) }
        setDragging(true)
      }}
      onPointerMove={event => {
        const next = fromPointer(event)
        if (next != null) {
          setCurrent(next)
          onPreview(next)
        }
      }}
      onPointerUp={event => {
        const next = fromPointer(event)
        origin.current = null
        setDragging(false)
        if (next != null) commit(next)
      }}
      onPointerCancel={() => {
        origin.current = null
        setDragging(false)
      }}
      onDoubleClick={() => commit(0)}
      onKeyDown={event => {
        const step = event.shiftKey ? KEY_STEP * 3 : KEY_STEP
        if (event.key === 'ArrowLeft') {
          event.preventDefault()
          commit(clamp(measuredFrom(event.currentTarget) - step))
        } else if (event.key === 'ArrowRight') {
          event.preventDefault()
          commit(clamp(measuredFrom(event.currentTarget) + step))
        } else if (event.key === 'Home' || event.key === 'Delete') {
          event.preventDefault()
          commit(0)
        }
      }}
    />
  )
}
