import { useEffect, useState } from 'react'
import {
  Button, Table, Modal, Form, Input, InputNumber, Select, Tag, Space,
  message, Card, Drawer, Descriptions, Divider, Row, Col, Popconfirm, Typography
} from 'antd'
import {
  PlusOutlined, DeleteOutlined, PlayCircleOutlined, PauseCircleOutlined,
  SettingOutlined,
} from '@ant-design/icons'
import {
  listProxies, createProxy, deleteProxy, startProxy, stopProxy,
  listRoutes, createRoute, deleteRoute,
  listBackends, addBackend, deleteBackend,
  listPlatforms, listModels, proxyBaseURL as getProxyBaseURL,
} from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { getPresetName, platformPresets } from '../presets'

const { Text, Paragraph } = Typography

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

  useEffect(() => { loadAll() }, [])

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

  const getPlatformDisplayName = (id: string) => {
    const plat = platforms.find((p: any) => p.id === id)
    if (!plat) return id
    const preset = platformPresets.find(p => p.name === plat.name)
    return preset ? getPresetName(preset, locale) : plat.name
  }
  const getModelName = (id: string) => models.find((m: any) => m.id === id)?.display_name || id
  const getModelId = (id: string) => models.find((m: any) => m.id === id)?.model_id || id

  const columns = [
    { title: t(locale, 'name'), dataIndex: 'name', key: 'name', render: (v: string) => <strong>{v}</strong> },
    { title: t(locale, 'port'), dataIndex: 'listen_port', key: 'listen_port', render: (v: number) => <Tag>{v}</Tag> },
    {
      title: t(locale, 'protocols'), dataIndex: 'protocols', key: 'protocols',
      render: (v: string[]) => (v || []).map((p: string) => <Tag key={p} color="blue">{p}</Tag>),
    },
    {
      title: t(locale, 'status'), dataIndex: 'status', key: 'status',
      render: (v: string) => <Tag color={v === 'Running' ? 'success' : 'default'}>{v === 'Running' ? t(locale, 'running') : t(locale, 'stopped')}</Tag>,
    },
    {
      title: t(locale, 'action'), key: 'action', render: (_: any, record: any) => (
        <Space>
          {record.status === 'Running' ? (
            <Button size="small" icon={<PauseCircleOutlined />} onClick={() => handleStop(record.id)}>{t(locale, 'stopped').slice(0,2)}</Button>
          ) : (
            <Button type="primary" size="small" icon={<PlayCircleOutlined />} onClick={() => handleStart(record.id)}>{t(locale, 'running').slice(0,2)}</Button>
          )}
          <Button size="small" icon={<SettingOutlined />} onClick={() => openDetail(record)}>{t(locale, 'edit')}</Button>
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
        <span>{t(locale, 'proxyDesc')}</span>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateModalOpen(true)}>{t(locale, 'newProxy')}</Button>
      </div>

      <Table columns={columns} dataSource={proxies} rowKey="id" loading={loading} />

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
            <Input.Password placeholder={t(locale, 'authTokenPlaceholder')} />
          </Form.Item>
        </Form>
      </Modal>

      {/* Proxy Detail Drawer */}
      <Drawer title={`${t(locale, 'proxyConfig')} - ${detailProxy?.name || ''}`} open={!!detailProxy} onClose={() => setDetailProxy(null)} width={720}>
        {detailProxy && (
          <div>
            <Descriptions column={2} size="small" bordered>
              <Descriptions.Item label={t(locale, 'name')}>{detailProxy.name}</Descriptions.Item>
              <Descriptions.Item label={t(locale, 'port')}>{detailProxy.listen_port}</Descriptions.Item>
              <Descriptions.Item label={t(locale, 'protocols')}>{(detailProxy.protocols || []).map((p: string) => <Tag key={p} color="blue">{p}</Tag>)}</Descriptions.Item>
              <Descriptions.Item label={t(locale, 'status')}>
                <Tag color={detailProxy.status === 'Running' ? 'success' : 'default'}>{detailProxy.status}</Tag>
              </Descriptions.Item>
            </Descriptions>

            {detailProxy.status === 'Running' && (
              <Card size="small" style={{ marginTop: 12, background: '#f6ffed', borderColor: '#b7eb8f' }}>
                <Text strong>{t(locale, 'usage')}：</Text>
                <Paragraph style={{ marginBottom: 0, marginTop: 4 }}>
                  {t(locale, 'openaiCompat')}：<Text code>{getProxyBaseURL() || `http://localhost:${detailProxy.listen_port}`}/v1/chat/completions</Text>
                </Paragraph>
                {detailProxy.protocols?.includes('Anthropic') && (
                  <Paragraph style={{ marginBottom: 0 }}>
                    {t(locale, 'anthropicCompat')}：<Text code>{getProxyBaseURL() || `http://localhost:${detailProxy.listen_port}`}/v1/messages</Text>
                  </Paragraph>
                )}
              </Card>
            )}

            <Divider orientation="left" style={{ marginTop: 20 }}>
              {t(locale, 'routeRules')}
              <Button type="link" size="small" icon={<PlusOutlined />} onClick={() => setAddRouteModalOpen(true)}>{t(locale, 'add')}</Button>
            </Divider>

            {detailRoutes.length === 0 ? (
              <Text type="secondary">{t(locale, 'noRoutes')}</Text>
            ) : (
              detailRoutes.map((route: any) => (
                <Card key={route.id} size="small" style={{ marginBottom: 12 }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
                    <div>
                      <Tag color="purple" style={{ fontSize: 13, padding: '2px 8px' }}>{route.virtual_model}</Tag>
                      <Tag>{route.lb_strategy}</Tag>
                    </div>
                    <Space>
                      <Button size="small" icon={<PlusOutlined />} onClick={() => { setCurrentRouteId(route.id); setAddBackendModalOpen(true) }}>{t(locale, 'addBackend')}</Button>
                      <Popconfirm title={t(locale, 'deleteRoute')} onConfirm={() => handleDeleteRoute(route.id)}>
                        <Button danger size="small" icon={<DeleteOutlined />} />
                      </Popconfirm>
                    </Space>
                  </div>
                  <Table
                    size="small" pagination={false}
                    dataSource={route.backends || []} rowKey="id"
                    columns={[
                      { title: t(locale, 'platforms').slice(0,2), render: (_: any, r: any) => getPlatformDisplayName(r.platform_id) },
                      { title: t(locale, 'models').slice(0,2), render: (_: any, r: any) => <Tag>{getModelId(r.model_id)}</Tag> },
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

      {/* Add Route */}
      <Modal title={t(locale, 'addRoute')} open={addRouteModalOpen} onCancel={() => setAddRouteModalOpen(false)} onOk={() => routeForm.submit()} width={600}>
        <Form form={routeForm} layout="vertical" onFinish={handleAddRoute}>
          <Form.Item name="virtual_model" label={t(locale, 'virtualModel')} rules={[{ required: true }]}>
            <Input placeholder={t(locale, 'virtualModelPlaceholder')} />
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
                        })} size="small" />
                      </Form.Item>
                    </Col>
                    <Col span={7}>
                      <Form.Item {...rest} name={[name, 'model_id']} style={{ marginBottom: 0 }}>
                        <Select placeholder={t(locale, 'selectModel')} options={models.map((m: any) => ({ value: m.id, label: `${m.display_name} (${m.model_id})` }))} size="small" showSearch optionFilterProp="label" />
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
                <Button type="dashed" onClick={() => add()} block icon={<PlusOutlined />}>{t(locale, 'addBackend')}</Button>
              </>
            )}
          </Form.List>
        </Form>
      </Modal>

      {/* Add Backend */}
      <Modal title={t(locale, 'addBackend')} open={addBackendModalOpen} onCancel={() => setAddBackendModalOpen(false)} onOk={() => backendForm.submit()}>
        <Form form={backendForm} layout="vertical" onFinish={handleAddBackend}>
          <Form.Item name="platform_id" label={t(locale, 'platforms')} rules={[{ required: true }]}>
            <Select options={platforms.map((p: any) => {
              const preset = platformPresets.find(pr => pr.name === p.name)
              return { value: p.id, label: preset ? getPresetName(preset, locale) : p.name }
            })} />
          </Form.Item>
          <Form.Item name="model_id" label={t(locale, 'models')} rules={[{ required: true }]}>
            <Select options={models.map((m: any) => ({ value: m.id, label: `${m.display_name} (${m.model_id})` }))} showSearch optionFilterProp="label" />
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
