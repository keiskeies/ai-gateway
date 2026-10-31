import { useEffect, useState } from 'react'
import {
  Table,
  Modal,
  TextInput,
  Button,
  Group,
  Stack,
  Text,
  ActionIcon,
  Tooltip,
  CopyButton,
  Switch,
  Checkbox,
} from '@mantine/core'
import { useForm } from '@mantine/form'
import { useNavigate } from 'react-router-dom'
import { notifications } from '@mantine/notifications'
import {
  IconPlus,
  IconTrash,
  IconCopy,
  IconCheck,
  IconKey,
  IconAlertTriangle,
  IconRefresh,
  IconArrowsExchange,
  IconX,
  IconChartDots,
  IconPencil,
  IconPlayerPlay,
  IconPlayerPause,
} from '@tabler/icons-react'
import {
  listApiKeys,
  createApiKey,
  updateApiKey,
  rotateApiKey,
  deleteApiKey,
  batchApiKeys,
  type ApiKeyBatchAction,
  listProxies,
  getApiKeyUsage,
  getApiKeyModelUsage,
  type RangeOpts,
} from '../api'
import { rangeToParams, useAppState } from '../app/AppState'
import { formatCompact, formatCost, formatDuration, formatNumber } from '../lib/format'
import type { ApiKeyModelUsage, ApiKeyUsage } from '../types/stats'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import {
  KeyFields,
  costValue,
  emptyKeyForm,
  fromLocalInput,
  quotaValue,
  toLocalInput,
  windowLabel,
  windowValue,
  type KeyFormValues,
} from '../components/KeyFields'

function proxyNameFor(row: any, proxies: any[]): string | undefined {
  return row.proxy_name || proxies.find((proxy: any) => proxy.id === row.proxy_id)?.name
}

