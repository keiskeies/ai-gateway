import { useEffect, useState } from 'react'
import {
  Table,
  Modal,
  TextInput,
  Select,
  Button,
  Group,
  Stack,
  Text,
  ActionIcon,
  Tooltip,
  CopyButton,
} from '@mantine/core'
import { useForm } from '@mantine/form'
import { notifications } from '@mantine/notifications'
import {
  IconPlus,
  IconTrash,
  IconCopy,
  IconCheck,
  IconKey,
  IconAlertTriangle,
  IconRefresh,
  IconX,
} from '@tabler/icons-react'
import { listApiKeys, createApiKey, deleteApiKey, listProxies } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'

function maskKey(key: string): string {
  if (!key || key.length <= 8) return 'sk-****'
  return key.slice(0, 4) + '••••••••' + key.slice(-4)
}

export default function ApiKeys() {
  const [keys, setKeys] = useState<any[]>([])
  const [proxies, setProxies] = useState<any[]>([])
  const [loading, setLoading] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [createdKey, setCreatedKey] = useState<string>('')
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null)
  const { locale } = useAppContext()

  const form = useForm({
    initialValues: {
      name: '',
      proxy_id: '' as string,
    },
    validate: {
      name: (v) => (v.trim() ? null : (locale === 'zh' ? '请输入名称' : 'Name is required')),
    },
  })

  useEffect(() => { loadAll() }, [])

  const loadAll = async () => {
    setLoading(true)
    try {
      const [keyList, proxyList] = await Promise.all([
        listApiKeys().catch(() => []),
        listProxies().catch(() => []),
      ])
      setKeys(keyList)
      setProxies(proxyList)
    } catch {
      // silently fail
    } finally {
      setLoading(false)
    }
  }

  const handleCreate = async (values: { name: string; proxy_id: string }) => {
    try {
      const result = await createApiKey({
        name: values.name,
        proxy_id: values.proxy_id || null,
      })
      const fullKey = result.key || ''
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
        <div className="ds-table-wrap">
          <Table stickyHeader>
            <Table.Thead>
              <Table.Tr>
                <Table.Th style={{ width: 200 }}>{t(locale, 'apiKeyName')}</Table.Th>
                <Table.Th>{t(locale, 'apiKeyKey')}</Table.Th>
                <Table.Th style={{ width: 180 }}>{t(locale, 'apiKeyProxy')}</Table.Th>
                <Table.Th style={{ width: 190 }}>{t(locale, 'apiKeyCreatedAt')}</Table.Th>
                <Table.Th style={{ width: 190 }}>{t(locale, 'lastUsed')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 110 }}>{t(locale, 'action')}</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {loading && keys.length === 0 && (
                <Table.Tr>
                  <Table.Td colSpan={6}>
                    <div className="ds-empty">{t(locale, 'loading')}</div>
                  </Table.Td>
                </Table.Tr>
              )}
              {!loading && keys.length === 0 && (
                <Table.Tr>
                  <Table.Td colSpan={6}>
                    <div className="ds-empty">
                      <IconKey size={28} stroke={1.2} />
                      <span>{locale === 'zh' ? '暂无 API Key，点击右上角创建' : 'No API keys yet. Create one from the top right.'}</span>
                    </div>
                  </Table.Td>
                </Table.Tr>
              )}
              {keys.map((row) => (
                <Table.Tr key={row.id}>
                  <Table.Td>
                    <div className="ds-strong">{row.name}</div>
                  </Table.Td>
                  <Table.Td>
                    <Group gap={6} wrap="nowrap">
                      <span className="ds-mono ds-dim">{maskKey(row.key)}</span>
                      <CopyButton value={row.key}>
                        {({ copied, copy }) => (
                          <Tooltip label={copied ? (locale === 'zh' ? '已复制' : 'Copied') : (locale === 'zh' ? '复制' : 'Copy')}>
                            <ActionIcon size="sm" onClick={copy} style={copied ? { color: 'var(--ok)' } : undefined}>
                              {copied ? <IconCheck size={14} /> : <IconCopy size={14} />}
                            </ActionIcon>
                          </Tooltip>
                        )}
                      </CopyButton>
                    </Group>
                  </Table.Td>
                  <Table.Td>
                    {row.proxy_name ? (
                      <span className="ds-pill accent bare">{row.proxy_name}</span>
                    ) : (
                      <span className="ds-pill neutral bare">{t(locale, 'apiKeyProxyAll')}</span>
                    )}
                  </Table.Td>
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
                      {deleteConfirmId === row.id ? (
                        <>
                          <Tooltip label={t(locale, 'confirmDelete')}>
                            <ActionIcon variant="filled" color="red" size="md" onClick={() => handleDelete(row.id)}>
                              <IconTrash size={15} />
                            </ActionIcon>
                          </Tooltip>
                          <Tooltip label={t(locale, 'cancel')}>
                            <ActionIcon size="md" onClick={() => setDeleteConfirmId(null)}>
                              <IconX size={15} />
                            </ActionIcon>
                          </Tooltip>
                        </>
                      ) : (
                        <Tooltip label={t(locale, 'delete')}>
                          <ActionIcon size="md" color="red" onClick={() => setDeleteConfirmId(row.id)}>
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
              <Stack gap="sm">
                <TextInput
                  label={t(locale, 'apiKeyName')}
                  placeholder={t(locale, 'apiKeyNamePlaceholder')}
                  required
                  {...form.getInputProps('name')}
                />
                <Select
                  label={t(locale, 'apiKeyProxy')}
                  description={t(locale, 'apiKeyProxyAll')}
                  placeholder={t(locale, 'apiKeyProxyAll')}
                  data={proxyOptions}
                  clearable
                  {...form.getInputProps('proxy_id')}
                />
              </Stack>
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
    </div>
  )
}
