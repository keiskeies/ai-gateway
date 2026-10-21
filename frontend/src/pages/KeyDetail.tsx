import { useEffect, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, CopyButton, Modal, Stack, Switch, Table, Tooltip } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import {
  IconAlertTriangle,
  IconArrowsExchange,
  IconCheck,
  IconCopy,
  IconExternalLink,
  IconTrash,
} from '@tabler/icons-react'
import { useNavigate, useParams } from 'react-router-dom'
import {
  deleteApiKey,
  getApiKeyModelUsage,
  getApiKeyUsage,
  listApiKeys,
  listProxies,
  getRequestLogs,
  rotateApiKey,
  updateApiKey,
} from '../api'
import { rangeToParams, useAppState } from '../app/AppState'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { PageHeader } from '../components/ui/PageHeader'
import { EmptyState, ErrorState, PageSkeleton } from '../components/ui/States'
import { Readout } from '../components/ui/Readout'
import { StatusPill } from '../components/ui/StatusPill'
import { KeyFields, costValue, fromLocalInput, quotaValue, toLocalInput, windowLabel, windowValue, type KeyFormValues } from '../components/KeyFields'
import { formatCompact, formatCost, formatCostCoverage, formatDuration, formatLogTime, formatNumber } from '../lib/format'

interface KeyDraft extends KeyFormValues {
  enabled: boolean
}

const draftFrom = (row: any): KeyDraft => ({
  name: row.name || '',
  proxy_id: row.proxy_id || '',
  expires_at: toLocalInput(row.expires_at),
  quota_requests: row.quota_requests == null ? '' : String(row.quota_requests),
  quota_tokens: row.quota_tokens == null ? '' : String(row.quota_tokens),
  quota_cost: row.quota_cost == null ? '' : String(row.quota_cost),
  quota_window: row.quota_window ?? 'lifetime',
  enabled: row.enabled !== false,
})

// 脏判断必须按语义比，不能比字符串：库里存的是带时区的 RFC3339（还有小数秒），
// datetime-local 只到分钟，配额又可能是 number 或 string——直接 stringify 对一遍，
// 保存成功也会永远显示"未保存"。
const minuteOf = (value?: string | number | null) => {
  if (value == null || value === '') return ''
  const ms = new Date(value).getTime()
  return Number.isNaN(ms) ? '' : String(Math.floor(ms / 60000))
}

const isDirtyAgainst = (draft: KeyDraft | null, row?: any): boolean => {
  if (!draft || !row) return false
  return draft.name.trim() !== (row.name || '')
    || (draft.proxy_id || '') !== (row.proxy_id || '')
    || minuteOf(fromLocalInput(draft.expires_at)) !== minuteOf(row.expires_at)
    || quotaValue(draft.quota_requests) !== (row.quota_requests ?? null)
    || quotaValue(draft.quota_tokens) !== (row.quota_tokens ?? null)
    || costValue(draft.quota_cost) !== (row.quota_cost ?? null)
    || windowValue(draft.quota_window) !== (row.quota_window ?? null)
    || draft.enabled !== (row.enabled !== false)
}

