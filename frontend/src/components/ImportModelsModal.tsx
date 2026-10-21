import { useEffect, useMemo, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, Checkbox, Modal, ScrollArea, Stack, TextInput } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { fetchRemoteModels, importPlatformModels } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { EmptyState, ErrorState } from './ui/States'

interface RemoteListResponse {
  models?: { id?: string }[]
  model_count?: number
  note?: string
}

// 勾选导入：远端列表来自 GET /platforms/{id}/remote-models，库里已有的直接不出现在勾选列表里
// （导入接口本来也会跳过它们，价格/能力/显示名绝不覆盖），只在计数里交代一句。
// 平台动辄列出上百个模型，所以默认一个都不选：全选等于一键灌库，之后还得逐行删。
export function ImportModelsModal({
  platformId,
  knownModelIds,
  opened,
  onClose,
}: {
  platformId: string
  knownModelIds: string[]
  opened: boolean
  onClose: () => void
}) {
  const queryClient = useQueryClient()
  const { locale } = useAppContext()
  const [picked, setPicked] = useState<Set<string>>(new Set())
  const [filter, setFilter] = useState('')
  const [saving, setSaving] = useState(false)

  const remoteQuery = useQuery({
    queryKey: ['platform-remote-models', platformId],
    queryFn: async (): Promise<{ ids: string[]; note?: string }> => {
      const data = (await fetchRemoteModels(platformId)) as RemoteListResponse
      const ids: string[] = []
      const seen = new Set<string>()
      for (const model of data?.models ?? []) {
        const id = (model?.id ?? '').trim()
        if (!id || seen.has(id)) continue
        seen.add(id)
        ids.push(id)
      }
      return { ids, note: data?.note }
    },
    enabled: opened && Boolean(platformId),
  })

  const known = useMemo(() => new Set(knownModelIds), [knownModelIds])
  const remote = remoteQuery.data?.ids ?? []
  const missing = useMemo(() => remote.filter(id => !known.has(id)), [remote, known])
  const visible = useMemo(() => {
    const needle = filter.trim().toLowerCase()
    return needle ? missing.filter(id => id.toLowerCase().includes(needle)) : missing
  }, [missing, filter])

  useEffect(() => {
    if (opened) {
      setPicked(new Set())
      setFilter('')
    }
  }, [opened])

  const allVisiblePicked = visible.length > 0 && visible.every(id => picked.has(id))
  const someVisiblePicked = visible.some(id => picked.has(id))

  const toggle = (id: string) => {
    setPicked(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const toggleVisible = () => {
    setPicked(prev => {
      const next = new Set(prev)
      visible.forEach(id => (allVisiblePicked ? next.delete(id) : next.add(id)))
      return next
    })
  }

  const runImport = async () => {
    const ids = [...picked]
    if (ids.length === 0) return
    setSaving(true)
    try {
      const result = await importPlatformModels(platformId, ids)
      notifications.show({
        message: t(locale, 'importModelsDone', String(result?.created ?? 0), String(result?.existing ?? 0)),
        color: 'green',
      })
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['platform-models', platformId] }),
        queryClient.invalidateQueries({ queryKey: ['platform-linked-models', platformId] }),
        queryClient.invalidateQueries({ queryKey: ['platform', platformId] }),
        queryClient.invalidateQueries({ queryKey: ['platforms'] }),
      ])
      setPicked(new Set())
      onClose()
    } catch (error: any) {
      notifications.show({
        message: t(locale, 'importModelsFailed', error?.response?.data?.error?.message || ''),
        color: 'red',
      })
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal opened={opened} onClose={onClose} title={t(locale, 'importModelsTitle')} size="lg">
      {remoteQuery.isPending ? (
        <div className="console-empty"><span>{t(locale, 'connectingPlatform')}</span></div>
      ) : remoteQuery.isError ? (
        <ErrorState
          title={t(locale, 'fetchRemoteModelsFailed')}
          description={(remoteQuery.error as any)?.response?.data?.error?.message}
          onRetry={() => void remoteQuery.refetch()}
        />
      ) : remote.length === 0 ? (
        <EmptyState
          title={t(locale, 'noRemoteModels')}
          description={remoteQuery.data?.note || t(locale, 'noRemoteModelsDesc')}
        />
      ) : (
        <>
          <div className="console-note">
            {t(locale, 'importModelsCounts', String(remote.length), String(remote.length - missing.length))}
          </div>
          <TextInput
            size="xs"
            mt={10}
            value={filter}
            placeholder={t(locale, 'importModelsFilter')}
            onChange={event => setFilter(event.currentTarget.value)}
          />
          <Stack gap={2} mt={10}>
            <Checkbox
              size="xs"
              label={t(locale, 'importModelsAll')}
              checked={allVisiblePicked}
              indeterminate={!allVisiblePicked && someVisiblePicked}
              disabled={visible.length === 0}
              onChange={toggleVisible}
            />
            <span className="muted" style={{ fontSize: 12 }}>
              {t(locale, 'importModelsPicked', String(picked.size))}
            </span>
          </Stack>
          <ScrollArea.Autosize mah={320} mt={8} type="auto">
            {visible.length > 0 ? (
              <Stack gap={6}>
                {visible.map(id => (
                  <Checkbox
                    key={id}
                    size="xs"
                    label={<span className="console-mono">{id}</span>}
                    checked={picked.has(id)}
                    onChange={() => toggle(id)}
                  />
                ))}
              </Stack>
            ) : (
              <div className="console-empty">
                <span>{filter.trim() ? t(locale, 'importModelsNoMatch') : t(locale, 'importModelsAllAdded')}</span>
              </div>
            )}
          </ScrollArea.Autosize>
        </>
      )}
      <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
        <Button size="xs" variant="default" onClick={onClose}>{t(locale, 'cancel')}</Button>
        <Button size="xs" loading={saving} disabled={picked.size === 0} onClick={() => void runImport()}>
          {t(locale, 'importModelsSubmit', String(picked.size))}
        </Button>
      </div>
    </Modal>
  )
}
