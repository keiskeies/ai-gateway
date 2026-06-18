import { Card, Typography } from 'antd'
import type { ReactNode } from 'react'

const { Title } = Typography

interface SectionCardProps {
  title: string
  extra?: ReactNode
  children: ReactNode
  className?: string
}

export default function SectionCard({ title, extra, children, className }: SectionCardProps) {
  return (
    <Card
      bordered
      className={`section-card ${className || ''}`}
      title={<Title level={5} style={{ margin: 0, fontSize: 15, fontWeight: 600 }}>{title}</Title>}
      extra={extra}
    >
      {children}
    </Card>
  )
}