export default function KeyDetail() {
  const { id = '' } = useParams()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { locale } = useAppContext()
  const { range } = useAppState()

  const keysQuery = useQuery({ queryKey: ['api-keys'], queryFn: listApiKeys })
  const proxiesQuery = useQuery({ queryKey: ['proxies'], queryFn: listProxies })
  const usageQuery = useQuery({
    queryKey: ['api-keys-usage', range],
    queryFn: () => getApiKeyUsage(rangeToParams(range)),
  })
  const modelUsageQuery = useQuery({
    queryKey: ['api-key-model-usage', id, range],
    queryFn: () => getApiKeyModelUsage(id, rangeToParams(range)),
    enabled: Boolean(id),
  })
  const recentQuery = useQuery({
    queryKey: ['api-key-requests', id, range],
    queryFn: () => getRequestLogs({ ...rangeToParams(range), api_key: id, page: 1, page_size: 10 }),
    enabled: Boolean(id),
  })

  const key = (keysQuery.data ?? []).find((row: any) => row.id === id)
  const proxies = proxiesQuery.data ?? []
  const usage = (usageQuery.data ?? []).find((row: any) => row.api_key_id === id)
  const modelRows = modelUsageQuery.data ?? []
  const recent = recentQuery.data?.items ?? []

  const [draft, setDraft] = useState<KeyDraft | null>(null)

  // 库里换了值（别的标签页改的、SSE 之后的 refetch）就跟随；本地有未保存改动时不动，避免编辑到一半被覆盖
  useEffect(() => {
    if (!key) return
    setDraft(current => (isDirtyAgainst(current, key) ? current : draftFrom(key)))
  }, [key])

  const [saving, setSaving] = useState(false)
  const [rotateOpen, setRotateOpen] = useState(false)
  const [rotatedKey, setRotatedKey] = useState('')
  const [rotating, setRotating] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)

  const dirty = isDirtyAgainst(draft, key)

  const setField = <K extends keyof KeyDraft>(field: K, value: KeyDraft[K]) => {
    setDraft(current => (current ? { ...current, [field]: value } : current))
  }

  // KeyFields 是给 @mantine/form 写的；这里用受控 state 也能喂它，保持字段与校验完全同源
  const getInputProps = (field: keyof KeyFormValues) => ({
    value: draft ? draft[field] : '',
    onChange: (event: any) => {
      const next = event?.target?.value ?? event
      setField(field, next as never)
    },
  })

  const save = async () => {
    if (!draft || !dirty) return
    setSaving(true)
    try {
      await updateApiKey(id, {
        name: draft.name.trim(),
        enabled: draft.enabled,
        proxy_id: draft.proxy_id || null,
        expires_at: fromLocalInput(draft.expires_at),
        quota_requests: quotaValue(draft.quota_requests),
        quota_tokens: quotaValue(draft.quota_tokens),
        quota_cost: costValue(draft.quota_cost),
        quota_window: windowValue(draft.quota_window),
      })
      notifications.show({ message: t(locale, 'keyUpdated'), color: 'green' })
      await queryClient.invalidateQueries({ queryKey: ['api-keys'] })
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'updateFailed'),
        message: error?.response?.data?.error?.message || t(locale, 'requestFailed'),
        color: 'red',
      })
    } finally {
      setSaving(false)
    }
  }

  const runRotate = async () => {
    setRotating(true)
    try {
      const result = await rotateApiKey(id)
      setRotatedKey(result.secret || '')
      notifications.show({ title: t(locale, 'rotateSuccess'), message: '', color: 'green' })
      await queryClient.invalidateQueries({ queryKey: ['api-keys'] })
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'rotateFailed'),
        message: error?.response?.data?.error?.message || t(locale, 'requestFailed'),
        color: 'red',
      })
    } finally {
      setRotating(false)
    }
  }

  const runDelete = async () => {
    try {
      await deleteApiKey(id)
      notifications.show({ message: t(locale, 'deleteSuccess'), color: 'green' })
      navigate('/keys')
      await queryClient.invalidateQueries({ queryKey: ['api-keys'] })
    } catch (error: any) {
      notifications.show({
        message: error?.response?.data?.error?.message || t(locale, 'requestFailed'),
        color: 'red',
      })
    }
  }

  if (keysQuery.isPending) return <PageSkeleton rows={5} />
  if (keysQuery.isError) return <ErrorState onRetry={() => void keysQuery.refetch()} />
  if (!key) {
    return (
      <div className="console-page">
        <EmptyState
          title={t(locale, 'keyNotFound')}
          description={t(locale, 'keyNotFoundDesc')}
          action={(
            <Button size="xs" onClick={() => navigate('/keys')}>{t(locale, 'backToList')}</Button>
          )}
        />
      </div>
    )
  }

  const binding = key.proxy_name || proxies.find((proxy: any) => proxy.id === key.proxy_id)?.name
  const orphan = Boolean(key.proxy_id && !binding)
  const expired = Boolean(key.expired)
  const totalTokens = usage ? usage.token_input + usage.token_output : 0
  const quotaHit = (key.used_requests ?? 0) >= (key.quota_requests ?? Infinity)
    || (key.used_tokens ?? 0) >= (key.quota_tokens ?? Infinity)
    || (key.used_cost ?? 0) >= (key.quota_cost ?? Infinity)

  return (
    <div className="console-page">
      <PageHeader
        kicker={<>{t(locale, 'kickerAccess')} <span className="console-kicker-sep" /> {t(locale, 'kickerApiKey')}</>}
        title={draft?.name || key.name}
        mono
        subtitle={`${key.key} · ${binding || (orphan ? t(locale, 'orphanProxyBinding') : t(locale, 'apiKeyProxyAll'))}`}
        actions={
          <>
            <Button size="xs" variant="default" onClick={() => navigate('/keys')}>
              {t(locale, 'backToList')}
            </Button>
            <Button
              size="xs"
              variant="default"
              leftSection={<IconExternalLink size={14} />}
              onClick={() => navigate(`/traffic?key=${encodeURIComponent(id)}`)}
            >
              {t(locale, 'keyOpenInTraffic')}
            </Button>
            <Button size="xs" loading={saving} disabled={!dirty} onClick={() => void save()}>
              {t(locale, 'save')}
            </Button>
          </>
        }
      />

      <div className="console-status-strip">
        <span className={`console-dot ${expired ? 'warn' : key.enabled === false ? 'idle' : 'ok'}`} />
        <strong>
          {expired
            ? t(locale, 'keyExpired')
            : key.enabled === false ? t(locale, 'keyDisabled') : t(locale, 'keyEnabled')}
        </strong>
        <span className="console-status-sep">·</span>
        <span>{t(locale, 'apiKeyCreatedAt')} {key.created_at ? new Date(key.created_at).toLocaleString() : '—'}</span>
        <span className="console-status-sep">·</span>
        <span>{t(locale, 'lastUsed')} {key.last_used ? new Date(key.last_used).toLocaleString() : t(locale, 'neverUsed')}</span>
        <span className="console-status-right">
          <span>{t(locale, 'keyExpires')} {key.expires_at ? new Date(key.expires_at).toLocaleString() : t(locale, 'keyNotSet')}</span>
          <span>{dirty ? t(locale, 'unsaved') : t(locale, 'saved')}</span>
        </span>
      </div>

      <div className="console-settings-grid">
        <section className="console-panel">
          <div className="console-panel-head">
            <div>
              <div className="console-panel-title">{t(locale, 'keyAccessPanel')}</div>
              <div className="console-panel-sub">{t(locale, 'keyAccessDesc')}</div>
            </div>
          </div>
          <div className="console-panel-body">
            {draft && (
              <>
                <KeyFields
                  getInputProps={getInputProps}
                  locale={locale}
                  proxyOptions={[
                    { value: '', label: t(locale, 'apiKeyProxyAll') },
                    ...proxies.map((proxy: any) => ({ value: proxy.id, label: proxy.name })),
                  ]}
                />
                <Switch
                  mt={12}
                  label={t(locale, 'status')}
                  checked={draft.enabled}
                  onChange={event => setField('enabled', event.currentTarget.checked)}
                />
                {expired && (
                  <div className="console-note warn">{t(locale, 'expiredHint', key.expires_at ? new Date(key.expires_at).toLocaleString() : '')}</div>
                )}
              </>
            )}
          </div>
        </section>

        <section className="console-panel">
          <div className="console-panel-head">
            <div>
              <div className="console-panel-title">{t(locale, 'apiKeyKey')}</div>
              <div className="console-panel-sub">{t(locale, 'keySecretDesc')}</div>
            </div>
          </div>
          <div className="console-panel-body">
            <div className="ds-keybox">
              <span className="ds-mono" style={{ color: 'var(--text-1)' }}>{key.key}</span>
              <Tooltip label={t(locale, 'apiKeyMaskTip')}>
                <span className="console-pill idle"><span className="console-dot idle" />{t(locale, 'masked')}</span>
              </Tooltip>
            </div>
            {key.quota_requests != null || key.quota_tokens != null || key.quota_cost != null ? (
              <div className="console-note">
                {t(locale, 'quotaHint')} · {formatNumber(key.used_requests ?? 0)} / {key.quota_requests ?? '∞'} · {formatCompact(key.used_tokens ?? 0)} / {key.quota_tokens == null ? '∞' : formatCompact(key.quota_tokens)}
                {key.quota_cost != null ? (
                  // 金额只覆盖已定价的请求，覆盖率不说出来这个数字就是半句话
                  <> · {formatCost(key.used_cost)} / {formatCost(key.quota_cost)} ({formatCostCoverage(key.priced_requests, key.used_requests)})</>
                ) : null}
                {key.quota_window ? ` · ${windowLabel(locale, key.quota_window)}` : ''}
                {quotaHit && <span className="ds-pill warn bare" style={{ marginLeft: 6 }}>{t(locale, 'quotaReached')}</span>}
              </div>
            ) : (
              <div className="console-note">{t(locale, 'quotaHint')}</div>
            )}
            <div className="console-inspector-actions">
              <Button
                size="xs"
                variant="default"
                leftSection={<IconArrowsExchange size={14} />}
                onClick={() => { setRotatedKey(''); setRotateOpen(true) }}
              >
                {t(locale, 'rotateKey')}
              </Button>
            </div>
          </div>
        </section>
      </div>

      <section className="console-panel">
        <div className="console-panel-head">
          <div>
            <div className="console-panel-title">{t(locale, 'keyUsage')}</div>
            <div className="console-panel-sub">{t(locale, 'keyUsageDesc')}</div>
          </div>
        </div>
        <div className="console-panel-body">
          <div className="console-readouts">
            <Readout
              label={t(locale, 'requests')}
              value={usage ? formatNumber(usage.requests) : '—'}
              meta={usage?.failures ? `${t(locale, 'failures')} ${formatNumber(usage.failures)}` : t(locale, 'noFailures')}
            />
            <Readout label={t(locale, 'tokens')} value={usage ? formatCompact(totalTokens) : '—'} meta={t(locale, 'inOut')} />
            <Readout
              label={t(locale, 'avgLatencyShort')}
              value={usage?.requests ? formatDuration(usage.avg_latency_ms) : '—'}
              meta={key.last_used ? `${t(locale, 'lastUsed')} ${new Date(key.last_used).toLocaleTimeString()}` : t(locale, 'neverUsed')}
            />
            <Readout
              label={t(locale, 'cost')}
              value={usage?.total_cost == null ? '—' : formatCost(usage.total_cost)}
              meta={usage?.total_cost == null ? t(locale, 'costUnpriced') : formatCostCoverage(usage.priced_requests, usage.requests)}
            />
          </div>

          {modelUsageQuery.isPending ? (
            <div className="console-empty"><span>{t(locale, 'loading')}</span></div>
          ) : modelRows.length === 0 ? (
            <div className="console-empty"><span>{t(locale, 'keyNoUsage')}</span></div>
          ) : (
            <div className="console-table-wrap console-table-scroll" style={{ maxHeight: 320 }}>
              <Table className="console-table">
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>{t(locale, 'model')}</Table.Th>
                    <Table.Th className="num">{t(locale, 'requests')}</Table.Th>
                    <Table.Th className="num">{t(locale, 'failures')}</Table.Th>
                    <Table.Th className="num">{t(locale, 'tokens')}</Table.Th>
                    <Table.Th className="num">{t(locale, 'avgLatencyShort')}</Table.Th>
                    <Table.Th className="num">
                      <Tooltip label={t(locale, 'costHint')}><span>{t(locale, 'cost')}</span></Tooltip>
                    </Table.Th>
                    <Table.Th>{t(locale, 'lastUsed')}</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {modelRows.map((row: any) => (
                    <Table.Tr key={row.model_id}>
                      <Table.Td><span className="console-strong console-mono">{row.model_id}</span></Table.Td>
                      <Table.Td className="num">{formatNumber(row.requests)}</Table.Td>
                      <Table.Td className="num">
                        {row.failed_requests
                          ? <span className="console-pill warn"><span className="console-dot warn" />{formatNumber(row.failed_requests)}</span>
                          : <span className="muted">—</span>}
                      </Table.Td>
                      <Table.Td className="num"><span className="muted">{formatCompact(row.token_input + row.token_output)}</span></Table.Td>
                      <Table.Td className="num"><span className="muted">{formatDuration(row.avg_latency_ms)}</span></Table.Td>
                      <Table.Td className="num">
                        {row.total_cost == null
                          ? <span className="muted">—</span>
                          : <span className="muted">{formatCost(row.total_cost)}</span>}
                      </Table.Td>
                      <Table.Td>
                        <span className="muted console-mono">
                          {row.last_used ? new Date(row.last_used).toLocaleString() : t(locale, 'neverUsed')}
                        </span>
                      </Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </div>
          )}
        </div>
      </section>

      <section className="console-panel">
        <div className="console-panel-head">
          <div>
            <div className="console-panel-title">{t(locale, 'keyRecentRequests')}</div>
            <div className="console-panel-sub">{t(locale, 'keyRecentDesc')}</div>
          </div>
          <span className="console-panel-sub">{recentQuery.data?.total ?? 0} {t(locale, 'total')}</span>
        </div>
        <div className="console-panel-body">
          {recentQuery.isPending ? (
            <div className="console-empty"><span>{t(locale, 'loading')}</span></div>
          ) : recent.length === 0 ? (
            <div className="console-empty"><span>{t(locale, 'keyNoUsage')}</span></div>
          ) : (
            <div className="console-table-wrap console-table-scroll">
              <Table className="console-table">
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>{t(locale, 'time')}</Table.Th>
                    <Table.Th>{t(locale, 'apiKeyProxy')}</Table.Th>
                    <Table.Th>{t(locale, 'model')}</Table.Th>
                    <Table.Th className="num">{t(locale, 'status')}</Table.Th>
                    <Table.Th className="num">{t(locale, 'latency')}</Table.Th>
                    <Table.Th className="num">{t(locale, 'cost')}</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {recent.map((row: any) => (
                    <Table.Tr key={row.id}>
                      <Table.Td><span className="console-mono">{formatLogTime(row.created_at, locale)}</span></Table.Td>
                      <Table.Td>{row.proxy_name || '—'}</Table.Td>
                      <Table.Td><span className="console-mono muted">{row.model_id}</span></Table.Td>
                      <Table.Td className="num">
                        <span className={`console-pill ${row.status_code >= 200 && row.status_code < 300 ? 'ok' : 'err'}`}>
                          <span className={`console-dot ${row.status_code >= 200 && row.status_code < 300 ? 'ok' : 'err'}`} />
                          {row.status_code}
                        </span>
                      </Table.Td>
                      <Table.Td className="num">{formatDuration(row.latency_ms)}</Table.Td>
                      <Table.Td className="num">{row.cost == null ? <span className="muted">—</span> : formatCost(row.cost)}</Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </div>
          )}
        </div>
      </section>

      <section className="console-panel">
        <div className="console-panel-head">
          <div className="console-panel-title">{t(locale, 'dangerZone')}</div>
          <span className="console-panel-sub">{t(locale, 'irreversible')}</span>
        </div>
        <div className="console-panel-body">
          <div className="console-note warn">{t(locale, 'keyDeleteLead')}</div>
          <div className="console-inspector-actions">
            <Button size="xs" color="red" variant="default" leftSection={<IconTrash size={14} />} onClick={() => setDeleteOpen(true)}>
              {t(locale, 'delete')}
            </Button>
          </div>
        </div>
      </section>

      <Modal
        opened={rotateOpen}
        onClose={() => { setRotateOpen(false); setRotatedKey('') }}
        title={t(locale, 'rotateKeyTitle', key.name)}
        size="md"
        closeOnClickOutside={false}
      >
        {rotatedKey ? (
          <Stack gap="sm">
            <div className="ds-hint">{t(locale, 'rotatedKeyTip')}</div>
            <div className="ds-keybox">
              <span className="ds-mono" style={{ color: 'var(--text-1)', wordBreak: 'break-all' }}>{rotatedKey}</span>
              <CopyButton value={rotatedKey}>
                {({ copied, copy }) => (
                  <Button
                    size="xs"
                    variant={copied ? 'filled' : 'light'}
                    leftSection={copied ? <IconCheck size={14} /> : <IconCopy size={14} />}
                    onClick={copy}
                    style={{ flexShrink: 0 }}
                  >
                    {copied ? (locale === 'zh' ? '已复制' : 'Copied') : (locale === 'zh' ? '复制' : 'Copy')}
                  </Button>
                )}
              </CopyButton>
            </div>
            <div className="console-note warn">{t(locale, 'apiKeyCopyWarning')}</div>
            <div className="console-inspector-actions">
              <Button size="xs" onClick={() => { setRotateOpen(false); setRotatedKey('') }}>{t(locale, 'close')}</Button>
            </div>
          </Stack>
        ) : (
          <Stack gap="sm">
            <div className="console-note warn">{t(locale, 'rotateKeyWarning')}</div>
            <div className="console-inspector-actions">
              <Button size="xs" variant="default" onClick={() => setRotateOpen(false)}>{t(locale, 'cancel')}</Button>
              <Button size="xs" color="red" loading={rotating} onClick={() => void runRotate()}>{t(locale, 'rotateKey')}</Button>
            </div>
          </Stack>
        )}
      </Modal>

      <Modal
        opened={deleteOpen}
        onClose={() => setDeleteOpen(false)}
        title={t(locale, 'confirmDelete')}
        size="sm"
      >
        <Stack gap="sm">
          <div className="console-note warn">{t(locale, 'keyDeleteConfirm', key.name)}</div>
          <div className="console-inspector-actions">
            <Button size="xs" variant="default" onClick={() => setDeleteOpen(false)}>{t(locale, 'cancel')}</Button>
            <Button size="xs" color="red" leftSection={<IconAlertTriangle size={14} />} onClick={() => void runDelete()}>
              {t(locale, 'delete')}
            </Button>
          </div>
        </Stack>
      </Modal>
    </div>
  )
}
