import { useEffect, useState } from 'react'
import {
  Table,
  Modal,
  Select,
  TextInput,
  PasswordInput,
  Button,
  Group,
  Stack,
  ActionIcon,
  Tooltip,
} from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import { notifications } from '@mantine/notifications'
import {
  IconPlus,
  IconEdit,
  IconTrash,
  IconRefresh,
  IconPlugConnected,
  IconX,
} from '@tabler/icons-react'
import { listPlatforms, createPlatform, updatePlatform, deletePlatform } from '../api'
import { useAppContext } from '../ThemeContext'
import { t, type Locale, type TranslationKey } from '../i18n'
import { platformPresets, getPresetName } from '../presets'

const PLATFORM_TYPES = [
  { value: 'OpenAI', labelKey: 'openaiType' as TranslationKey },
  { value: 'Anthropic', labelKey: 'anthropicType' as TranslationKey },
  { value: 'Ollama', labelKey: 'ollamaType' as TranslationKey },
  { value: 'Azure', labelKey: 'azureType' as TranslationKey },
  { value: 'Custom', labelKey: 'customType' as TranslationKey },
]

function getPlatformTypeLabel(value: string, locale: Locale): string {
  const entry = PLATFORM_TYPES.find(pt => pt.value === value)
  return entry ? t(locale, entry.labelKey) : value
}

interface PlatformFormValues {
  name: string
  type: string
  base_url: string
  api_key: string
  organization: string
}

const emptyForm: PlatformFormValues = {
  name: '',
  type: 'OpenAI',
  base_url: '',
  api_key: '',
  organization: '',
}

