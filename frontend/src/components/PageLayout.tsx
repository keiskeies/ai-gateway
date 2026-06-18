import type { ReactNode } from 'react'
import { Typography } from 'antd'

const { Title } = Typography

interface PageLayoutProps {
  title: string
  extra?: ReactNode
  children: ReactNode
}

export default function PageLayout({ title, extra, children }: PageLayoutProps) {
  return (
    <div className="page-layout">
      <div className="page-header">
        <Title level={5} className="page-title">{title}</Title>
        {extra && <div className="page-extra">{extra}</div>}
      </div>
      <div className="page-content">{children}</div>
    </div>
  )
}