export default function ApiKeys() {
  const [keys, setKeys] = useState<any[]>([])
  const [proxies, setProxies] = useState<any[]>([])
  const [loading, setLoading] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [createdKey, setCreatedKey] = useState<string>('')
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null)
  const [togglingId, setTogglingId] = useState<string | null>(null)
  const [selected, setSelected] = useState<string[]>([])
  const [batchBusy, setBatchBusy] = useState(false)
  const [batchDeleteOpen, setBatchDeleteOpen] = useState(false)
  const [rotateId, setRotateId] = useState<string | null>(null)
  const [rotatedKey, setRotatedKey] = useState('')
  const [rotating, setRotating] = useState(false)
  const [usage, setUsage] = useState<Record<string, ApiKeyUsage>>({})
  const [detailId, setDetailId] = useState<string | null>(null)
  const [detailRows, setDetailRows] = useState<ApiKeyModelUsage[]>([])
  const [detailLoading, setDetailLoading] = useState(false)
  const { locale } = useAppContext()
  const { range } = useAppState()
  const navigate = useNavigate()

  const form = useForm<KeyFormValues>({
    initialValues: emptyKeyForm,
    validate: {
      name: (v) => (v.trim() ? null : (locale === 'zh' ? '请输入名称' : 'Name is required')),
    },
  })

  const [editId, setEditId] = useState<string | null>(null)
  const [editBusy, setEditBusy] = useState(false)
  const editForm = useForm<KeyFormValues & { enabled: boolean }>({
    initialValues: { ...emptyKeyForm, enabled: true },
    validate: {
      name: (v) => (v.trim() ? null : (locale === 'zh' ? '请输入名称' : 'Name is required')),
    },
  })

  useEffect(() => { loadAll() }, [])

  useEffect(() => { loadUsage() }, [range])

  const loadUsage = async () => {
    const rows: ApiKeyUsage[] = await getApiKeyUsage(rangeToParams(range)).catch(() => [])
    setUsage(Object.fromEntries((rows || []).map(item => [item.api_key_id, item])))
  }

  const openDetail = async (id: string) => {
    setDetailId(id)
    setDetailLoading(true)
    setDetailRows([])
    try {
      const rows: ApiKeyModelUsage[] = await getApiKeyModelUsage(id, rangeToParams(range))
      setDetailRows(rows || [])
    } catch {
      // 拉取失败时保持空列表，弹窗给出空态
    } finally {
      setDetailLoading(false)
    }
  }

  const loadAll = async () => {
    setLoading(true)
    try {
      const [keyList, proxyList] = await Promise.all([
        listApiKeys().catch(() => []),
        listProxies().catch(() => []),
      ])
      setKeys(keyList)
      setProxies(proxyList)
      // 选中项跟着列表收敛：别处删掉的 Key 留在选择里，批量就会多报一条"已不在库里"
      setSelected(prev => prev.filter(id => (keyList || []).some((row: any) => row.id === id)))
    } catch {
      // silently fail
    } finally {
      setLoading(false)
    }
  }

  const handleCreate = async (values: KeyFormValues) => {
    try {
      const result = await createApiKey({
        name: values.name,
        proxy_id: values.proxy_id || null,
        expires_at: fromLocalInput(values.expires_at),
        quota_requests: quotaValue(values.quota_requests),
        quota_tokens: quotaValue(values.quota_tokens),
        quota_cost: costValue(values.quota_cost),
        quota_window: windowValue(values.quota_window),
      })
      // 明文只在这一次响应里（secret）；列表里的 key 已经是掩码，不能当凭据用
      const fullKey = result.secret || ''
      setCreatedKey(fullKey)
      notifications.show({
        title: t(locale, 'createSuccess'),
        message: t(locale, 'apiKeyCreatedTip'),
        color: 'green',
      })
      loadAll()
    } catch {
      notifications.show({
        title: t(locale, 'createFailed'),
        message: '',
        color: 'red',
      })
    }
  }

  // 编辑弹窗按"整份表单提交"：空的到期时间就发 null（＝改成不过期），不做"没动过就不发"的猜测。
  // 这样三态语义只有一条规则，用户看到的和库里的不会分家。
  const openEdit = (row: any) => {
    setEditId(row.id)
    editForm.setValues({
      name: row.name || '',
      proxy_id: row.proxy_id || '',
      expires_at: toLocalInput(row.expires_at),
      quota_requests: row.quota_requests == null ? '' : String(row.quota_requests),
      quota_tokens: row.quota_tokens == null ? '' : String(row.quota_tokens),
      quota_cost: row.quota_cost == null ? '' : String(row.quota_cost),
      quota_window: row.quota_window ?? 'lifetime',
      enabled: row.enabled !== false,
    })
  }

  const closeEdit = () => {
    setEditId(null)
    editForm.reset()
  }

  const saveEdit = async (values: KeyFormValues & { enabled: boolean }) => {
    if (!editId) return
    setEditBusy(true)
    try {
      await updateApiKey(editId, {
        name: values.name.trim(),
        enabled: values.enabled,
        proxy_id: values.proxy_id || null,
        expires_at: fromLocalInput(values.expires_at),
        quota_requests: quotaValue(values.quota_requests),
        quota_tokens: quotaValue(values.quota_tokens),
        quota_cost: costValue(values.quota_cost),
        quota_window: windowValue(values.quota_window),
      })
      notifications.show({ message: t(locale, 'keyUpdated'), color: 'green' })
      closeEdit()
      await loadAll()
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'updateFailed'),
        message: error?.response?.data?.error?.message || '',
        color: 'red',
      })
    } finally {
      setEditBusy(false)
    }
  }

  const toggleKey = async (row: any, enabled: boolean) => {
    setTogglingId(row.id)
    try {
      await updateApiKey(row.id, { enabled })
      notifications.show({
        message: enabled
          ? (locale === 'zh' ? 'Key 已启用' : 'Key enabled')
          : (locale === 'zh' ? 'Key 已禁用' : 'Key disabled'),
        color: 'green',
      })
      await loadAll()
    } catch {
      notifications.show({ message: t(locale, 'updateFailed'), color: 'red' })
    } finally {
      setTogglingId(null)
    }
  }

  const handleDelete = async (id: string) => {
    try {
      await deleteApiKey(id)
      notifications.show({
        message: t(locale, 'deleteSuccess'),
        color: 'green',
      })
      setDeleteConfirmId(null)
      loadAll()
    } catch {
      // silently fail
    }
  }

  const allSelected = keys.length > 0 && selected.length === keys.length
  const toggleAllSelected = () => setSelected(allSelected ? [] : keys.map(row => row.id))
  const toggleSelected = (id: string) =>
    setSelected(prev => (prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id]))
  const selectedRows = keys.filter(row => selected.includes(row.id))
  const selectedEnabled = selectedRows.filter(row => row.enabled !== false).length

  // 批量走一条 SQL，但界面要报的是库里的真实改动数：affected 与 missing 分开说，
  // 否则"点了 5 个、只动了 3 个"会被读成"5 个都改好了"。
  const runBatch = async (action: ApiKeyBatchAction) => {
    if (!selected.length) return
    setBatchBusy(true)
    try {
      const result = await batchApiKeys(selected, action)
      const doneKey = action === 'enable'
        ? 'keyBatchEnabled'
        : action === 'disable' ? 'keyBatchDisabled' : 'keyBatchDeleted'
      notifications.show({
        message: t(locale, doneKey, String(result.affected))
          + (result.missing.length ? ` · ${t(locale, 'keyBatchMissing', String(result.missing.length))}` : ''),
        color: 'green',
      })
      setBatchDeleteOpen(false)
      // 启用/停用后保留选择：同一批人常常要连着做两步（先停用、再删）。
      // 删除必须清空——那些行已经不在了，留着就是一串点不动的勾。
      if (action === 'delete') setSelected([])
      await Promise.all([loadAll(), loadUsage()])
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'keyBatchFailed',
          error?.response?.data?.error?.message || (locale === 'zh' ? '请求失败' : 'Request failed')),
        message: '',
        color: 'red',
      })
    } finally {
      setBatchBusy(false)
    }
  }

  const handleRotate = async () => {
    if (!rotateId) return
    setRotating(true)
    try {
      const result = await rotateApiKey(rotateId)
      setRotatedKey(result.secret || '')
      notifications.show({ title: t(locale, 'rotateSuccess'), message: '', color: 'green' })
      await loadAll()
    } catch {
      notifications.show({ title: t(locale, 'rotateFailed'), message: '', color: 'red' })
      setRotateId(null)
    } finally {
      setRotating(false)
    }
  }

  const closeRotateModal = () => {
    setRotateId(null)
    setRotatedKey('')
  }

  const openCreateModal = () => {
    form.reset()
    setCreatedKey('')
    setCreateOpen(true)
  }

  const closeCreateModal = () => {
    setCreateOpen(false)
    setCreatedKey('')
    form.reset()
  }

  const proxyOptions = [
    { value: '', label: t(locale, 'apiKeyProxyAll') },
    ...proxies.map((p: any) => ({ value: p.id, label: p.name })),
  ]

  return (
    <div className="page">
      {/* ===== 页头 ===== */}
      <div className="page-head">
        <div>
          <div className="page-title">
            <span className="page-title-chip"><IconKey size={17} stroke={1.7} /></span>
            {t(locale, 'apiKeys')}
          </div>
          <div className="page-sub">
            {keys.length} {t(locale, 'total')}
          </div>
        </div>
        <div className="page-actions">
          <Button
            variant="default"
            leftSection={<IconRefresh size={15} />}
            onClick={loadAll}
            loading={loading}
          >
            {t(locale, 'refresh')}
          </Button>
          <Button leftSection={<IconPlus size={15} />} onClick={openCreateModal}>
            {t(locale, 'newApiKey')}
          </Button>
        </div>
      </div>

      {/* ===== 密钥列表 ===== */}
      <div className="ds-panel">
        {selected.length > 0 && (
          <div className="console-batch-bar">
            <span>
              {t(locale, 'batchSelected', String(selected.length))}
              {/* 混合状态写出来：不然"停用"按下去之后不知道本来有几条是启用的 */}
              <span className="ds-dim"> · {t(locale, 'keyBatchEnabledOf', String(selectedEnabled))}</span>
            </span>
            <Group gap={6}>
              <Button
                size="compact-xs"
                variant="default"
                loading={batchBusy}
                leftSection={<IconPlayerPlay size={12} />}
                onClick={() => void runBatch('enable')}
              >
                {t(locale, 'keyBatchEnable')}
              </Button>
              <Button
                size="compact-xs"
                variant="default"
                loading={batchBusy}
                leftSection={<IconPlayerPause size={12} />}
                onClick={() => void runBatch('disable')}
              >
                {t(locale, 'keyBatchDisable')}
              </Button>
              <Button
                size="compact-xs"
                variant="default"
                color="red"
                leftSection={<IconTrash size={12} />}
                onClick={() => setBatchDeleteOpen(true)}
              >
                {t(locale, 'keyBatchDelete')}
              </Button>
              <Button size="compact-xs" variant="subtle" onClick={() => setSelected([])}>
                {t(locale, 'batchClear')}
              </Button>
            </Group>
          </div>
        )}
        <div className="ds-table-wrap">
          <Table stickyHeader>
            <Table.Thead>
              <Table.Tr>
                <Table.Th className="console-col-check">
                  <Checkbox
                    size="xs"
                    aria-label={t(locale, 'batchSelectAll')}
                    checked={allSelected}
                    indeterminate={selected.length > 0 && !allSelected}
                    onChange={toggleAllSelected}
                  />
                </Table.Th>
                <Table.Th style={{ width: 180 }}>{t(locale, 'apiKeyName')}</Table.Th>
                <Table.Th>
                  <Tooltip label={t(locale, 'apiKeyMaskTip')}>
                    <span>{t(locale, 'apiKeyKey')}</span>
                  </Tooltip>
                </Table.Th>
                <Table.Th style={{ width: 150 }}>{t(locale, 'apiKeyProxy')}</Table.Th>
                <Table.Th style={{ width: 130 }}>{t(locale, 'status')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 100 }}>{t(locale, 'requests')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 110 }}>{t(locale, 'tokens')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 110 }}>{t(locale, 'avgLatencyShort')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 110 }}>
                  <Tooltip label={t(locale, 'costHint')}>
                    <span>{t(locale, 'cost')}</span>
                  </Tooltip>
                </Table.Th>
                <Table.Th style={{ width: 160 }}>{t(locale, 'apiKeyCreatedAt')}</Table.Th>
                <Table.Th style={{ width: 160 }}>{t(locale, 'lastUsed')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 180 }}>{t(locale, 'action')}</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {loading && keys.length === 0 && (
                <Table.Tr>
                  <Table.Td colSpan={12}>
                    <div className="ds-empty">{t(locale, 'loading')}</div>
                  </Table.Td>
                </Table.Tr>
              )}
              {!loading && keys.length === 0 && (
                <Table.Tr>
                  <Table.Td colSpan={12}>
                    <div className="ds-empty">
                      <IconKey size={28} stroke={1.2} />
                      <span>{locale === 'zh' ? '暂无 API Key，点击右上角创建' : 'No API keys yet. Create one from the top right.'}</span>
                    </div>
                  </Table.Td>
                </Table.Tr>
              )}
              {keys.map((row) => (
                <Table.Tr key={row.id}>
                  <Table.Td className="console-col-check">
                    <Checkbox
                      size="xs"
                      aria-label={row.name}
                      checked={selected.includes(row.id)}
                      onChange={() => toggleSelected(row.id)}
                    />
                  </Table.Td>
                  <Table.Td>
                    <button
                      type="button"
                      className="console-name-link"
                      onClick={() => navigate(`/keys/${row.id}`)}
                    >
                      {row.name}
                    </button>
                    {(() => {
                      const segments: string[] = []
                      if (row.quota_requests != null) segments.push(`${formatNumber(row.used_requests ?? 0)} / ${formatNumber(row.quota_requests)}`)
                      if (row.quota_tokens != null) segments.push(`${formatCompact(row.used_tokens ?? 0)} / ${formatCompact(row.quota_tokens)}`)
                      if (row.quota_cost != null) segments.push(`${formatCost(row.used_cost)} / ${formatCost(row.quota_cost)}`)
                      if (!segments.length) return null
                      const reached = (row.used_requests ?? 0) >= (row.quota_requests ?? Infinity)
                        || (row.used_tokens ?? 0) >= (row.quota_tokens ?? Infinity)
                        || (row.used_cost ?? 0) >= (row.quota_cost ?? Infinity)
                      return (
                        <Tooltip label={t(locale, 'quotaHint')}>
                          <div className="ds-dim" style={{ fontSize: 12 }}>
                            {segments.join(' · ')}
                            {row.quota_window ? ` · ${windowLabel(locale, row.quota_window)}` : ''}
                            {reached
                              ? <span className="ds-pill warn bare" style={{ marginLeft: 5 }}>{t(locale, 'quotaReached')}</span>
                              : null}
                          </div>
                        </Tooltip>
                      )
                    })()}
                  </Table.Td>
                  <Table.Td>
                    {/* 服务端已经掩码，这里没有可复制的凭据——所以这一格不给复制按钮 */}
                    <span className="ds-mono ds-dim" style={{ whiteSpace: 'nowrap' }}>{row.key}</span>
                  </Table.Td>
                  <Table.Td>
                    {(() => {
                      const binding = proxyNameFor(row, proxies)
                      if (binding) return <span className="ds-pill accent bare">{binding}</span>
                      // proxy_id 有值但模型列表里找不到：这个 Key 绑的虚拟模型已被删除，不能显示成"全部"
                      if (row.proxy_id) return (
                        <Tooltip label={t(locale, 'orphanProxyHint')}>
                          <span className="ds-pill warn bare">{t(locale, 'orphanProxyBinding')}</span>
                        </Tooltip>
                      )
                      return <span className="ds-pill neutral bare">{t(locale, 'apiKeyProxyAll')}</span>
                    })()}
                  </Table.Td>
                  <Table.Td>
                    <Group gap={7} wrap="nowrap">
                      <Switch
                        size="xs"
                        checked={row.enabled !== false}
                        disabled={togglingId === row.id}
                        onChange={event => void toggleKey(row, event.currentTarget.checked)}
                        aria-label={locale === 'zh' ? '启用或禁用 Key' : 'Enable or disable key'}
                      />
                      {/* 过期是独立状态：开关仍是"启用"，但请求已经在 401。合并显示才看得出这种"看着能用其实不能用" */}
                      {row.expired ? (
                        <Tooltip label={t(locale, 'expiredHint', row.expires_at ? new Date(row.expires_at).toLocaleString() : '')}>
                          <span className={`ds-pill ${row.enabled !== false ? 'warn' : 'neutral'} bare`}>
                            {row.enabled !== false ? t(locale, 'keyExpired') : t(locale, 'keyDisabled')}
                          </span>
                        </Tooltip>
                      ) : (
                        <span className={`ds-pill ${row.enabled !== false ? 'ok' : 'neutral'} bare`}>
                          {row.enabled !== false
                            ? (locale === 'zh' ? '启用' : 'Enabled')
                            : (locale === 'zh' ? '禁用' : 'Disabled')}
                        </span>
                      )}
                    </Group>
                  </Table.Td>
                  {(() => {
                    const item = usage[row.id]
                    return (
                      <>
                        <Table.Td className="ds-num">
                          <span className={item?.requests ? 'ds-strong' : 'ds-dim'}>
                            {item ? formatNumber(item.requests) : '—'}
                          </span>
                        </Table.Td>
                        <Table.Td className="ds-num">
                          <span className="ds-dim">
                            {item ? formatCompact(item.token_input + item.token_output) : '—'}
                          </span>
                        </Table.Td>
                        <Table.Td className="ds-num">
                          <span className="ds-dim">
                            {item?.requests ? formatDuration(item.avg_latency_ms) : '—'}
                          </span>
                        </Table.Td>
                        {/* 没定价、只填一侧或改价前的历史行都估算不出成本：留 —，不要写 0 */}
                        <Table.Td className="ds-num">
                          {item?.total_cost == null
                            ? <span className="ds-dim">—</span>
                            : item.priced_requests < item.requests
                              ? (
                                <Tooltip label={t(locale, 'costCoverageHint', String(item.priced_requests), String(item.requests))}>
                                  <span className="ds-dim">{formatCost(item.total_cost)}</span>
                                </Tooltip>
                              )
                              : <span className="ds-dim">{formatCost(item.total_cost)}</span>}
                        </Table.Td>
                      </>
                    )
                  })()}
                  <Table.Td>
                    <span className="ds-dim ds-num" style={{ fontSize: 12 }}>
                      {row.created_at ? new Date(row.created_at).toLocaleString() : '—'}
                    </span>
                  </Table.Td>
                  <Table.Td>
                    <span className="ds-dim ds-num" style={{ fontSize: 12 }}>
                      {row.last_used ? new Date(row.last_used).toLocaleString() : t(locale, 'neverUsed')}
                    </span>
                  </Table.Td>
                  <Table.Td className="ds-num">
                    <Group gap={4} wrap="nowrap" justify="flex-end">
                      <Tooltip label={t(locale, 'editKey')}>
                        <ActionIcon size="md" aria-label={t(locale, 'editKey')} onClick={() => openEdit(row)}>
                          <IconPencil size={16} />
                        </ActionIcon>
                      </Tooltip>
                      <Tooltip label={t(locale, 'keyModelUsage')}>
                        <ActionIcon size="md" aria-label={t(locale, 'keyModelUsage')} onClick={() => openDetail(row.id)}>
                          <IconChartDots size={16} />
                        </ActionIcon>
                      </Tooltip>
                      <Tooltip label={t(locale, 'rotateKey')}>
                        <ActionIcon size="md" aria-label={t(locale, 'rotateKey')} onClick={() => { setRotatedKey(''); setRotateId(row.id) }}>
                          <IconArrowsExchange size={16} />
                        </ActionIcon>
                      </Tooltip>
                      {deleteConfirmId === row.id ? (
                        <>
                          <Tooltip label={t(locale, 'confirmDelete')}>
                            <ActionIcon variant="filled" color="red" size="md" aria-label={t(locale, 'confirmDelete')} onClick={() => handleDelete(row.id)}>
                              <IconTrash size={15} />
                            </ActionIcon>
                          </Tooltip>
                          <Tooltip label={t(locale, 'cancel')}>
                            <ActionIcon size="md" aria-label={t(locale, 'cancel')} onClick={() => setDeleteConfirmId(null)}>
                              <IconX size={15} />
                            </ActionIcon>
                          </Tooltip>
                        </>
                      ) : (
                        <Tooltip label={t(locale, 'delete')}>
                          <ActionIcon size="md" color="red" aria-label={t(locale, 'delete')} onClick={() => setDeleteConfirmId(row.id)}>
                            <IconTrash size={16} />
                          </ActionIcon>
                        </Tooltip>
                      )}
                    </Group>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </div>
      </div>

      {/* ===== 新建密钥弹窗 ===== */}
      <Modal
        opened={createOpen}
        onClose={closeCreateModal}
        title={t(locale, 'newApiKey')}
        size="md"
        closeOnClickOutside={false}
      >
        {createdKey ? (
          <Stack gap="md">
            <div className="ds-hint">{t(locale, 'apiKeyCreatedTip')}</div>
            <div className="ds-keybox">
              <span className="ds-mono" style={{ color: 'var(--text-1)', wordBreak: 'break-all' }}>
                {createdKey}
              </span>
              <CopyButton value={createdKey}>
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
            <div className="ds-row" style={{ alignItems: 'flex-start' }}>
              <IconAlertTriangle size={14} style={{ color: 'var(--warn)', flexShrink: 0, marginTop: 2 }} />
              <span className="ds-hint" style={{ color: 'var(--warn)' }}>{t(locale, 'apiKeyCopyWarning')}</span>
            </div>
            <div className="ds-modal-footer">
              <Button onClick={closeCreateModal}>{t(locale, 'confirm')}</Button>
            </div>
          </Stack>
        ) : (
          <form onSubmit={form.onSubmit(handleCreate)}>
            <div className="ds-form-section">
              <div className="ds-form-section-title">{t(locale, 'basicInfo')}</div>
              <KeyFields
                getInputProps={form.getInputProps}
                locale={locale}
                proxyOptions={proxyOptions}
              />
            </div>
            <div className="ds-modal-footer">
              <Button variant="default" onClick={closeCreateModal}>
                {t(locale, 'cancel')}
              </Button>
              <Button type="submit">{t(locale, 'create')}</Button>
            </div>
          </form>
        )}
      </Modal>

      {/* ===== 编辑密钥弹窗：只改元数据，密钥本身要换请用轮换 ===== */}
      <Modal
        opened={editId !== null}
        onClose={closeEdit}
        title={t(locale, 'editApiKey')}
        size="md"
      >
        <form onSubmit={editForm.onSubmit(saveEdit)}>
          <div className="ds-form-section">
            <div className="ds-form-section-title">{t(locale, 'basicInfo')}</div>
            <KeyFields
              getInputProps={editForm.getInputProps}
              locale={locale}
              proxyOptions={proxyOptions}
            />
            {/* 不套 .console-field：那里的 label{display:block} 会把 Mantine 的标签挤到开关下面 */}
            <Switch
              mt={12}
              label={t(locale, 'status')}
              checked={editForm.values.enabled}
              onChange={event => editForm.setFieldValue('enabled', event.currentTarget.checked)}
            />
          </div>
          <div className="ds-modal-footer">
            <Button variant="default" onClick={closeEdit}>
              {t(locale, 'cancel')}
            </Button>
            <Button type="submit" loading={editBusy}>{t(locale, 'save')}</Button>
          </div>
        </form>
      </Modal>

      {/* ===== 批量删除确认：列出名字，并说清历史行的归属会断 ===== */}
      <Modal
        opened={batchDeleteOpen}
        onClose={() => setBatchDeleteOpen(false)}
        title={t(locale, 'keyBatchDeleteTitle')}
        size="md"
        closeOnClickOutside={false}
      >
        <Stack gap="md">
          <div className="ds-row" style={{ alignItems: 'flex-start' }}>
            <IconAlertTriangle size={14} style={{ color: 'var(--warn)', flexShrink: 0, marginTop: 2 }} />
            <span className="ds-hint" style={{ color: 'var(--warn)' }}>{t(locale, 'keyBatchDeleteLead')}</span>
          </div>
          <div className="ds-hint">{t(locale, 'keyBatchDeleteCount', String(selected.length))}</div>
          <Group gap={6}>
            {selectedRows.slice(0, 12).map(row => (
              <span className="ds-pill neutral bare" key={row.id}>{row.name}</span>
            ))}
            {selectedRows.length > 12 && (
              <span className="ds-dim" style={{ fontSize: 12 }}>
                {t(locale, 'keyBatchDeleteMore', String(selectedRows.length - 12))}
              </span>
            )}
          </Group>
          <div className="ds-modal-footer">
            <Button variant="default" onClick={() => setBatchDeleteOpen(false)}>{t(locale, 'cancel')}</Button>
            <Button color="red" loading={batchBusy} onClick={() => void runBatch('delete')}>
              {t(locale, 'confirmDelete')}
            </Button>
          </div>
        </Stack>
      </Modal>

      {/* ===== 密钥轮换弹窗 ===== */}
      <Modal
        opened={rotateId !== null}
        onClose={closeRotateModal}
        title={t(locale, 'rotateKeyTitle', keys.find(row => row.id === rotateId)?.name ?? '')}
        size="md"
        closeOnClickOutside={false}
      >
        {rotatedKey ? (
          <Stack gap="md">
            <div className="ds-hint">{t(locale, 'rotatedKeyTip')}</div>
            <div className="ds-keybox">
              <span className="ds-mono" style={{ color: 'var(--text-1)', wordBreak: 'break-all' }}>
                {rotatedKey}
              </span>
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
            <div className="ds-row" style={{ alignItems: 'flex-start' }}>
              <IconAlertTriangle size={14} style={{ color: 'var(--warn)', flexShrink: 0, marginTop: 2 }} />
              <span className="ds-hint" style={{ color: 'var(--warn)' }}>{t(locale, 'apiKeyCopyWarning')}</span>
            </div>
            <div className="ds-modal-footer">
              <Button onClick={closeRotateModal}>{t(locale, 'confirm')}</Button>
            </div>
          </Stack>
        ) : (
          <Stack gap="md">
            <div className="ds-row" style={{ alignItems: 'flex-start' }}>
              <IconAlertTriangle size={14} style={{ color: 'var(--warn)', flexShrink: 0, marginTop: 2 }} />
              <span className="ds-hint" style={{ color: 'var(--warn)' }}>{t(locale, 'rotateKeyWarning')}</span>
            </div>
            <div className="ds-modal-footer">
              <Button variant="default" onClick={closeRotateModal}>{t(locale, 'cancel')}</Button>
              <Button color="red" loading={rotating} onClick={() => void handleRotate()}>
                {t(locale, 'rotateKey')}
              </Button>
            </div>
          </Stack>
        )}
      </Modal>

      {/* ===== 密钥用量弹窗 ===== */}
      <Modal
        opened={detailId !== null}
        onClose={() => setDetailId(null)}
        title={`${keys.find(row => row.id === detailId)?.name || t(locale, 'keyUsage')} · ${t(locale, 'keyModelUsage')}`}
        size="xl"
      >
        <div className="ds-table-wrap">
          <Table highlightOnHover>
            <Table.Thead>
              <Table.Tr>
                <Table.Th style={{ minWidth: 200 }}>{t(locale, 'model')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 90 }}>{t(locale, 'requests')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 80 }}>{t(locale, 'failures')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 110 }}>{t(locale, 'tokens')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 110 }}>{t(locale, 'avgLatencyShort')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 100 }}>
                  <Tooltip label={t(locale, 'costHint')}>
                    <span>{t(locale, 'cost')}</span>
                  </Tooltip>
                </Table.Th>
                <Table.Th style={{ width: 170 }}>{t(locale, 'lastUsed')}</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {detailLoading && (
                <Table.Tr>
                  <Table.Td colSpan={7}>
                    <div className="ds-empty">{t(locale, 'loading')}</div>
                  </Table.Td>
                </Table.Tr>
              )}
              {!detailLoading && detailRows.length === 0 && (
                <Table.Tr>
                  <Table.Td colSpan={7}>
                    <div className="ds-empty">{t(locale, 'keyNoUsage')}</div>
                  </Table.Td>
                </Table.Tr>
              )}
              {detailRows.map(row => (
                <Table.Tr key={row.model_id}>
                  <Table.Td>
                    <div className="ds-strong">{row.model_id}</div>
                  </Table.Td>
                  <Table.Td className="ds-num">{formatNumber(row.requests)}</Table.Td>
                  <Table.Td className="ds-num">
                    {row.failed_requests
                      ? <span className="ds-pill warn bare">{formatNumber(row.failed_requests)}</span>
                      : <span className="ds-dim">—</span>}
                  </Table.Td>
                  <Table.Td className="ds-num"><span className="ds-dim">{formatCompact(row.token_input + row.token_output)}</span></Table.Td>
                  <Table.Td className="ds-num"><span className="ds-dim">{formatDuration(row.avg_latency_ms)}</span></Table.Td>
                  <Table.Td className="ds-num">
                    {row.total_cost == null
                      ? <span className="ds-dim">—</span>
                      : row.priced_requests < row.requests
                        ? (
                          <Tooltip label={t(locale, 'costCoverageHint', String(row.priced_requests), String(row.requests))}>
                            <span className="ds-dim">{formatCost(row.total_cost)}</span>
                          </Tooltip>
                        )
                        : <span className="ds-dim">{formatCost(row.total_cost)}</span>}
                  </Table.Td>
                  <Table.Td>
                    <span className="ds-dim ds-num" style={{ fontSize: 12 }}>
                      {row.last_used ? new Date(row.last_used).toLocaleString() : t(locale, 'neverUsed')}
                    </span>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </div>
        <div className="ds-modal-footer">
          <Button variant="default" onClick={() => setDetailId(null)}>{t(locale, 'close')}</Button>
        </div>
      </Modal>
    </div>
  )
}