export default function Platforms() {
  const [platforms, setPlatforms] = useState<any[]>([])
  const [loading, setLoading] = useState(false)
  const [modalOpen, { open: openModal, close: closeModal }] = useDisclosure(false)
  const [editItem, setEditItem] = useState<any>(null)
  const [formValues, setFormValues] = useState<PlatformFormValues>({ ...emptyForm })
  const [presetValue, setPresetValue] = useState<string | null>(null)
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null)
  const { locale } = useAppContext()

  useEffect(() => { loadPlatforms() }, [])

  const loadPlatforms = async () => {
    setLoading(true)
    try {
      setPlatforms(await listPlatforms())
    } catch {
      notifications.show({
        title: t(locale, 'loadFailed'),
        message: '',
        color: 'red',
      })
    }
    setLoading(false)
  }

  const openCreate = () => {
    setEditItem(null)
    setFormValues({ ...emptyForm })
    setPresetValue(null)
    openModal()
  }

  const openEdit = (record: any) => {
    setEditItem(record)
    setFormValues({
      name: record.name || '',
      type: record.type || 'OpenAI',
      base_url: record.base_url || '',
      api_key: record.api_key || '',
      organization: record.organization || '',
    })
    setPresetValue(null)
    openModal()
  }

  const applyPreset = (presetName: string | null) => {
    setPresetValue(presetName)
    if (!presetName) return
    const preset = platformPresets.find(p => p.name === presetName)
    if (preset) {
      setFormValues(prev => ({
        ...prev,
        name: preset.name,
        type: preset.platform_type,
        base_url: preset.base_url,
      }))
    }
  }

  const handleSubmit = async () => {
    if (!formValues.name || !formValues.type || !formValues.base_url) return

    const payload = {
      name: formValues.name,
      type: formValues.type,
      base_url: formValues.base_url,
      api_key: formValues.api_key,
      organization: formValues.organization || undefined,
    }

    try {
      if (editItem) {
        await updatePlatform(editItem.id, payload)
        notifications.show({
          message: t(locale, 'updateSuccess'),
          color: 'green',
        })
      } else {
        await createPlatform(payload)
        notifications.show({
          message: t(locale, 'createSuccess'),
          color: 'green',
        })
      }
      closeModal()
      setEditItem(null)
      setFormValues({ ...emptyForm })
      loadPlatforms()
    } catch (e: any) {
      notifications.show({
        title: editItem ? t(locale, 'updateFailed') : t(locale, 'createFailed'),
        message: e?.response?.data?.error?.message || '',
        color: 'red',
      })
    }
  }

  const handleDelete = async (id: string) => {
    try {
      await deletePlatform(id)
      notifications.show({
        message: t(locale, 'deleteSuccess'),
        color: 'green',
      })
      setDeleteConfirmId(null)
      loadPlatforms()
    } catch {
      notifications.show({
        message: t(locale, 'loadFailed'),
        color: 'red',
      })
    }
  }

  const updateField = <K extends keyof PlatformFormValues>(key: K, value: PlatformFormValues[K]) => {
    setFormValues(prev => ({ ...prev, [key]: value }))
  }

  const presetSelectData = platformPresets.map(p => ({
    value: p.name,
    label: getPresetName(p, locale),
  }))

  const typeSelectData = PLATFORM_TYPES.map(pt => ({
    value: pt.value,
    label: t(locale, pt.labelKey),
  }))

  return (
    <div className="page">
      {/* ===== 页头 ===== */}
      <div className="page-head">
        <div>
          <div className="page-title">
            <span className="page-title-chip"><IconPlugConnected size={17} stroke={1.7} /></span>
            {t(locale, 'platforms')}
          </div>
          <div className="page-sub">
            {platforms.length} {t(locale, 'total')}
          </div>
        </div>
        <div className="page-actions">
          <Button
            variant="default"
            leftSection={<IconRefresh size={15} />}
            onClick={loadPlatforms}
            loading={loading}
          >
            {t(locale, 'refresh')}
          </Button>
          <Button
            leftSection={<IconPlus size={15} />}
            onClick={openCreate}
          >
            {t(locale, 'addPlatform')}
          </Button>
        </div>
      </div>

      {/* ===== 平台列表 ===== */}
      <div className="ds-panel">
        <div className="ds-table-wrap">
          <Table stickyHeader>
            <Table.Thead>
              <Table.Tr>
                <Table.Th style={{ width: 200 }}>{t(locale, 'name')}</Table.Th>
                <Table.Th style={{ width: 150 }}>{t(locale, 'type')}</Table.Th>
                <Table.Th>{t(locale, 'baseUrl')}</Table.Th>
                <Table.Th style={{ width: 200 }}>API Key</Table.Th>
                <Table.Th className="ds-num" style={{ width: 110 }}>{t(locale, 'action')}</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {loading && platforms.length === 0 && (
                <Table.Tr>
                  <Table.Td colSpan={5}>
                    <div className="ds-empty">{t(locale, 'loading')}</div>
                  </Table.Td>
                </Table.Tr>
              )}
              {!loading && platforms.length === 0 && (
                <Table.Tr>
                  <Table.Td colSpan={5}>
                    <div className="ds-empty">
                      <IconPlugConnected size={28} stroke={1.2} />
                      <span>{t(locale, 'noPlatforms')}</span>
                    </div>
                  </Table.Td>
                </Table.Tr>
              )}
              {platforms.map((platform) => {
                const preset = platformPresets.find(p => p.name === platform.name)
                const displayName = preset ? getPresetName(preset, locale) : platform.name

                return (
                  <Table.Tr key={platform.id}>
                    <Table.Td>
                      <div className="ds-strong">{displayName}</div>
                    </Table.Td>
                    <Table.Td>
                      <span className="ds-pill neutral bare">{getPlatformTypeLabel(platform.type, locale)}</span>
                    </Table.Td>
                    <Table.Td>
                      <div
                        className="ds-mono ds-dim"
                        title={platform.base_url}
                        style={{ maxWidth: 360, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
                      >
                        {platform.base_url}
                      </div>
                    </Table.Td>
                    <Table.Td>
                      {platform.api_key ? (
                        <span className="ds-mono ds-dim" title={platform.api_key.slice(0, 8)}>
                          {platform.api_key.slice(0, 8)}••••••
                        </span>
                      ) : (
                        <span className="ds-dim">—</span>
                      )}
                    </Table.Td>
                    <Table.Td className="ds-num">
                      <Group gap={4} wrap="nowrap" justify="flex-end">
                        <Tooltip label={t(locale, 'editPlatform')}>
                          <ActionIcon size="md" onClick={() => openEdit(platform)}>
                            <IconEdit size={16} />
                          </ActionIcon>
                        </Tooltip>
                        {deleteConfirmId === platform.id ? (
                          <>
                            <Tooltip label={t(locale, 'confirmDelete')}>
                              <ActionIcon variant="filled" color="red" size="md" onClick={() => handleDelete(platform.id)}>
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
                            <ActionIcon size="md" color="red" onClick={() => setDeleteConfirmId(platform.id)}>
                              <IconTrash size={16} />
                            </ActionIcon>
                          </Tooltip>
                        )}
                      </Group>
                    </Table.Td>
                  </Table.Tr>
                )
              })}
            </Table.Tbody>
          </Table>
        </div>
      </div>

      {/* ===== 新增 / 编辑弹窗 ===== */}
      <Modal
        opened={modalOpen}
        onClose={() => { closeModal(); setEditItem(null) }}
        title={editItem ? t(locale, 'editPlatform') : t(locale, 'addPlatform')}
        size="lg"
        closeOnClickOutside={false}
      >
        {!editItem && (
          <div className="ds-form-section">
            <div className="ds-form-section-title">{t(locale, 'quickPreset')}</div>
            <Select
              placeholder={t(locale, 'quickPreset')}
              data={presetSelectData}
              value={presetValue}
              onChange={applyPreset}
              searchable
              clearable
            />
          </div>
        )}

        <div className="ds-form-section">
          <div className="ds-form-section-title">{t(locale, 'basicInfo')}</div>
          <Stack gap="sm">
            <TextInput
              label={t(locale, 'platformName')}
              placeholder="OpenAI, DeepSeek, etc."
              required
              value={formValues.name}
              onChange={(e) => updateField('name', e.currentTarget.value)}
            />
            <Select
              label={t(locale, 'platformType')}
              data={typeSelectData}
              required
              value={formValues.type}
              onChange={(val) => updateField('type', val || 'OpenAI')}
            />
            <TextInput
              label={t(locale, 'baseUrl')}
              placeholder="https://api.openai.com/v1"
              required
              value={formValues.base_url}
              onChange={(e) => updateField('base_url', e.currentTarget.value)}
            />
          </Stack>
        </div>

        <div className="ds-form-section">
          <div className="ds-form-section-title">{t(locale, 'credentials')}</div>
          <Stack gap="sm">
            <PasswordInput
              label={t(locale, 'apiKey')}
              placeholder="sk-..."
              value={formValues.api_key}
              onChange={(e) => updateField('api_key', e.currentTarget.value)}
            />
            <TextInput
              label={t(locale, 'organization')}
              placeholder="org-xxx"
              description="Optional"
              value={formValues.organization}
              onChange={(e) => updateField('organization', e.currentTarget.value)}
            />
          </Stack>
        </div>

        <div className="ds-modal-footer">
          <Button variant="default" onClick={() => { closeModal(); setEditItem(null) }}>
            {t(locale, 'cancel')}
          </Button>
          <Button
            onClick={handleSubmit}
            disabled={!formValues.name || !formValues.type || !formValues.base_url}
          >
            {editItem ? t(locale, 'save') : t(locale, 'create')}
          </Button>
        </div>
      </Modal>
    </div>
  )
}
