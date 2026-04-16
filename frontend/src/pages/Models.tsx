import { useEffect, useState } from 'react'
import { Table, Button, Modal, Form, Input, InputNumber, Select, Tag, Space, message, Popconfirm } from 'antd'
import { PlusOutlined, DeleteOutlined, EditOutlined } from '@ant-design/icons'
import { listModels, createModel, updateModel, deleteModel, listPlatforms } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { getModelsForPlatform, getModelDisplayName, getPresetName, platformPresets } from '../presets'

export default function Models() {
  const [models, setModels] = useState<any[]>([])
  const [platforms, setPlatforms] = useState<any[]>([])
  const [modalOpen, setModalOpen] = useState(false)
  const [editItem, setEditItem] = useState<any>(null)
  const [form] = Form.useForm()
  const [loading, setLoading] = useState(false)
  const [selectedPlatformName, setSelectedPlatformName] = useState<string>('')
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
    setModalOpen(true)
  }

  const openEdit = (record: any) => {
    setEditItem(record)
    const plat = platforms.find((p: any) => p.id === record.platform_id)
    setSelectedPlatformName(plat?.name || '')
    form.setFieldsValue({
      platform_id: record.platform_id,
      model_id: record.model_id,
      display_name: record.display_name,
      max_tokens: record.max_tokens,
      context_window: record.context_window,
    })
    setModalOpen(true)
  }

  const handlePlatformChange = (platformId: string) => {
    const plat = platforms.find((p: any) => p.id === platformId)
    setSelectedPlatformName(plat?.name || '')
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
      })
    }
  }

  const handleSubmit = async (values: any) => {
    try {
      if (editItem) {
        await updateModel(editItem.id, values)
        message.success(t(locale, 'updateSuccess'))
      } else {
        await createModel(values)
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

  const presetModels = getModelsForPlatform(selectedPlatformName)

  const columns = [
    {
      title: t(locale, 'displayName'),
      dataIndex: 'display_name',
      key: 'display_name',
    },
    {
      title: t(locale, 'modelId'),
      dataIndex: 'model_id',
      key: 'model_id',
      render: (v: string) => <Tag>{v}</Tag>,
    },
    {
      title: t(locale, 'belongPlatform'),
      dataIndex: 'platform_id',
      key: 'platform_id',
      render: (v: string) => {
        const plat = platforms.find((p: any) => p.id === v)
        if (!plat) return v
        const preset = platformPresets.find(p => p.name === plat.name)
        return preset ? getPresetName(preset, locale) : plat.name
      },
    },
    { title: t(locale, 'maxTokens'), dataIndex: 'max_tokens', key: 'max_tokens' },
    { title: t(locale, 'contextWindow'), dataIndex: 'context_window', key: 'context_window' },
    {
      title: t(locale, 'capabilities'),
      dataIndex: 'capabilities',
      key: 'capabilities',
      render: (caps: string[]) => (caps || []).map((c: string) => <Tag key={c} color="blue" style={{ fontSize: 11 }}>{c}</Tag>),
    },
    {
      title: t(locale, 'action'),
      key: 'action',
      render: (_: any, record: any) => (
        <Space>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(record)}>{t(locale, 'edit')}</Button>
          <Popconfirm title={t(locale, 'deleteConfirm')} onConfirm={() => handleDelete(record.id)}>
            <Button danger size="small" icon={<DeleteOutlined />}>{t(locale, 'delete')}</Button>
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <div>
      <div style={{ marginBottom: 16, display: 'flex', justifyContent: 'space-between' }}>
        <span>{t(locale, 'modelDesc')}</span>
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>{t(locale, 'addModel')}</Button>
      </div>

      <Table columns={columns} dataSource={models} rowKey="id" loading={loading} />

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
          <Form.Item name="model_id" label={t(locale, 'modelId')} rules={[{ required: true }]}>
            <Select
              showSearch
              placeholder={t(locale, 'modelIdPlaceholder')}
              options={presetModels.map(m => ({ value: m.model_id, label: `${getModelDisplayName(m, locale)} (${m.model_id})` }))}
              filterOption={(input, option) => (option?.label as string)?.toLowerCase().includes(input.toLowerCase())}
              onSelect={handleModelPresetSelect}
              allowClear
            />
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
        </Form>
      </Modal>
    </div>
  )
}
