import type { ReactNode } from 'react'
import { Button, Skeleton, Stack, Text } from '@mantine/core'
import { IconAlertTriangle, IconInbox, IconRefresh } from '@tabler/icons-react'
import { useAppContext } from '../../ThemeContext'
import { t } from '../../i18n'

export function PageSkeleton({ rows = 6 }: { rows?: number }) {
  const { locale } = useAppContext()
  return (
    <div className="console-skeleton" aria-label={t(locale, 'loading')}>
      <Skeleton height={34} radius="sm" />
      <Skeleton height={92} radius="sm" />
      <Skeleton height={210} radius="sm" />
      <Stack gap={8}>
        {Array.from({ length: rows }).map((_, index) => (
          <Skeleton key={index} height={28} radius="sm" />
        ))}
      </Stack>
    </div>
  )
}

export function EmptyState({
  title,
  description,
  action,
}: {
  title: string
  description?: string
  action?: ReactNode
}) {
  return (
    <div className="console-empty">
      <IconInbox size={26} stroke={1.3} />
      <div className="console-empty-title">{title}</div>
      {description && <div className="console-empty-desc">{description}</div>}
      {action && <div className="console-empty-action">{action}</div>}
    </div>
  )
}

export function ErrorState({
  title,
  description,
  onRetry,
}: {
  title?: string
  description?: string
  onRetry?: () => void
}) {
  const { locale } = useAppContext()
  return (
    <div className="console-error">
      <IconAlertTriangle size={24} stroke={1.4} />
      <div className="console-error-title">{title || t(locale, 'loadFailed')}</div>
      {description && <div className="console-error-desc">{description}</div>}
      {onRetry && (
        <Button
          variant="default"
          size="xs"
          leftSection={<IconRefresh size={13} />}
          onClick={onRetry}
        >
          {t(locale, 'retry')}
        </Button>
      )}
    </div>
  )
}

export function StaleBadge({ updatedAt }: { updatedAt: number }) {
  const { locale } = useAppContext()
  if (!updatedAt) return null
  const time = new Date(updatedAt).toLocaleTimeString(locale === 'zh' ? 'zh-CN' : 'en-US', {
    hour12: false,
  })
  return <Text className="console-stale">{t(locale, 'staleData', time)}</Text>
}
