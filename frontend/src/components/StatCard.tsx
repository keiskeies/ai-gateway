import { Card, Statistic } from 'antd'
import type { ReactNode } from 'react'

interface StatCardProps {
  title: string
  value: string | number
  prefix?: ReactNode
  suffix?: string
  precision?: number
  valueStyle?: React.CSSProperties
  className?: string
}

export default function StatCard({ title, value, prefix, suffix, precision, valueStyle, className }: StatCardProps) {
  return (
    <Card bordered className={`stat-card ${className || ''}`}>
      <Statistic
        title={title}
        value={value}
        prefix={prefix}
        suffix={suffix}
        precision={precision}
        valueStyle={{ fontWeight: 700, fontSize: 22, letterSpacing: '-0.02em', ...valueStyle }}
      />
    </Card>
  )
}
