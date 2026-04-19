import { useEffect, useState } from 'react'
import { Table, Button, Modal, Form, Input, InputNumber, Select, Tag, Space, message, Popconfirm, Card, Typography, Tooltip, Divider } from 'antd'
import { PlusOutlined, DeleteOutlined, EditOutlined, SearchOutlined, ApiOutlined, CheckCircleOutlined, CloseCircleOutlined, LoadingOutlined, CloudDownloadOutlined } from '@ant-design/icons'
import { listModels, createModel, updateModel, deleteModel, listPlatforms, testModelConnection, fetchRemoteModels } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { getModelsForPlatform, getModelDisplayName, getPresetName, platformPresets, CAPABILITY_OPTIONS, getCapabilityLabel, getCapabilityColor } from '../presets'

const { Text, Title } = Typography

export default function Models() {
  const [models, setModels] = useState<any[]>([])
  const [platforms, setPlatforms] = useState<any[]>([])
  const [modalOpen, setModalOpen] = useState(false)
  const [editItem, setEditItem] = useState<any>(null)
  const [form] = Form.useForm()
  const [loading, setLoading] = useState(false)
  const [selectedPlatformName, setSelectedPlatformName] = useState<string>('')
  const [useCustomModelId, setUseCustomModelId] = useState(false)
  const [customModelId, setCustomModelId] = useState('')
  const [testingId, setTestingId] = useState<string | null>(null)
  const [testResults, setTestResults] = useState<Record<string, { success: boolean; latency_ms: number; message: string }>>({})
  const [remoteModels, setRemoteModels] = useState<{ id: string; owned_by: string }[]>([])
  const [fetchingRemote, setFetchingRemote] = useState(false)
  const [showRemoteModels, setShowRemoteModels] = useState(false)
  const { locale } = useAppContext()

  useEffect(() => { loadModels(); loadPlatforms() }, [])

  const loadModels = async () => {
    setLoading(true)
    try { setModels(await listModels()) } catch {}
    setLoading(false)
  }

  const loadPlatforms = async () => {
    try { setPlatforms(await listPlatforms()) } catch {}
  }

  const openCreate = () => {
    setEditItem(null)
    form.resetFields()
    setSelectedPlatformName('')
    setUseCustomModelId(false)
    setCustomModelId('')
    setModalOpen(true)
  }

  const openEdit = (record: any) => {
    setEditItem(record)
    const plat = platforms.find((p: any) => p.id === record.platform_id)
    setSelectedPlatformName(plat?.name || '')
    // Check if the model_id is in presets
    const presetModels = getModelsForPlatform(plat?.name || '')
    const isPreset = presetModels.some(m => m.model_id === record.model_id)
    setUseCustomModelId(!isPreset)
    setCustomModelId(isPreset ? '' : record.model_id)
    form.setFieldsValue({
      platform_id: record.platform_id,
      model_id: isPreset ? record.model_id : undefined,
      display_name: record.display_name,
      max_tokens: record.max_tokens,
      context_window: record.context_window,
      capabilities: record.capabilities || [],
    })
    setModalOpen(true)
  }

  const handlePlatformChange = (platformId: string) => {
    const plat = platforms.find((p: any) => p.id === platformId)
    setSelectedPlatformName(plat?.name || '')
    setUseCustomModelId(false)
    setCustomModelId('')
    setRemoteModels([])
    setShowRemoteModels(false)
    form.setFieldsValue({ model_id: undefined, display_name: undefined })
  }

  const handleModelPresetSelect = (modelId: string) => {
    const presetModels = getModelsForPlatform(selectedPlatformName)
    const preset = presetModels.find(m => m.model_id === modelId)
    if (preset) {
      form.setFieldsValue({
        model_id: preset.model_id,
        display_name: getModelDisplayName(preset, locale),
        max_tokens: preset.max_tokens,
        context_window: preset.context_window,
        capabilities: preset.capabilities || [],
      })
    }
  }

  const handleFetchRemoteModels = async () => {
    const platformId = form.getFieldValue('platform_id')
    if (!platformId) {
      message.warning(t(locale, 'selectPlatform'))
      return
    }
    setFetchingRemote(true)
    try {
      const data = await fetchRemoteModels(platformId)
      const models = data.models || []
      if (models.length === 0) {
        message.info(t(locale, 'noRemoteModels'))
      }
      setRemoteModels(models)
      setShowRemoteModels(true)
    } catch (e: any) {
      message.error(t(locale, 'fetchRemoteModelsFailed'))
    }
    setFetchingRemote(false)
  }

  const handleRemoteModelSelect = (modelId: string) => {
    // Try to find preset info for this model
    const presetModels = getModelsForPlatform(selectedPlatformName)
    const preset = presetModels.find(m => m.model_id === modelId)
    if (preset) {
      form.setFieldsValue({
        model_id: preset.model_id,
        display_name: getModelDisplayName(preset, locale),
        max_tokens: preset.max_tokens,
        context_window: preset.context_window,
        capabilities: preset.capabilities || [],
      })
    } else {
      // No preset — use model_id as display name and reasonable defaults
      form.setFieldsValue({
        model_id: modelId,
        display_name: modelId,
        max_tokens: 4096,
        context_window: 8192,
        capabilities: [],
      })
    }
    setUseCustomModelId(false)
    setCustomModelId('')
  }

  const handleSubmit = async (values: any) => {
    try {
      // If custom model ID, override the form value
      const submitValues = {
        ...values,
        model_id: useCustomModelId ? customModelId : values.model_id,
      }
      if (!submitValues.model_id) {
        message.error(t(locale, 'modelId'))
        return
      }
      if (editItem) {
        await updateModel(editItem.id, submitValues)
        message.success(t(locale, 'updateSuccess'))
      } else {
        await createModel(submitValues)
        message.success(t(locale, 'createSuccess'))
      }
      setModalOpen(false)
      form.resetFields()
      setEditItem(null)
      loadModels()
    } catch (e: any) {
      message.error(e?.response?.data?.error?.message || (editItem ? t(locale, 'updateFailed') : t(locale, 'createFailed')))
    }
  }

  const handleDelete = async (id: string) => {
    try {
      await deleteModel(id)
      message.success(t(locale, 'deleteSuccess'))
      loadModels()
    } catch {}
  }

  const handleTestConnection = async (id: string) => {
    setTestingId(id)
    try {
      const result = await testModelConnection(id)
      setTestResults(prev => ({ ...prev, [id]: result }))
      if (result.success) {
        message.success(`${t(locale, 'testConnectionSuccess')} (${result.latency_ms}ms)`)
      } else {
        message.error(`${t(locale, 'testConnectionFailed')}: ${result.message}`)
      }
    } catch (e: any) {
      setTestResults(prev => ({ ...prev, [id]: { success: false, latency_ms: 0, message: e?.message || 'Unknown error' } }))
      message.error(t(locale, 'testConnectionFailed'))
    }
    setTestingId(null)
  }

  const presetModels = getModelsForPlatform(selectedPlatformName)

  const columns = [
    {
      title: t(locale, 'modelInfo'),
      key: 'model_info',
      width: 280,
      render: (_: any, record: any) => {
        const plat = platforms.find((p: any) => p.id === record.platform_id)
        const platDisplay = plat ? (platformPresets.find(p => p.name === plat.name) ? getPresetName(platformPresets.find(p => p.name === plat.name)!, locale) : plat.name) : ''
        return (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 2, padding: '4px 0' }}>
            <Text strong style={{ fontSize: 14 }}>{record.display_name}</Text>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <Tag style={{ fontFamily: 'monospace', fontSize: 11, maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{record.model_id}</Tag>
              {platDisplay && <Text type="secondary" style={{ fontSize: 11 }}>{platDisplay}</Text>}
            </div>
          </div>
        )
      },
    },
    { title: t(locale, 'maxTokens'), dataIndex: 'max_tokens', key: 'max_tokens', width: 100 },
    { title: t(locale, 'contextWindow'), dataIndex: 'context_window', key: 'context_window', width: 110 },
    {
      title: t(locale, 'capabilities'),
      dataIndex: 'capabilities',
      key: 'capabilities',
      width: 180,
      render: (caps: string[]) => {
        if (!caps || caps.length === 0) return <Text type="secondary">-</Text>
        return (
          <Space size={4} wrap>
            {caps.map((c: string) => (
              <Tag key={c} color={getCapabilityColor(c)} style={{ fontSize: 11, borderRadius: 4 }}>
                {getCapabilityLabel(c, locale)}
              </Tag>
            ))}
          </Space>
        )
      },
    },
    {
      title: t(locale, 'action'),
      key: 'action',
      width: 100,
      render: (_: any, record: any) => (
        <Space>
          <Tooltip title={testResults[record.id]?.success ? `${t(locale, 'connectionSuccess')} (${testResults[record.id].latency_ms}ms)` : testResults[record.id] ? `${t(locale, 'connectionFailed')}: ${testResults[record.id].message}` : t(locale, 'testConnection')}>
            <Button
              type="text"
              size="small"
              icon={
                testingId === record.id ? <LoadingOutlined /> :
                testResults[record.id]?.success ? <CheckCircleOutlined style={{ color: '#52c41a' }} /> :
                testResults[record.id] ? <CloseCircleOutlined style={{ color: '#ff4d4f' }} /> :
                <ApiOutlined />
              }
              onClick={() => handleTestConnection(record.id)}
              loading={testingId === record.id}
            />
          </Tooltip>
          <Button type="text" size="small" icon={<EditOutlined />} onClick={() => openEdit(record)} />
          <Popconfirm title={t(locale, 'deleteConfirm')} onConfirm={() => handleDelete(record.id)}>
            <Button type="text" danger size="small" icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <Title level={5} style={{ margin: 0 }}>{t(locale, 'models')}</Title>
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>{t(locale, 'addModel')}</Button>
      </div>

      <Card styles={{ body: { padding: 0 } }}>
        <Table columns={columns} dataSource={models} rowKey="id" loading={loading} pagination={{ pageSize: 20, showSizeChanger: false }} />
      </Card>

      <Modal
        title={editItem ? t(locale, 'editModel') : t(locale, 'addModel')}
        open={modalOpen}
        onCancel={() => { setModalOpen(false); setEditItem(null) }}
        onOk={() => form.submit()}
        width={500}
      >
        <Form form={form} layout="vertical" onFinish={handleSubmit}>
          <Form.Item name="platform_id" label={t(locale, 'belongPlatform')} rules={[{ required: true }]}>
            <Select
              placeholder={t(locale, 'selectPlatform')}
              options={platforms.map((p: any) => {
                const preset = platformPresets.find(pr => pr.name === p.name)
                return { value: p.id, label: preset ? getPresetName(preset, locale) : p.name }
              })}
              onChange={handlePlatformChange}
            />
          </Form.Item>

          <Form.Item label={t(locale, 'modelId')} required>
            {useCustomModelId ? (
              <Space.Compact style={{ width: '100%' }}>
                <Input
                  placeholder="e.g. gpt-4-1106-preview"
                  value={customModelId}
                  onChange={e => setCustomModelId(e.target.value)}
                  style={{ flex: 1 }}
                />
                <Button onClick={() => { setUseCustomModelId(false); setCustomModelId('') }}>
                  {presetModels.length > 0 ? t(locale, 'add') : ''}
                </Button>
              </Space.Compact>
            ) : (
              <Space.Compact style={{ width: '100%' }}>
                <Form.Item name="model_id" noStyle rules={[{ required: !useCustomModelId }]}>
                  <Select
                    showSearch
                    placeholder={t(locale, 'modelIdPlaceholder')}
                    options={presetModels.map(m => ({ value: m.model_id, label: `${getModelDisplayName(m, locale)} (${m.model_id})` }))}
                    filterOption={(input, option) => (option?.label as string)?.toLowerCase().includes(input.toLowerCase())}
                    onSelect={handleModelPresetSelect}
                    style={{ width: '100%' }}
                    allowClear
                  />
                </Form.Item>
                <Button onClick={() => setUseCustomModelId(true)}>
                  {t(locale, 'customInput')}
                </Button>
              </Space.Compact>
            )}

            {/* Fetch remote models from platform */}
            <div style={{ marginTop: 8, display: 'flex', gap: 8, alignItems: 'center' }}>
              <Button
                size="small"
                icon={<CloudDownloadOutlined />}
                onClick={handleFetchRemoteModels}
                loading={fetchingRemote}
                disabled={!form.getFieldValue('platform_id')}
              >
                {t(locale, 'fetchFromPlatform')}
              </Button>
              <Button
                size="small"
                onClick={() => setUseCustomModelId(true)}
              >
                {t(locale, 'customInput')}
              </Button>
            </div>

            {/* Remote models list */}
            {showRemoteModels && remoteModels.length > 0 && (
              <div style={{ marginTop: 8 }}>
                <div style={{ marginBottom: 4 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>
                    {t(locale, 'remoteModelsCount', String(remoteModels.length))}
                  </Text>
                </div>
                <Select
                  showSearch
                  placeholder={t(locale, 'selectRemoteModel')}
                  style={{ width: '100%' }}
                  options={remoteModels.map(m => ({ value: m.id, label: m.id }))}
                  filterOption={(input, option) => (option?.label as string)?.toLowerCase().includes(input.toLowerCase())}
                  onSelect={handleRemoteModelSelect}
                  allowClear
                />
              </div>
            )}
          </Form.Item>

          <Form.Item name="display_name" label={t(locale, 'displayName')} rules={[{ required: true }]}>
            <Input placeholder="GPT-4o" />
          </Form.Item>
          <Form.Item name="max_tokens" label={t(locale, 'maxTokens')} initialValue={4096}>
            <InputNumber min={1} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="context_window" label={t(locale, 'contextWindow')} initialValue={8192}>
            <InputNumber min={1} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="capabilities" label={t(locale, 'capabilities')} initialValue={[]}>
            <Select
              mode="multiple"
              placeholder={t(locale, 'selectCapabilities')}
              options={CAPABILITY_OPTIONS.map(c => ({
                value: c.value,
                label: locale === 'zh' ? c.labelZh : c.labelEn,
              }))}
              optionFilterProp="label"
            />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  )
}
