import { Typography } from 'antd'
import type { ReactNode } from 'react'

const { Text } = Typography

interface EmptyStateProps {
  icon: ReactNode
  description: string
}

export default function EmptyState({ icon, description }: EmptyStateProps) {
  return (
    <div className="empty-state">
      <div className="empty-state-icon">{icon}</div>
      <Text type="secondary">{description}</Text>
    </div>
  )
}
