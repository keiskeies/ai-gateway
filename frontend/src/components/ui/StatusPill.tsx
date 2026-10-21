import type { ReactNode } from 'react'
import type { StatusTone } from '../../lib/format'

export function StatusPill({
  tone = 'idle',
  children,
}: {
  tone?: StatusTone | 'signal'
  children: ReactNode
}) {
  return (
    <span className={`console-pill ${tone}`}>
      <span className={`console-dot ${tone}`} />
      {children}
    </span>
  )
}
