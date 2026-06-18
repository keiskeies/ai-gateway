import { useEffect, useState } from 'react'
import {
  Button, Table, Modal, Form, Input, Select, Tag, Space,
  message, Row, Col, Popconfirm, Typography, InputNumber, Tabs, Card,
} from 'antd'
import {
  PlusOutlined, DeleteOutlined, LoadingOutlined,
  SettingOutlined, CodeOutlined, CopyOutlined,
} from '@ant-design/icons'
import {
  listProxies, createProxy, updateProxy, deleteProxy,
  listRoutes, createRoute, deleteRoute, deleteBackend,
  listPlatforms, fetchRemoteModels, getSettings, listApiKeys as fetchApiKeys,
} from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { getPresetName, platformPresets, getModelsForPlatform, CAPABILITY_OPTIONS } from '../presets'
import PageLayout from '../components/PageLayout'

const { Text } = Typography

const LB_OPTIONS = [
  { value: 'RoundRobin', label: t('zh', 'roundRobin') },
  { value: 'WeightedRandom', label: t('zh', 'weightedRandom') },
  { value: 'LeastConnections', label: t('zh', 'leastConnections') },
  { value: 'Priority', label: t('zh', 'priorityMode') },
  { value: 'LatencyBased', label: t('zh', 'latencyBased') },
]

