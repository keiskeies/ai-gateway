import { useEffect, useState } from 'react'
import {
  Button, Table, Modal, Form, Input, InputNumber, Select, Tag, Space,
  message, Card, Drawer, Descriptions, Divider, Row, Col, Popconfirm, Typography, Tabs,
} from 'antd'
import {
  PlusOutlined, DeleteOutlined, PlayCircleOutlined, PauseCircleOutlined,
  SettingOutlined, CopyOutlined, CodeOutlined, EditOutlined, KeyOutlined,
  EyeOutlined, EyeInvisibleOutlined,
} from '@ant-design/icons'
import {
  listProxies, createProxy, updateProxy, deleteProxy, startProxy, stopProxy,
  listRoutes, createRoute, deleteRoute,
  listBackends, addBackend, deleteBackend,
  listPlatforms, listModels, proxyBaseURL as getProxyBaseURL, getSettings,
} from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { getPresetName, platformPresets } from '../presets'

const { Text, Paragraph } = Typography

// Generate a random token with sk-ag- prefix
function generateAgToken(): string {
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789'
  let token = 'sk-ag-'
  for (let i = 0; i < 32; i++) {
    token += chars.charAt(Math.floor(Math.random() * chars.length))
  }
  return token
}

export default function Proxies() {
  const [proxies, setProxies] = useState<any[]>([])
  const [platforms, setPlatforms] = useState<any[]>([])
  const [models, setModels] = useState<any[]>([])
  const [createModalOpen, setCreateModalOpen] = useState(false)
  const [form] = Form.useForm()
  const [loading, setLoading] = useState(false)
  const { locale } = useAppContext()

  const [detailProxy, setDetailProxy] = useState<any>(null)
  const [detailRoutes, setDetailRoutes] = useState<any[]>([])
  const [addRouteModalOpen, setAddRouteModalOpen] = useState(false)
  const [routeForm] = Form.useForm()
  const [addBackendModalOpen, setAddBackendModalOpen] = useState(false)
  const [currentRouteId, setCurrentRouteId] = useState('')
  const [backendForm] = Form.useForm()
  const [selectedBackendPlatformId, setSelectedBackendPlatformId] = useState<string>('')

  // Edit proxy modal state
  const [editProxyModalOpen, setEditProxyModalOpen] = useState(false)
  const [editProxyForm] = Form.useForm()
  const [tokenVisible, setTokenVisible] = useState(false)

  // Usage code modal state
  const [usageModalOpen, setUsageModalOpen] = useState(false)
  const [usageProxy, setUsageProxy] = useState<any>(null)
  const [usageRoute, setUsageRoute] = useState<any>(null)
  const [adminPort, setAdminPort] = useState<number>(1994)

  const PROTOCOL_OPTIONS = [
    { value: 'OpenAI', label: t(locale, 'openaiCompatLabel') },
    { value: 'Anthropic', label: t(locale, 'anthropicCompatLabel') },
  ]

  const LB_OPTIONS = [
    { value: 'RoundRobin', label: t(locale, 'roundRobin') },
    { value: 'WeightedRandom', label: t(locale, 'weightedRandom') },
    { value: 'LeastConnections', label: t(locale, 'leastConnections') },
    { value: 'Priority', label: t(locale, 'priorityMode') },
    { value: 'LatencyBased', label: t(locale, 'latencyBased') },
  ]

  useEffect(() => { loadAll(); loadAdminPort() }, [])

  const loadAdminPort = async () => {
    try {
      const settings = await getSettings()
      setAdminPort(settings.admin_port)
    } catch {}
  }

  const loadAll = async () => {
    setLoading(true)
    try {
      const [p, pl, m] = await Promise.all([listProxies(), listPlatforms(), listModels()])
      setProxies(p); setPlatforms(pl); setModels(m)
    } catch {}
    setLoading(false)
  }

  const handleCreate = async (values: any) => {
    try {
      await createProxy(values)
      message.success(t(locale, 'createSuccess'))
      setCreateModalOpen(false)
      form.resetFields()
      loadAll()
    } catch { message.error(t(locale, 'createFailed')) }
  }

  const handleStart = async (id: string) => {
    try { await startProxy(id); message.success(t(locale, 'running')); loadAll() } catch (e: any) { message.error(e?.response?.data?.error?.message || 'Start failed') }
  }

  const handleStop = async (id: string) => {
    try { await stopProxy(id); message.success(t(locale, 'stopped')); loadAll() } catch {}
  }

  const handleDelete = async (id: string) => {
    try { await deleteProxy(id); message.success(t(locale, 'deleteSuccess')); loadAll() } catch {}
  }

  const openDetail = async (proxy: any) => {
    setDetailProxy(proxy)
    try { setDetailRoutes(await listRoutes(proxy.id)) } catch { setDetailRoutes([]) }
  }

  const handleAddRoute = async (values: any) => {
    try {
      const backends = values.backends || []
      await createRoute(detailProxy.id, {
        virtual_model: values.virtual_model,
        lb_strategy: values.lb_strategy || 'RoundRobin',
        backends: backends.filter((b: any) => b.platform_id && b.model_id),
      })
      message.success(t(locale, 'createSuccess'))
      setAddRouteModalOpen(false)
      routeForm.resetFields()
      openDetail(detailProxy)
    } catch { message.error(t(locale, 'createFailed')) }
  }

  const handleAddBackend = async (values: any) => {
    try {
      await addBackend(currentRouteId, values)
      message.success(t(locale, 'createSuccess'))
      setAddBackendModalOpen(false)
      backendForm.resetFields()
      openDetail(detailProxy)
    } catch { message.error(t(locale, 'createFailed')) }
  }

  const handleDeleteRoute = async (id: string) => {
    try { await deleteRoute(id); message.success(t(locale, 'deleteSuccess')); openDetail(detailProxy) } catch {}
  }

  const handleDeleteBackend = async (id: string) => {
    try { await deleteBackend(id); message.success(t(locale, 'deleteSuccess')); openDetail(detailProxy) } catch {}
  }

  const handleEditProxy = async (values: any) => {
    try {
      const wasRunning = detailProxy.status === 'Running'
      const tokenChanged = values.auth_token !== detailProxy.auth_token
      // If token changed and proxy is running, stop it first
      if (wasRunning && tokenChanged) {
        await stopProxy(detailProxy.id)
      }
      await updateProxy(detailProxy.id, values)
      message.success(t(locale, 'updateSuccess'))
      setEditProxyModalOpen(false)
      // Refresh detail
      const updated = { ...detailProxy, ...values }
      setDetailProxy(updated)
      loadAll()
      // If token changed and proxy was running, restart it
      if (wasRunning && tokenChanged) {
        try { await startProxy(detailProxy.id) } catch {}
        loadAll()
      }
    } catch { message.error(t(locale, 'updateFailed')) }
  }

  const openEditProxy = () => {
    editProxyForm.setFieldsValue({
      name: detailProxy.name,
      listen_port: detailProxy.listen_port,
      protocols: detailProxy.protocols || ['OpenAI'],
      auth_token: detailProxy.auth_token || '',
    })
    setEditProxyModalOpen(true)
  }

  const openUsageModal = (proxy: any) => {
    setUsageProxy(proxy)
    setUsageRoute(null)
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
  const getModelName = (id: string) => models.find((m: any) => m.id === id)?.display_name || id
  const getModelId = (id: string) => models.find((m: any) => m.id === id)?.model_id || id

  // Build usage code snippets for a proxy or a specific route
  const getUsageSnippets = (proxy: any, route?: any) => {
    const port = adminPort
    const baseUrl = `http://localhost:${port}`
    const token = proxy.auth_token
    const modelName = route?.virtual_model || 'your-model-name'
    const openaiUrl = `${baseUrl}/v1/chat/completions`
    const anthropicUrl = `${baseUrl}/v1/messages`

    const curlOpenai = `curl ${openaiUrl} \\
  -H "Content-Type: application/json" \\
${token ? `  -H "Authorization: Bearer ${token}" \\
  ` : '  '}--data '{
    "model": "${modelName}",
    "messages": [{"role": "user", "content": "Hello"}],
    "max_tokens": 100
  }'`

    const curlAnthropic = `curl ${anthropicUrl} \\
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
    base_url="${anthropicUrl}",${token ? `\n    api_key="${token}",` : ''}
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
  baseURL: '${anthropicUrl}',${token ? `\n  apiKey: '${token}',` : ''}
});

const message = await client.messages.create({
  model: '${modelName}',
  max_tokens: 100,
  messages: [{ role: 'user', content: 'Hello' }],
});
console.log(message.content[0].text);`

    const shellOpenai = `# OpenAI Compatible API
OPENAI_BASE_URL="${baseUrl}/v1"${token ? `\nexport OPENAI_API_KEY="${token}"` : ''}

# Using curl
curl $OPENAI_BASE_URL/chat/completions \\
  -H "Content-Type: application/json" \\
${token ? `  -H "Authorization: Bearer $OPENAI_API_KEY" \\
  ` : '  '}--data '{
    "model": "${modelName}",
    "messages": [{"role": "user", "content": "Hello"}],
    "max_tokens": 100
  }'`

    const shellAnthropic = `# Anthropic Compatible API
ANTHROPIC_BASE_URL="${anthropicUrl}"${token ? `\nexport ANTHROPIC_API_KEY="${token}"` : ''}

# Using curl
curl $ANTHROPIC_BASE_URL \\
  -H "Content-Type: application/json" \\
  -H "anthropic-version: 2023-06-01" \\
${token ? `  -H "x-api-key: $ANTHROPIC_API_KEY" \\
  ` : '  '}--data '{
    "model": "${modelName}",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hello"}]
  }'`

    const snippets: { key: string; label: string; children: { key: string; label: string; code: string }[] }[] = []

    if (proxy.protocols?.includes('OpenAI') || !proxy.protocols?.length) {
      snippets.push({
        key: 'curl',
        label: t(locale, 'curlExample'),
        children: [{ key: 'curl-openai', label: t(locale, 'openaiCompat'), code: curlOpenai }],
      })
      snippets.push({
        key: 'python',
        label: t(locale, 'pythonExample'),
        children: [{ key: 'python-openai', label: t(locale, 'openaiCompat'), code: pythonOpenai }],
      })
      snippets.push({
        key: 'node',
        label: t(locale, 'nodeExample'),
        children: [{ key: 'node-openai', label: t(locale, 'openaiCompat'), code: nodeOpenai }],
      })
      snippets.push({
        key: 'shell',
        label: t(locale, 'shellExample'),
        children: [{ key: 'shell-openai', label: t(locale, 'openaiCompat'), code: shellOpenai }],
      })
    }

    if (proxy.protocols?.includes('Anthropic')) {
      const curlTab = snippets.find(s => s.key === 'curl')
      if (curlTab) curlTab.children.push({ key: 'curl-anthropic', label: t(locale, 'anthropicCompat'), code: curlAnthropic })
      else snippets.push({ key: 'curl', label: t(locale, 'curlExample'), children: [{ key: 'curl-anthropic', label: t(locale, 'anthropicCompat'), code: curlAnthropic }] })

      const pythonTab = snippets.find(s => s.key === 'python')
      if (pythonTab) pythonTab.children.push({ key: 'python-anthropic', label: t(locale, 'anthropicCompat'), code: pythonAnthropic })
      else snippets.push({ key: 'python', label: t(locale, 'pythonExample'), children: [{ key: 'python-anthropic', label: t(locale, 'anthropicCompat'), code: pythonAnthropic }] })

      const nodeTab = snippets.find(s => s.key === 'node')
      if (nodeTab) nodeTab.children.push({ key: 'node-anthropic', label: t(locale, 'anthropicCompat'), code: nodeAnthropic })
      else snippets.push({ key: 'node', label: t(locale, 'nodeExample'), children: [{ key: 'node-anthropic', label: t(locale, 'anthropicCompat'), code: nodeAnthropic }] })

      const shellTab = snippets.find(s => s.key === 'shell')
      if (shellTab) shellTab.children.push({ key: 'shell-anthropic', label: t(locale, 'anthropicCompat'), code: shellAnthropic })
      else snippets.push({ key: 'shell', label: t(locale, 'shellExample'), children: [{ key: 'shell-anthropic', label: t(locale, 'anthropicCompat'), code: shellAnthropic }] })
    }

    return snippets
  }

  // Open usage modal for a specific route
  const openRouteUsageModal = (route: any) => {
    setUsageProxy(detailProxy)
    setUsageRoute(route)
    setUsageModalOpen(true)
  }

  const columns = [
    {
      title: t(locale, 'name'),
      dataIndex: 'name',
      key: 'name',
      render: (v: string) => <Text strong>{v}</Text>,
    },
    {
      title: t(locale, 'port'),
      dataIndex: 'listen_port',
      key: 'listen_port',
      render: (v: number) => <Tag style={{ fontFamily: 'monospace', borderRadius: 4 }}>{v}</Tag>,
    },
    {
      title: t(locale, 'protocols'),
      dataIndex: 'protocols',
      key: 'protocols',
      render: (v: string[]) => (v || []).map((p: string) => <Tag key={p} style={{ borderRadius: 4, fontSize: 11 }}>{p}</Tag>),
    },
    {
      title: t(locale, 'status'),
      dataIndex: 'status',
      key: 'status',
      render: (v: string) => (
        <Tag color={v === 'Running' ? 'success' : 'default'} style={{ borderRadius: 12, padding: '0 12px' }}>
          {v === 'Running' ? t(locale, 'running') : t(locale, 'stopped')}
        </Tag>
      ),
    },
    {
      title: t(locale, 'action'),
      key: 'action',
      width: 160,
      render: (_: any, record: any) => (
        <Space>
          {record.status === 'Running' ? (
            <Button type="text" size="small" icon={<PauseCircleOutlined />} onClick={() => handleStop(record.id)} />
          ) : (
            <Button type="text" size="small" icon={<PlayCircleOutlined />} style={{ color: '#52c41a' }} onClick={() => handleStart(record.id)} />
          )}
          <Button type="text" size="small" icon={<SettingOutlined />} onClick={() => openDetail(record)} />
          <Popconfirm title={t(locale, 'deleteConfirm')} onConfirm={() => handleDelete(record.id)}>
            <Button type="text" danger size="small" icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <div>
      <div style={{ marginBottom: 16, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <Text type="secondary">{t(locale, 'proxyDesc')}</Text>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateModalOpen(true)}>{t(locale, 'newProxy')}</Button>
      </div>

      <Card styles={{ body: { padding: 0 } }}>
        <Table columns={columns} dataSource={proxies} rowKey="id" loading={loading} pagination={{ pageSize: 20, showSizeChanger: false }} />
      </Card>

      {/* Create Proxy */}
      <Modal title={t(locale, 'newProxy')} open={createModalOpen} onCancel={() => setCreateModalOpen(false)} onOk={() => form.submit()}>
        <Form form={form} layout="vertical" onFinish={handleCreate}>
          <Form.Item name="name" label={t(locale, 'proxyName')} rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="listen_port" label={t(locale, 'listenPort')} rules={[{ required: true }]}>
            <InputNumber min={1024} max={65535} style={{ width: '100%' }} placeholder="10888" />
          </Form.Item>
          <Form.Item name="protocols" label={t(locale, 'protocols')} initialValue={['OpenAI']}>
            <Select mode="multiple" options={PROTOCOL_OPTIONS} />
          </Form.Item>
          <Form.Item name="auth_token" label={t(locale, 'authToken')}>
            <Space.Compact style={{ width: '100%' }}>
              <Input placeholder={t(locale, 'authTokenPlaceholder')} style={{ flex: 1 }} />
              <Button icon={<KeyOutlined />} onClick={() => form.setFieldsValue({ auth_token: generateAgToken() })}>
                {t(locale, 'generateToken')}
              </Button>
            </Space.Compact>
          </Form.Item>
        </Form>
      </Modal>

      {/* Usage Code Modal */}
      <Modal
        title={`${t(locale, 'usageCode')} - ${usageProxy?.name || ''}${usageRoute ? ` / ${usageRoute.virtual_model}` : ''}`}
        open={usageModalOpen}
        onCancel={() => setUsageModalOpen(false)}
        footer={null}
        width={720}
      >
        {usageProxy && (
          <Tabs
            items={getUsageSnippets(usageProxy, usageRoute || undefined).map(lang => ({
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
            }))}
          />
        )}
      </Modal>

      {/* Proxy Detail Drawer */}
      <Drawer title={`${t(locale, 'proxyConfig')} - ${detailProxy?.name || ''}`} open={!!detailProxy} onClose={() => setDetailProxy(null)} width={720}>
        {detailProxy && (
          <div>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
              <Text strong style={{ fontSize: 14 }}>{t(locale, 'editProxyInfo')}</Text>
              <Button size="small" icon={<EditOutlined />} onClick={openEditProxy}>{t(locale, 'edit')}</Button>
            </div>
            <Descriptions column={2} size="small" bordered>
              <Descriptions.Item label={t(locale, 'name')}>{detailProxy.name}</Descriptions.Item>
              <Descriptions.Item label={t(locale, 'port')}>{detailProxy.listen_port}</Descriptions.Item>
              <Descriptions.Item label={t(locale, 'protocols')}>{(detailProxy.protocols || []).map((p: string) => <Tag key={p} style={{ borderRadius: 4 }}>{p}</Tag>)}</Descriptions.Item>
              <Descriptions.Item label={t(locale, 'status')}>
                <Tag color={detailProxy.status === 'Running' ? 'success' : 'default'}>{detailProxy.status}</Tag>
              </Descriptions.Item>
              <Descriptions.Item label={t(locale, 'authToken')} span={2}>
                {detailProxy.auth_token ? (
                  <Space>
                    <Text code style={{ fontSize: 12 }}>{tokenVisible ? detailProxy.auth_token : '••••••••••••'}</Text>
                    <Button type="text" size="small" icon={tokenVisible ? <EyeInvisibleOutlined /> : <EyeOutlined />} onClick={() => setTokenVisible(!tokenVisible)} />
                    <Button type="text" size="small" icon={<CopyOutlined />} onClick={() => copyText(detailProxy.auth_token)} />
                  </Space>
                ) : (
                  <Text type="secondary">-</Text>
                )}
              </Descriptions.Item>
            </Descriptions>

            <Divider orientation="left" style={{ marginTop: 20 }}>
              {t(locale, 'virtualModels')}
              <Button type="link" size="small" icon={<PlusOutlined />} onClick={() => setAddRouteModalOpen(true)}>{t(locale, 'add')}</Button>
            </Divider>

            {detailRoutes.length === 0 ? (
              <Text type="secondary">{t(locale, 'noVirtualModels')}</Text>
            ) : (
              detailRoutes.map((route: any) => (
                <Card key={route.id} size="small" style={{ marginBottom: 12, borderRadius: 8 }} styles={{ body: { padding: '12px 16px' } }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
                    <div>
                      <Tag color="purple" style={{ fontSize: 13, padding: '2px 10px', borderRadius: 4 }}>{route.virtual_model}</Tag>
                      <Tag style={{ borderRadius: 4 }}>{route.lb_strategy}</Tag>
                    </div>
                    <Space>
                      <Button size="small" icon={<CodeOutlined />} style={{ color: '#1677ff' }} onClick={() => openRouteUsageModal(route)}>{t(locale, 'usageCode')}</Button>
                      <Button size="small" icon={<PlusOutlined />} onClick={() => { setCurrentRouteId(route.id); setAddBackendModalOpen(true) }}>{t(locale, 'addBackendModel')}</Button>
                      <Popconfirm title={t(locale, 'deleteRoute')} onConfirm={() => handleDeleteRoute(route.id)}>
                        <Button danger type="text" size="small" icon={<DeleteOutlined />} />
                      </Popconfirm>
                    </Space>
                  </div>
                  <Table
                    size="small" pagination={false}
                    dataSource={route.backends || []} rowKey="id"
                    columns={[
                      { title: t(locale, 'platforms').slice(0,2), render: (_: any, r: any) => getPlatformDisplayName(r.platform_id) },
                      { title: t(locale, 'models').slice(0,2), render: (_: any, r: any) => <Tag style={{ borderRadius: 4, fontFamily: 'monospace' }}>{getModelId(r.model_id)}</Tag> },
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
              ))
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
          <Form.Item name="listen_port" label={t(locale, 'listenPort')} rules={[{ required: true }]}>
            <InputNumber min={1024} max={65535} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="protocols" label={t(locale, 'protocols')} rules={[{ required: true }]}>
            <Select mode="multiple" options={PROTOCOL_OPTIONS} />
          </Form.Item>
          <Form.Item name="auth_token" label={t(locale, 'authToken')}>
            <Space.Compact style={{ width: '100%' }}>
              <Input placeholder={t(locale, 'authTokenPlaceholder')} style={{ flex: 1 }} />
              <Button icon={<KeyOutlined />} onClick={() => editProxyForm.setFieldsValue({ auth_token: generateAgToken() })}>
                {t(locale, 'generateToken')}
              </Button>
            </Space.Compact>
          </Form.Item>
        </Form>
      </Modal>

      {/* Add Route */}
      <Modal title={t(locale, 'addVirtualModel')} open={addRouteModalOpen} onCancel={() => setAddRouteModalOpen(false)} onOk={() => routeForm.submit()} width={600}>
        <Form form={routeForm} layout="vertical" onFinish={handleAddRoute}>
          <Form.Item name="virtual_model" label={t(locale, 'virtualModel')} rules={[{ required: true }]}
            extra={<Text type="secondary" style={{ fontSize: 12 }}>{t(locale, 'virtualModelPlaceholder')}</Text>}>
            <Input placeholder="gpt-4" />
          </Form.Item>
          <Form.Item name="lb_strategy" label={t(locale, 'lbStrategy')} initialValue="RoundRobin">
            <Select options={LB_OPTIONS} />
          </Form.Item>
          <Form.List name="backends" initialValue={[{}]} >
            {(fields, { add, remove }) => (
              <>
                {fields.map(({ key, name, ...rest }) => (
                  <Row key={key} gutter={8} align="middle" style={{ marginBottom: 8 }}>
                    <Col span={8}>
                      <Form.Item {...rest} name={[name, 'platform_id']} style={{ marginBottom: 0 }}>
                        <Select placeholder={t(locale, 'selectPlatform')} options={platforms.map((p: any) => {
                          const preset = platformPresets.find(pr => pr.name === p.name)
                          return { value: p.id, label: preset ? getPresetName(preset, locale) : p.name }
                        })} size="small" onChange={() => {
                          // Clear model selection when platform changes
                          routeForm.setFieldsValue({ backends: routeForm.getFieldValue('backends')?.map((b: any, i: number) => i === name ? { ...b, model_id: undefined } : b) })
                        }} />
                      </Form.Item>
                    </Col>
                    <Col span={7}>
                      <Form.Item {...rest} name={[name, 'platform_id']} noStyle>
                        <Input type="hidden" />
                      </Form.Item>
                      <Form.Item {...rest} name={[name, 'model_id']} style={{ marginBottom: 0 }}>
                        <Select placeholder={t(locale, 'selectModel')} size="small" showSearch optionFilterProp="label"
                          options={
                            (() => {
                              const backends = routeForm.getFieldValue('backends') || []
                              const pid = backends[name]?.platform_id
                              return models
                                .filter((m: any) => !pid || m.platform_id === pid)
                                .map((m: any) => ({ value: m.id, label: `${m.display_name} (${m.model_id})` }))
                            })()
                          }
                        />
                      </Form.Item>
                    </Col>
                    <Col span={4}>
                      <Form.Item {...rest} name={[name, 'weight']} initialValue={1} style={{ marginBottom: 0 }}>
                        <InputNumber min={1} placeholder={t(locale, 'weight')} size="small" style={{ width: '100%' }} />
                      </Form.Item>
                    </Col>
                    <Col span={3}>
                      <Form.Item {...rest} name={[name, 'priority']} initialValue={0} style={{ marginBottom: 0 }}>
                        <InputNumber min={0} placeholder={t(locale, 'priority')} size="small" style={{ width: '100%' }} />
                      </Form.Item>
                    </Col>
                    <Col span={2}>
                      {name > 0 && <Button danger type="text" size="small" onClick={() => remove(name)}>-</Button>}
                    </Col>
                  </Row>
                ))}
                <Button type="dashed" onClick={() => add()} block icon={<PlusOutlined />}>{t(locale, 'addBackendModel')}</Button>
              </>
            )}
          </Form.List>
        </Form>
      </Modal>

      {/* Add Backend */}
      <Modal title={t(locale, 'addBackendModel')} open={addBackendModalOpen} onCancel={() => { setAddBackendModalOpen(false); setSelectedBackendPlatformId('') }} onOk={() => backendForm.submit()}>
        <Form form={backendForm} layout="vertical" onFinish={handleAddBackend}>
          <Form.Item name="platform_id" label={t(locale, 'platforms')} rules={[{ required: true }]}>
            <Select
              options={platforms.map((p: any) => {
                const preset = platformPresets.find(pr => pr.name === p.name)
                return { value: p.id, label: preset ? getPresetName(preset, locale) : p.name }
              })}
              onChange={(value: string) => {
                setSelectedBackendPlatformId(value)
                backendForm.setFieldsValue({ model_id: undefined })
              }}
            />
          </Form.Item>
          <Form.Item name="model_id" label={t(locale, 'models')} rules={[{ required: true }]}>
            <Select
              options={models
                .filter((m: any) => !selectedBackendPlatformId || m.platform_id === selectedBackendPlatformId)
                .map((m: any) => ({ value: m.id, label: `${m.display_name} (${m.model_id})` }))}
              showSearch optionFilterProp="label"
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
