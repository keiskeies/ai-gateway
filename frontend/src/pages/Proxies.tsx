import { useEffect, useState } from 'react'
import {
  Button, Table, Modal, Form, Input, Select, Tag, Space,
  message, Card, Drawer, Descriptions, Divider, Row, Col, Popconfirm, Typography, InputNumber, Tabs,
} from 'antd'
import {
  PlusOutlined, DeleteOutlined, LoadingOutlined,
  SettingOutlined, CodeOutlined, CopyOutlined,
} from '@ant-design/icons'
import {
  listProxies, createProxy, updateProxy, deleteProxy,
  listRoutes, createRoute, deleteRoute,
  listBackends, addBackend, deleteBackend,
  listPlatforms, fetchRemoteModels, getSettings, listApiKeys as fetchApiKeys,
} from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { getPresetName, platformPresets, getCapabilityLabel, getCapabilityColor, CAPABILITY_OPTIONS, getModelsForPlatform } from '../presets'

const { Text, Title } = Typography

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
  const [form] = Form.useForm()
  const [loading, setLoading] = useState(false)
  const { locale } = useAppContext()

  // Detail drawer
  const [detailProxy, setDetailProxy] = useState<any>(null)
  const [detailRoute, setDetailRoute] = useState<any>(null)
  const [addBackendModalOpen, setAddBackendModalOpen] = useState(false)
  const [backendForm] = Form.useForm()
  const [selectedBackendPlatformId, setSelectedBackendPlatformId] = useState<string>('')
  const [remoteModels, setRemoteModels] = useState<{ id: string; owned_by?: string }[]>([])
  const [fetchingRemote, setFetchingRemote] = useState(false)

  // Edit proxy modal
  const [editProxyModalOpen, setEditProxyModalOpen] = useState(false)
  const [editProxyForm] = Form.useForm()

  // Usage code modal
  const [usageModalOpen, setUsageModalOpen] = useState(false)
  const [usageProxy, setUsageProxy] = useState<any>(null)
  const [adminPort, setAdminPort] = useState<number>(1994)
  const [apiKeysList, setApiKeysList] = useState<any[]>([])

  // Create modal: remote models per backend row
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

  const handleCreate = async (values: any) => {
    try {
      const backends = values.backends || []
      const proxy = await createProxy({ name: values.name })
      if (backends.length > 0) {
        await createRoute(proxy.id, {
          lb_strategy: values.lb_strategy || 'RoundRobin',
          backends: backends.filter((b: any) => b.platform_id && b.model_id),
        })
      }
      message.success(t(locale, 'createSuccess'))
      setCreateModalOpen(false)
      form.resetFields()
      setCreateRemoteModels({})
      setCreateFetching({})
      loadAll()
    } catch { message.error(t(locale, 'createFailed')) }
  }

  const handleDelete = async (id: string) => {
    try { await deleteProxy(id); message.success(t(locale, 'deleteSuccess')); loadAll() } catch {}
  }

  const openDetail = async (proxy: any) => {
    setDetailProxy(proxy)
    try {
      const routes = await listRoutes(proxy.id)
      setDetailRoute(routes.length > 0 ? routes[0] : null)
    } catch { setDetailRoute(null) }
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

  const handleAddBackend = async (values: any) => {
    try {
      let routeId = detailRoute?.id
      if (!routeId) {
        const route = await createRoute(detailProxy.id, { lb_strategy: 'RoundRobin', backends: [values] })
        routeId = route.id
        setDetailRoute(route)
      } else {
        await addBackend(routeId, values)
      }
      message.success(t(locale, 'createSuccess'))
      setAddBackendModalOpen(false)
      backendForm.resetFields()
      setSelectedBackendPlatformId('')
      setRemoteModels([])
      openDetail(detailProxy)
    } catch { message.error(t(locale, 'createFailed')) }
  }

  const handleDeleteBackend = async (id: string) => {
    try { await deleteBackend(id); message.success(t(locale, 'deleteSuccess')); openDetail(detailProxy) } catch {}
  }

  const handleEditProxy = async (values: any) => {
    try {
      await updateProxy(detailProxy.id, values)
      message.success(t(locale, 'updateSuccess'))
      setEditProxyModalOpen(false)
      loadAll()
      openDetail({ ...detailProxy, ...values })
    } catch { message.error(t(locale, 'updateFailed')) }
  }

  const openEditProxy = () => {
    editProxyForm.setFieldsValue({ name: detailProxy.name })
    setEditProxyModalOpen(true)
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

  // Get platform name by id (for preset matching)
  const getPlatformName = (id: string) => {
    const plat = platforms.find((p: any) => p.id === id)
    return plat?.name || ''
  }

  // Get capabilities for a model from presets
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
      render: (v: string) => <Tag color="purple" style={{ fontSize: 13, padding: '2px 10px', borderRadius: 4, fontFamily: 'monospace' }}>{v}</Tag>,
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
              <Tag key={b.id} style={{ borderRadius: 4, fontFamily: 'monospace', fontSize: 12 }}>{getPlatformDisplayName(b.platform_id)}·{b.model_id}</Tag>
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
          <Button type="text" size="small" icon={<SettingOutlined />} onClick={() => openDetail(record)} />
          <Popconfirm title={t(locale, 'deleteConfirm')} onConfirm={() => handleDelete(record.id)}>
            <Button type="text" danger size="small" icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  // Build usage code snippets
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
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <Title level={5} style={{ margin: 0 }}>{t(locale, 'proxies')}</Title>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateModalOpen(true)}>{t(locale, 'newProxy')}</Button>
      </div>

      <Card styles={{ body: { padding: 0 } }}>
        <Table columns={columns} dataSource={proxies} rowKey="id" loading={loading} pagination={{ pageSize: 20, showSizeChanger: false }} />
      </Card>

      {/* Create Proxy (Virtual Model) */}
      <Modal title={t(locale, 'newProxy')} open={createModalOpen} onCancel={() => { setCreateModalOpen(false); setCreateRemoteModels({}); setCreateFetching({}) }} onOk={() => form.submit()} width={680}>
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
                  <div key={key} style={{ border: '1px solid var(--ant-color-border-secondary)', borderRadius: 8, padding: 12, marginBottom: 8 }}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
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
                              // Auto-fetch remote models
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
                                // Combine remote models and preset models
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

      {/* Usage Code Modal */}
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
                        <pre style={{
                          background: 'var(--ant-color-bg-container)',
                          border: '1px solid var(--ant-color-border)',
                          borderRadius: 8,
                          padding: 16,
                          fontSize: 13,
                          lineHeight: 1.6,
                          overflow: 'auto',
                          maxHeight: 400,
                          whiteSpace: 'pre-wrap',
                          wordBreak: 'break-all',
                        }}>
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

      {/* Proxy Detail Drawer */}
      <Drawer title={`${t(locale, 'proxyConfig')} - ${detailProxy?.name || ''}`} open={!!detailProxy} onClose={() => { setDetailProxy(null); setDetailRoute(null) }} width={720}>
        {detailProxy && (
          <div>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
              <Text strong style={{ fontSize: 14 }}>{t(locale, 'editProxyInfo')}</Text>
              <Button size="small" onClick={openEditProxy}>{t(locale, 'edit')}</Button>
            </div>
            <Descriptions column={1} size="small" bordered>
              <Descriptions.Item label={t(locale, 'name')}>
                <Tag color="purple" style={{ fontSize: 13, padding: '2px 10px', borderRadius: 4, fontFamily: 'monospace' }}>{detailProxy.name}</Tag>
              </Descriptions.Item>
            </Descriptions>

            <Divider orientation="left" style={{ marginTop: 20 }}>
              {t(locale, 'backendModels')}
              <Button type="link" size="small" icon={<PlusOutlined />} onClick={() => setAddBackendModalOpen(true)}>{t(locale, 'add')}</Button>
            </Divider>

            {!detailRoute || !detailRoute.backends || detailRoute.backends.length === 0 ? (
              <Text type="secondary">{t(locale, 'noBackendModels')}</Text>
            ) : (
              <Card size="small" style={{ borderRadius: 8 }} styles={{ body: { padding: '12px 16px' } }}>
                <div style={{ marginBottom: 8 }}>
                  <Tag style={{ borderRadius: 4 }}>{detailRoute.lb_strategy}</Tag>
                </div>
                <Table
                  size="small" pagination={false}
                  dataSource={detailRoute.backends || []} rowKey="id"
                  columns={[
                    { title: t(locale, 'platforms').slice(0, 2), render: (_: any, r: any) => getPlatformDisplayName(r.platform_id) },
                    { title: t(locale, 'models').slice(0, 2), render: (_: any, r: any) => <Tag style={{ borderRadius: 4, fontFamily: 'monospace' }}>{getPlatformDisplayName(r.platform_id)}·{r.model_id}</Tag> },
                    { title: t(locale, 'weight'), dataIndex: 'weight', width: 60 },
                    { title: t(locale, 'priority'), dataIndex: 'priority', width: 60 },
                    {
                      title: '', width: 40,
                      render: (_: any, r: any) => (
                        <Popconfirm title={t(locale, 'deleteBackend')} onConfirm={() => handleDeleteBackend(r.id)}>
                          <Button danger type="text" size="small" icon={<DeleteOutlined />} />
                        </Popconfirm>
                      ),
                    },
                  ]}
                />
              </Card>
            )}
          </div>
        )}
      </Drawer>

      {/* Edit Proxy Modal */}
      <Modal title={t(locale, 'editProxyInfo')} open={editProxyModalOpen} onCancel={() => setEditProxyModalOpen(false)} onOk={() => editProxyForm.submit()}>
        <Form form={editProxyForm} layout="vertical" onFinish={handleEditProxy}>
          <Form.Item name="name" label={t(locale, 'proxyName')} rules={[{ required: true }]}>
            <Input />
          </Form.Item>
        </Form>
      </Modal>

      {/* Add Backend */}
      <Modal title={t(locale, 'addBackendModel')} open={addBackendModalOpen} onCancel={() => { setAddBackendModalOpen(false); setSelectedBackendPlatformId(''); setRemoteModels([]) }} onOk={() => backendForm.submit()}>
        <Form form={backendForm} layout="vertical" onFinish={handleAddBackend}>
          <Form.Item name="platform_id" label={t(locale, 'platforms')} rules={[{ required: true }]}>
            <Select
              options={platforms.map((p: any) => {
                const preset = platformPresets.find(pr => pr.name === p.name)
                return { value: p.id, label: preset ? getPresetName(preset, locale) : p.name }
              })}
              onChange={async (value: string) => {
                setSelectedBackendPlatformId(value)
                backendForm.setFieldsValue({ model_id: undefined, capabilities: undefined })
                setRemoteModels([])
                setFetchingRemote(true)
                const models = await fetchRemote(value)
                setRemoteModels(models)
                setFetchingRemote(false)
              }}
            />
          </Form.Item>
          <Form.Item name="model_id" label={t(locale, 'models')} rules={[{ required: true }]}>
            <Select
              showSearch
              optionFilterProp="label"
              notFoundContent={fetchingRemote ? <LoadingOutlined /> : undefined}
              placeholder={fetchingRemote ? t(locale, 'loading') : t(locale, 'selectModel')}
              options={
                (() => {
                  const pName = getPlatformName(selectedBackendPlatformId)
                  const presetMs = getModelsForPlatform(pName)
                  const remoteIds = new Set(remoteModels.map(m => m.id))
                  const presetOnly = presetMs.filter(m => !remoteIds.has(m.model_id))
                  const remoteOpts = remoteModels.map(m => {
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
                const pName = getPlatformName(selectedBackendPlatformId)
                const caps = getPresetCapabilities(pName, value)
                if (caps.length > 0) {
                  backendForm.setFieldsValue({ capabilities: caps })
                }
              }}
            />
          </Form.Item>
          <Form.Item name="capabilities" label={t(locale, 'capabilities')}>
            <Select
              mode="multiple"
              placeholder={t(locale, 'selectCapabilities')}
              options={CAPABILITY_OPTIONS.map(c => ({ value: c.value, label: locale === 'zh' ? c.labelZh : c.labelEn }))}
            />
          </Form.Item>
          <Form.Item name="weight" label={t(locale, 'weight')} initialValue={1}>
            <InputNumber min={1} />
          </Form.Item>
          <Form.Item name="priority" label={t(locale, 'priority')} initialValue={0}>
            <InputNumber min={0} />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  )
}
