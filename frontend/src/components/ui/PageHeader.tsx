import type { ReactNode } from 'react'

export function PageHeader({
  kicker,
  title,
  subtitle,
  actions,
  mono = false,
}: {
  kicker: ReactNode
  title: ReactNode
  subtitle?: ReactNode
  actions?: ReactNode
  mono?: boolean
}) {
  return (
    <div className="console-page-head">
      <div className="console-page-head-main">
        <div className="console-kicker">{kicker}</div>
        <h1 className={`console-page-title${mono ? ' mono' : ''}`}>{title}</h1>
        {subtitle && <div className="console-page-sub">{subtitle}</div>}
      </div>
      {actions && <div className="console-page-actions">{actions}</div>}
    </div>
  )
}
