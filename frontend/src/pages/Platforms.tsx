import { useEffect, useState } from 'react'
import { Button, Table, Modal, Form, Input, Select, Tag, Space, message, Popconfirm, Card, Typography } from 'antd'
import { PlusOutlined, DeleteOutlined, EditOutlined, KeyOutlined } from '@ant-design/icons'
import { listPlatforms, createPlatform, updatePlatform, deletePlatform } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { platformPresets, getPresetName } from '../presets'

const { Text, Title } = Typography

export default function Platforms() {
  const [platforms, setPlatforms] = useState<any[]>([])
  const [modalOpen, setModalOpen] = useState(false)
  const [editItem, setEditItem] = useState<any>(null)
  const [form] = Form.useForm()
  const [loading, setLoading] = useState(false)
  const { locale } = useAppContext()

  const PLATFORM_TYPES = [
    { value: 'OpenAI', label: t(locale, 'openaiType') },
    { value: 'Anthropic', label: t(locale, 'anthropicType') },
    { value: 'Ollama', label: t(locale, 'ollamaType') },
    { value: 'Azure', label: t(locale, 'azureType') },
    { value: 'Custom', label: t(locale, 'customType') },
  ]

  useEffect(() => { loadPlatforms() }, [])

  const loadPlatforms = async () => {
    setLoading(true)
    try { setPlatforms(await listPlatforms()) } catch {}
    setLoading(false)
  }

  const openCreate = () => {
    setEditItem(null)
    form.resetFields()
    setModalOpen(true)
  }

  const openEdit = (record: any) => {
    setEditItem(record)
    form.setFieldsValue({
      name: record.name,
      type: record.type,
      base_url: record.base_url,
      api_key: record.api_key,
      organization: record.organization,
    })
    setModalOpen(true)
  }

  const handleSubmit = async (values: any) => {
    try {
      if (editItem) {
        await updatePlatform(editItem.id, values)
        message.success(t(locale, 'updateSuccess'))
      } else {
        await createPlatform(values)
        message.success(t(locale, 'createSuccess'))
      }
      setModalOpen(false)
      form.resetFields()
      setEditItem(null)
      loadPlatforms()
    } catch (e: any) {
      message.error(e?.response?.data?.error?.message || (editItem ? t(locale, 'updateFailed') : t(locale, 'createFailed')))
    }
  }

  const handleDelete = async (id: string) => {
    try {
      await deletePlatform(id)
      message.success(t(locale, 'deleteSuccess'))
      loadPlatforms()
    } catch {}
  }

  const applyPreset = (preset: typeof platformPresets[0]) => {
    form.setFieldsValue({
      name: preset.name,
      type: preset.platform_type,
      base_url: preset.base_url,
    })
  }

  const columns = [
    {
      title: t(locale, 'name'),
      dataIndex: 'name',
      key: 'name',
      width: 140,
      render: (v: string) => {
        const preset = platformPresets.find(p => p.name === v)
        return <Text strong>{preset ? getPresetName(preset, locale) : v}</Text>
      },
    },
    {
      title: t(locale, 'type'),
      dataIndex: 'type',
      key: 'type',
      width: 120,
      render: (v: string) => <Tag style={{ borderRadius: 4 }}>{v}</Tag>,
    },
    { title: t(locale, 'baseUrl'), dataIndex: 'base_url', key: 'base_url', ellipsis: true },
    {
      title: t(locale, 'status'),
      dataIndex: 'status',
      key: 'status',
      width: 90,
      render: (v: string) => <Tag color={v === 'Active' ? 'success' : 'default'} style={{ borderRadius: 4 }}>{v === 'Active' ? t(locale, 'active') : v}</Tag>,
    },
    {
      title: 'API Key',
      dataIndex: 'api_key',
      key: 'api_key',
      width: 120,
      render: (v: string) => v ? <Text code style={{ fontSize: 12 }}>{v.slice(0, 8)}...</Text> : <Text type="secondary">-</Text>,
    },
    {
      title: t(locale, 'action'),
      key: 'action',
      width: 90,
      render: (_: any, record: any) => (
        <Space>
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
        <Title level={5} style={{ margin: 0 }}>{t(locale, 'platforms')}</Title>
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>{t(locale, 'addPlatform')}</Button>
      </div>

      <Card styles={{ body: { padding: 0 } }}>
        <Table columns={columns} dataSource={platforms} rowKey="id" loading={loading} pagination={{ pageSize: 20, showSizeChanger: false }} />
      </Card>

      {/* Create/Edit Platform Modal */}
      <Modal
        title={editItem ? t(locale, 'editPlatform') : t(locale, 'addPlatform')}
        open={modalOpen}
        onCancel={() => { setModalOpen(false); setEditItem(null) }}
        onOk={() => form.submit()}
        width={600}
      >
        {!editItem && (
          <div style={{ marginBottom: 16 }}>
            <Text type="secondary" style={{ fontSize: 12 }}>{t(locale, 'quickPreset')}</Text>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 8 }}>
              {platformPresets.map((p) => (
                <Tag key={p.name} color="processing" style={{ cursor: 'pointer', borderRadius: 4 }} onClick={() => applyPreset(p)}>
                  {getPresetName(p, locale)}
                </Tag>
              ))}
            </div>
          </div>
        )}
        <Form form={form} layout="vertical" onFinish={handleSubmit}>
          <Form.Item name="name" label={t(locale, 'platformName')} rules={[{ required: true }]}>
            <Input placeholder="OpenAI, DeepSeek, etc." />
          </Form.Item>
          <Form.Item name="type" label={t(locale, 'platformType')} rules={[{ required: true }]} initialValue="OpenAI">
            <Select options={PLATFORM_TYPES} />
          </Form.Item>
          <Form.Item name="base_url" label={t(locale, 'baseUrl')} rules={[{ required: true }]}>
            <Input placeholder="https://api.openai.com/v1" />
          </Form.Item>
          <Form.Item name="api_key" label={t(locale, 'apiKey')}>
            <Input.Password placeholder="sk-..." prefix={<KeyOutlined />} />
          </Form.Item>
          <Form.Item name="organization" label={t(locale, 'organization')}>
            <Input placeholder="org-xxx (optional)" />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  )
}