export default function Proxies() {
  const [proxies, setProxies] = useState<any[]>([])
  const [proxyRoutes, setProxyRoutes] = useState<Record<string, any>>({})
  const [platforms, setPlatforms] = useState<any[]>([])
  const [createModalOpen, setCreateModalOpen] = useState(false)
  const [editingProxy, setEditingProxy] = useState<any>(null)
  const [form] = Form.useForm()
  const [loading, setLoading] = useState(false)
  const { locale } = useAppContext()

  const [usageModalOpen, setUsageModalOpen] = useState(false)
  const [usageProxy, setUsageProxy] = useState<any>(null)
  const [adminPort, setAdminPort] = useState<number>(1994)
  const [apiKeysList, setApiKeysList] = useState<any[]>([])

  const [createRemoteModels, setCreateRemoteModels] = useState<Record<number, { id: string; owned_by?: string }[]>>({})
  const [createFetching, setCreateFetching] = useState<Record<number, boolean>>({})

  useEffect(() => { loadAll(); loadAdminPort() }, [])

  const loadAdminPort = async () => {
    try { const settings = await getSettings(); setAdminPort(settings.admin_port) } catch {}
  }

  const loadAll = async () => {
    setLoading(true)
    try {
      const [p, pl, ak] = await Promise.all([listProxies(), listPlatforms(), fetchApiKeys().catch(() => [])])
      setProxies(p); setPlatforms(pl); setApiKeysList(ak)
      const routeMap: Record<string, any> = {}
      await Promise.all(p.map(async (proxy: any) => {
        try {
          const routes = await listRoutes(proxy.id)
          if (routes.length > 0) routeMap[proxy.id] = routes[0]
        } catch {}
      }))
      setProxyRoutes(routeMap)
    } catch {}
    setLoading(false)
  }

  const resetCreateForm = () => {
    form.resetFields()
    setCreateRemoteModels({})
    setCreateFetching({})
    setEditingProxy(null)
  }

  const handleCreate = async (values: any) => {
    try {
      const backends = (values.backends || []).filter((b: any) => b.platform_id && b.model_id)
      if (editingProxy) {
        await updateProxy(editingProxy.id, { name: values.name })
        const routes = await listRoutes(editingProxy.id).catch(() => [])
        for (const r of routes) {
          for (const b of (r.backends || [])) {
            await deleteBackend(b.id).catch(() => {})
          }
          await deleteRoute(r.id).catch(() => {})
        }
        if (backends.length > 0) {
          await createRoute(editingProxy.id, {
            lb_strategy: values.lb_strategy || 'RoundRobin',
            backends,
          })
        }
        message.success(t(locale, 'updateSuccess'))
      } else {
        const proxy = await createProxy({ name: values.name })
        if (backends.length > 0) {
          await createRoute(proxy.id, {
            lb_strategy: values.lb_strategy || 'RoundRobin',
            backends,
          })
        }
        message.success(t(locale, 'createSuccess'))
      }
      setCreateModalOpen(false)
      resetCreateForm()
      loadAll()
    } catch { message.error(editingProxy ? t(locale, 'updateFailed') : t(locale, 'createFailed')) }
  }

  const handleDelete = async (id: string) => {
    try { await deleteProxy(id); message.success(t(locale, 'deleteSuccess')); loadAll() } catch {}
  }

  const fetchRemote = async (platformId: string): Promise<{ id: string; owned_by?: string }[]> => {
    if (!platformId) return []
    try {
      const data = await fetchRemoteModels(platformId)
      return data.models || []
    } catch {
      message.error(t(locale, 'fetchRemoteModelsFailed'))
      return []
    }
  }

  const openDirectEdit = async (proxy: any) => {
    setEditingProxy(proxy)
    let route: any = null
    try {
      const routes = await listRoutes(proxy.id)
      route = routes.length > 0 ? routes[0] : null
    } catch {}
    form.setFieldsValue({
      name: proxy.name,
      lb_strategy: route?.lb_strategy || 'RoundRobin',
      backends: route?.backends?.map((b: any) => ({
        platform_id: b.platform_id,
        model_id: b.model_id,
        weight: b.weight ?? 1,
        priority: b.priority ?? 0,
        capabilities: b.capabilities || undefined,
      })) || [],
    })
    setCreateModalOpen(true)
  }

  const openUsageModal = (proxy: any) => {
    setUsageProxy(proxy)
    setUsageModalOpen(true)
  }

  const copyText = (text: string) => {
    navigator.clipboard.writeText(text).then(() => message.success('Copied!')).catch(() => {})
  }

  const getPlatformDisplayName = (id: string) => {
    const plat = platforms.find((p: any) => p.id === id)
    if (!plat) return id
    const preset = platformPresets.find(p => p.name === plat.name)
    return preset ? getPresetName(preset, locale) : plat.name
  }

  const getPlatformName = (id: string) => {
    const plat = platforms.find((p: any) => p.id === id)
    return plat?.name || ''
  }

  const getPresetCapabilities = (platformName: string, modelId: string): string[] => {
    const presetModels = getModelsForPlatform(platformName)
    const preset = presetModels.find(m => m.model_id === modelId)
    return preset?.capabilities || []
  }

  const columns = [
    {
      title: t(locale, 'name'),
      dataIndex: 'name',
      key: 'name',
      width: 160,
      render: (v: string) => <Tag color="purple" className="tag-mono" style={{ padding: '2px 10px' }}>{v}</Tag>,
    },
    {
      title: t(locale, 'backends'),
      key: 'backends',
      render: (_: any, record: any) => {
        const route = proxyRoutes[record.id]
        const backends = route?.backends || []
        if (backends.length === 0) return <Text type="secondary">-</Text>
        return (
          <Space size={4} wrap>
            {backends.map((b: any) => (
              <Tag key={b.id} className="tag-mono" style={{ fontSize: 12 }}>{getPlatformDisplayName(b.platform_id)}.{b.model_id}</Tag>
            ))}
          </Space>
        )
      },
    },
    {
      title: t(locale, 'action'),
      key: 'action',
      width: 120,
      render: (_: any, record: any) => (
        <Space>
          <Button type="text" size="small" icon={<CodeOutlined />} onClick={() => openUsageModal(record)} />
          <Button type="text" size="small" icon={<SettingOutlined />} onClick={() => openDirectEdit(record)} />
          <Popconfirm title={t(locale, 'deleteConfirm')} onConfirm={() => handleDelete(record.id)}>
            <Button type="text" danger size="small" icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  const getUsageSnippets = (proxy: any) => {
    const port = adminPort
    const baseUrl = `http://localhost:${port}`
    const relevantKey = apiKeysList.find((k: any) => !k.proxy_id) || apiKeysList.find((k: any) => k.proxy_id === proxy.id)
    const token = relevantKey?.key || ''
    const modelName = proxy.name

    const curlOpenai = `curl ${baseUrl}/v1/chat/completions \\
  -H "Content-Type: application/json" \\
${token ? `  -H "Authorization: Bearer ${token}" \\
  ` : '  '}--data '{
    "model": "${modelName}",
    "messages": [{"role": "user", "content": "Hello"}],
    "max_tokens": 100
  }'`

    const curlAnthropic = `curl ${baseUrl}/v1/messages \\
  -H "Content-Type: application/json" \\
  -H "anthropic-version: 2023-06-01" \\
${token ? `  -H "x-api-key: ${token}" \\
  ` : '  '}--data '{
    "model": "${modelName}",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hello"}]
  }'`

    const pythonOpenai = `import openai

client = openai.OpenAI(
    base_url="${baseUrl}/v1",${token ? `\n    api_key="${token}",` : ''}
)

response = client.chat.completions.create(
    model="${modelName}",
    messages=[{"role": "user", "content": "Hello"}],
    max_tokens=100
)
print(response.choices[0].message.content)`

    const pythonAnthropic = `import anthropic

client = anthropic.Anthropic(
    base_url="${baseUrl}/v1/messages",${token ? `\n    api_key="${token}",` : ''}
)

message = client.messages.create(
    model="${modelName}",
    max_tokens=100,
    messages=[{"role": "user", "content": "Hello"}]
)
print(message.content[0].text)`

    const nodeOpenai = `import OpenAI from 'openai';

const client = new OpenAI({
  baseURL: '${baseUrl}/v1',${token ? `\n  apiKey: '${token}',` : ''}
});

const response = await client.chat.completions.create({
  model: '${modelName}',
  messages: [{ role: 'user', content: 'Hello' }],
  max_tokens: 100,
});
console.log(response.choices[0].message.content);`

    const nodeAnthropic = `import Anthropic from '@anthropic-ai/sdk';

const client = new Anthropic({
  baseURL: '${baseUrl}/v1/messages',${token ? `\n  apiKey: '${token}',` : ''}
});

const message = await client.messages.create({
  model: '${modelName}',
  max_tokens: 100,
  messages: [{ role: 'user', content: 'Hello' }],
});
console.log(message.content[0].text);`

    return [
      {
        key: 'curl', label: 'cURL',
        children: [
          { key: 'openai', label: 'OpenAI', code: curlOpenai },
          { key: 'anthropic', label: 'Anthropic', code: curlAnthropic },
        ],
      },
      {
        key: 'python', label: 'Python',
        children: [
          { key: 'openai', label: 'OpenAI', code: pythonOpenai },
          { key: 'anthropic', label: 'Anthropic', code: pythonAnthropic },
        ],
      },
      {
        key: 'node', label: 'Node.js',
        children: [
          { key: 'openai', label: 'OpenAI', code: nodeOpenai },
          { key: 'anthropic', label: 'Anthropic', code: nodeAnthropic },
        ],
      },
    ]
  }

  return (
    <PageLayout
      title={t(locale, 'proxies')}
      extra={<Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateModalOpen(true)}>{t(locale, 'newProxy')}</Button>}
    >
      <Card bordered className="table-card">
        <Table columns={columns} dataSource={proxies} rowKey="id" loading={loading} pagination={{ pageSize: 20, showSizeChanger: false }} />
      </Card>

      <Modal title={editingProxy ? `${t(locale, 'edit')} - ${editingProxy.name}` : t(locale, 'newProxy')} open={createModalOpen} onCancel={() => { setCreateModalOpen(false); resetCreateForm() }} onOk={() => form.submit()} width={680}>
        <Form form={form} layout="vertical" onFinish={handleCreate}>
          <Form.Item name="name" label={t(locale, 'proxyName')} rules={[{ required: true }]}
            extra={<Text type="secondary" style={{ fontSize: 12 }}>{t(locale, 'virtualModelPlaceholder')}</Text>}>
            <Input placeholder="qc480" />
          </Form.Item>
          <Form.Item name="lb_strategy" label={t(locale, 'lbStrategy')} initialValue="RoundRobin">
            <Select options={LB_OPTIONS} />
          </Form.Item>
          <Form.List name="backends" initialValue={[{}]} >
            {(fields, { add, remove }) => (
              <>
                {fields.map(({ key, name, ...rest }) => (
                  <div key={key} className="backend-row">
                    <div className="backend-row-header">
                      <Text strong style={{ fontSize: 12 }}>{t(locale, 'backendModel')} #{name + 1}</Text>
                      {name > 0 && <Button danger type="text" size="small" onClick={() => remove(name)}>{t(locale, 'delete')}</Button>}
                    </div>
                    <Row gutter={8}>
                      <Col span={12}>
                        <Form.Item {...rest} name={[name, 'platform_id']} style={{ marginBottom: 8 }} rules={[{ required: true }]}>
                          <Select
                            placeholder={t(locale, 'selectPlatform')}
                            size="small"
                            options={platforms.map((p: any) => {
                              const preset = platformPresets.find(pr => pr.name === p.name)
                              return { value: p.id, label: preset ? getPresetName(preset, locale) : p.name }
                            })}
                            onChange={async (value: string) => {
                              form.setFieldsValue({ backends: form.getFieldValue('backends')?.map((b: any, i: number) => i === name ? { ...b, model_id: undefined, capabilities: undefined } : b) })
                              setCreateRemoteModels(prev => ({ ...prev, [name]: [] }))
                              setCreateFetching(prev => ({ ...prev, [name]: true }))
                              const models = await fetchRemote(value)
                              setCreateRemoteModels(prev => ({ ...prev, [name]: models }))
                              setCreateFetching(prev => ({ ...prev, [name]: false }))
                            }}
                          />
                        </Form.Item>
                      </Col>
                      <Col span={12}>
                        <Form.Item {...rest} name={[name, 'model_id']} style={{ marginBottom: 8 }} rules={[{ required: true }]}>
                          <Select
                            placeholder={createFetching[name] ? t(locale, 'loading') : t(locale, 'selectModel')}
                            size="small"
                            showSearch
                            optionFilterProp="label"
                            notFoundContent={createFetching[name] ? <LoadingOutlined /> : undefined}
                            options={
                              (() => {
                                const backends = form.getFieldValue('backends') || []
                                const pid = backends[name]?.platform_id
                                const pName = getPlatformName(pid)
                                const remote = createRemoteModels[name] || []
                                const presetMs = getModelsForPlatform(pName)
                                const remoteIds = new Set(remote.map(m => m.id))
                                const presetOnly = presetMs.filter(m => !remoteIds.has(m.model_id))
                                const remoteOpts = remote.map(m => {
                                  const pm = presetMs.find(p => p.model_id === m.id)
                                  const display = pm ? `${locale === 'zh' ? pm.display_name_zh : pm.display_name} (${m.id})` : m.id
                                  return { value: m.id, label: display }
                                })
                                const presetOpts = presetOnly.map(m => ({
                                  value: m.model_id,
                                  label: `${locale === 'zh' ? m.display_name_zh : m.display_name} (${m.model_id})`,
                                }))
                                return [...remoteOpts, ...presetOpts]
                              })()
                            }
                            onChange={(value: string) => {
                              const backends = form.getFieldValue('backends') || []
                              const pid = backends[name]?.platform_id
                              const pName = getPlatformName(pid)
                              const caps = getPresetCapabilities(pName, value)
                              if (caps.length > 0) {
                                form.setFieldsValue({ backends: form.getFieldValue('backends')?.map((b: any, i: number) => i === name ? { ...b, capabilities: caps } : b) })
                              }
                            }}
                          />
                        </Form.Item>
                      </Col>
                    </Row>
                    <Row gutter={8}>
                      <Col span={8}>
                        <Form.Item {...rest} name={[name, 'weight']} initialValue={1} style={{ marginBottom: 0 }}>
                          <InputNumber min={1} placeholder={t(locale, 'weight')} size="small" style={{ width: '100%' }} />
                        </Form.Item>
                      </Col>
                      <Col span={8}>
                        <Form.Item {...rest} name={[name, 'priority']} initialValue={0} style={{ marginBottom: 0 }}>
                          <InputNumber min={0} placeholder={t(locale, 'priority')} size="small" style={{ width: '100%' }} />
                        </Form.Item>
                      </Col>
                      <Col span={8}>
                        <Form.Item {...rest} name={[name, 'capabilities']} style={{ marginBottom: 0 }}>
                          <Select
                            mode="multiple"
                            size="small"
                            placeholder={t(locale, 'capabilities')}
                            style={{ width: '100%' }}
                            options={CAPABILITY_OPTIONS.map(c => ({ value: c.value, label: locale === 'zh' ? c.labelZh : c.labelEn }))}
                          />
                        </Form.Item>
                      </Col>
                    </Row>
                  </div>
                ))}
                <Button type="dashed" onClick={() => add()} block icon={<PlusOutlined />}>{t(locale, 'addBackendModel')}</Button>
              </>
            )}
          </Form.List>
        </Form>
      </Modal>

      <Modal
        title={`${t(locale, 'usageCode')} - ${usageProxy?.name || ''}`}
        open={usageModalOpen}
        onCancel={() => setUsageModalOpen(false)}
        footer={null}
        width={720}
      >
        {usageProxy && (
          <div>
            <Tabs items={getUsageSnippets(usageProxy).map(lang => ({
              key: lang.key,
              label: lang.label,
              children: (
                <Tabs
                  type="card"
                  size="small"
                  items={lang.children.map(proto => ({
                    key: proto.key,
                    label: proto.label,
                    children: (
                      <div style={{ position: 'relative' }}>
                        <pre className="code-block" style={{ maxHeight: 400 }}>
                          {proto.code}
                        </pre>
                        <Button
                          size="small"
                          icon={<CopyOutlined />}
                          style={{ position: 'absolute', top: 8, right: 8 }}
                          onClick={() => copyText(proto.code)}
                        />
                      </div>
                    ),
                  }))}
                />
              ),
            }))} />
          </div>
        )}
      </Modal>
    </PageLayout>
  )
}
