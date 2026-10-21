import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, Modal } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import {
  IconAlertTriangle,
  IconCheck,
  IconDownload,
  IconHistory,
  IconRefresh,
  IconTrash,
} from '@tabler/icons-react'
import {
  createBackup,
  deleteBackup,
  downloadBackup,
  listBackups,
  restoreBackup,
  type BackupInfo,
} from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { formatLogTime } from '../lib/format'

// 备份按字节算，1.6 MB 和 1600000 比，前者才是人能扫的
const formatSize = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

const serverMessage = (err: any): string => err?.response?.data?.error?.message ?? ''

export function DatabaseBackup() {
  const { locale } = useAppContext()
  const queryClient = useQueryClient()
  const backupsQuery = useQuery({ queryKey: ['backups'], queryFn: () => listBackups() })

  const [taking, setTaking] = useState(false)
  const [restoreTarget, setRestoreTarget] = useState<BackupInfo | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<BackupInfo | null>(null)
  const [working, setWorking] = useState(false)

  const items = backupsQuery.data?.items ?? []

  const onTake = async () => {
    setTaking(true)
    try {
      const info = await createBackup()
      notifications.show({
        message: t(locale, 'backupTaken', info.name),
        color: 'green',
        icon: <IconCheck size={18} />,
      })
      await backupsQuery.refetch()
    } catch (err: any) {
      notifications.show({
        message: `${t(locale, 'backupTakeFailed')}: ${serverMessage(err) || err?.message}`,
        color: 'red',
      })
    } finally {
      setTaking(false)
    }
  }

  const onDownload = async (item: BackupInfo) => {
    try {
      const resp = await downloadBackup(item.name)
      const url = URL.createObjectURL(resp.data as Blob)
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = item.name
      anchor.click()
      URL.revokeObjectURL(url)
    } catch {
      notifications.show({ message: t(locale, 'loadFailed'), color: 'red' })
    }
  }

  const onRestore = async () => {
    if (!restoreTarget) return
    setWorking(true)
    try {
      const result = await restoreBackup(restoreTarget.name)
      notifications.show({
        title: t(locale, 'backupRestored', restoreTarget.name),
        // 还原把 sessions 一起回滚，下一次请求就 401：先说清下一步，再给出退路文件名
        message: (
          <>
            <div>{result.needs_restart ? t(locale, 'backupRestartHint') : t(locale, 'backupReloginHint')}</div>
            <div>{t(locale, 'backupSafetyNote', result.safety_backup)}</div>
          </>
        ),
        color: result.needs_restart ? 'yellow' : 'green',
        icon: result.needs_restart ? <IconAlertTriangle size={18} /> : <IconCheck size={18} />,
      })
      // 整库换掉了：任何按 queryKey 的局部失效迟早会漏掉一处还在显示还原前的数据
      await queryClient.invalidateQueries()
      setRestoreTarget(null)
    } catch (err: any) {
      notifications.show({
        message: `${t(locale, 'backupRestoreFailed')}: ${serverMessage(err) || err?.message}`,
        color: 'red',
      })
    } finally {
      setWorking(false)
    }
  }

  const onDelete = async () => {
    if (!deleteTarget) return
    setWorking(true)
    try {
      await deleteBackup(deleteTarget.name)
      notifications.show({ message: t(locale, 'backupDeleted', deleteTarget.name), color: 'gray' })
      await backupsQuery.refetch()
      setDeleteTarget(null)
    } catch (err: any) {
      notifications.show({
        message: `${t(locale, 'backupDeleteFailed')}: ${serverMessage(err) || err?.message}`,
        color: 'red',
      })
    } finally {
      setWorking(false)
    }
  }

  return (
    <section className="console-panel" style={{ marginTop: 14 }}>
      <div className="console-panel-head">
        <div className="console-panel-title">{t(locale, 'backupTitle')}</div>
        <span className="console-panel-sub">
          {items.length > 0
            ? `${t(locale, 'backupTotal', String(items.length))} · ${formatSize(backupsQuery.data?.total_bytes ?? 0)}`
            : t(locale, 'backupLabel')}
        </span>
      </div>
      <div className="console-panel-body">
        <div className="console-note">{t(locale, 'backupIntro')}</div>
        <div className="console-route-actions">
          <Button
            size="xs"
            variant="default"
            leftSection={<IconHistory size={13} />}
            loading={taking}
            onClick={() => void onTake()}
          >
            {t(locale, 'backupTake')}
          </Button>
          <Button
            size="xs"
            variant="default"
            leftSection={<IconRefresh size={13} />}
            loading={backupsQuery.isFetching}
            onClick={() => void backupsQuery.refetch()}
          >
            {t(locale, 'refresh')}
          </Button>
        </div>

        {backupsQuery.data && backupsQuery.data.dir && (
          <div className="console-backup-dir">
            <span>{t(locale, 'backupDirHint')}</span>
            <code className="console-mono">{backupsQuery.data.dir}</code>
          </div>
        )}

        {items.length === 0 && !backupsQuery.isPending && (
          <div className="console-note">{t(locale, 'backupEmpty')}</div>
        )}

        {items.length > 0 && (
          <div className="console-table-wrap">
            <table className="console-table">
              <thead>
                <tr>
                  <th>{t(locale, 'time')}</th>
                  <th className="num">{t(locale, 'backupColSize')}</th>
                  <th className="num">{t(locale, 'backupColVersion')}</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {items.map(item => (
                  <tr key={item.name}>
                    <td className="mono">{formatLogTime(item.created_at, locale)}</td>
                    <td className="num">{formatSize(item.size_bytes)}</td>
                    <td className="num muted">
                      {item.schema_version == null ? t(locale, 'backupVersionUnknown') : `V${item.schema_version}`}
                    </td>
                    <td>
                      <div className="console-backup-actions">
                        <Button size="compact-xs" variant="subtle" leftSection={<IconDownload size={12} />} onClick={() => void onDownload(item)}>
                          {t(locale, 'backupDownload')}
                        </Button>
                        <Button size="compact-xs" variant="subtle" leftSection={<IconHistory size={12} />} onClick={() => setRestoreTarget(item)}>
                          {t(locale, 'backupRestore')}
                        </Button>
                        <Button size="compact-xs" variant="subtle" color="red" leftSection={<IconTrash size={12} />} onClick={() => setDeleteTarget(item)}>
                          {t(locale, 'delete')}
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <Modal opened={deleteTarget !== null} onClose={() => setDeleteTarget(null)} title={t(locale, 'backupDeleteTitle')} size="sm">
        <div className="console-note warn">
          {deleteTarget?.name} · {formatSize(deleteTarget?.size_bytes ?? 0)}
        </div>
        <div className="console-note">{t(locale, 'backupDeleteDesc')}</div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setDeleteTarget(null)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" loading={working} onClick={() => void onDelete()}>{t(locale, 'confirmDelete')}</Button>
        </div>
      </Modal>

      <Modal opened={restoreTarget !== null} onClose={() => setRestoreTarget(null)} title={t(locale, 'backupRestoreTitle')} size="sm">
        <div className="console-note warn">
          {restoreTarget?.name} · {formatSize(restoreTarget?.size_bytes ?? 0)} · {t(locale, 'time')} {formatLogTime(restoreTarget?.created_at ?? '', locale)}
        </div>
        <div className="console-note warn">{t(locale, 'backupRestoreDesc')}</div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setRestoreTarget(null)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" loading={working} onClick={() => void onRestore()}>{t(locale, 'backupRestore')}</Button>
        </div>
      </Modal>
    </section>
  )
}
