import { useEffect, useState } from 'react'
import { Card, Form, InputNumber, Input, Select, Button, message, Typography, Divider, Switch } from 'antd'
import { SaveOutlined, DesktopOutlined, ControlOutlined, ThunderboltOutlined, SyncOutlined } from '@ant-design/icons'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { getSettings, updateSettings } from '../api'

const { Title, Text } = Typography

export default function Settings() {
  const [form] = Form.useForm()
  const { locale, isDark } = useAppContext()
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)

  useEffect(() => { loadSettings() }, [])

  const loadSettings = async () => {
    setLoading(true)
    try {
      const data = await getSettings()
      form.setFieldsValue(data)
    } catch {
      message.error(t(locale, 'loadFailed'))
    } finally {
      setLoading(false)
    }
  }

  const onSave = async (values: any) => {
    setSaving(true)
    try {
      await updateSettings(values)
      message.success(t(locale, 'updateSuccess'))
    } catch {
      message.error(t(locale, 'updateFailed'))
    } finally {
      setSaving(false)
    }
  }

  const LOG_OPTIONS = [
    { value: 'error', label: 'Error' },
    { value: 'warn', label: 'Warn' },
    { value: 'info', label: 'Info' },
    { value: 'debug', label: 'Debug' },
    { value: 'trace', label: 'Trace' },
  ]

  const LB_OPTIONS = [
    { value: 'RoundRobin', label: t(locale, 'roundRobin') },
    { value: 'WeightedRandom', label: t(locale, 'weightedRandom') },
    { value: 'LeastConnections', label: t(locale, 'leastConnections') },
    { value: 'Priority', label: t(locale, 'priorityMode') },
    { value: 'LatencyBased', label: t(locale, 'latencyBased') },
  ]

  const sectionIcon = (color1: string, color2: string, icon: React.ReactNode) => (
    <div style={{
      width: 28, height: 28, borderRadius: 6,
      background: `linear-gradient(135deg, ${color1}, ${color2})`,
      display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0,
    }}>
      {icon}
    </div>
  )

  const cardStyle: React.CSSProperties = {
    borderRadius: 12,
    border: `1px solid ${isDark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.06)'}`,
    background: isDark ? 'rgba(255,255,255,0.02)' : '#fff',
    marginBottom: 16,
  }

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <Title level={5} style={{ margin: 0 }}>{t(locale, 'settings')}</Title>
      </div>

      <Form form={form} layout="vertical" onFinish={onSave}>
        {/* Server Section */}
        <Card style={cardStyle} styles={{ body: { padding: '16px 24px' } }} loading={loading}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
            {sectionIcon('#1677ff', '#4096ff', <DesktopOutlined style={{ color: '#fff', fontSize: 14 }} />)}
            <Title level={5} style={{ margin: 0 }}>{t(locale, 'serverSettings')}</Title>
          </div>
          <Form.Item name="admin_port" label={t(locale, 'adminPort')}
            extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'adminPortDesc')}</Text>}>
            <InputNumber min={1024} max={65535} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="host" label={t(locale, 'listenHost')}
            extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'listenHostDesc')}</Text>}>
            <Input placeholder="0.0.0.0" />
          </Form.Item>
          <Form.Item name="log_level" label={t(locale, 'logLevel')}
            extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'logLevelDesc')}</Text>}>
            <Select options={LOG_OPTIONS} />
          </Form.Item>
          <Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'settingsNote')}</Text>
        </Card>

        {/* Request Policy Section */}
        <Card style={cardStyle} styles={{ body: { padding: '16px 24px' } }} loading={loading}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
            {sectionIcon('#722ed1', '#9254de', <ControlOutlined style={{ color: '#fff', fontSize: 14 }} />)}
            <Title level={5} style={{ margin: 0 }}>{t(locale, 'defaultSettings')}</Title>
          </div>
          <Form.Item name="lb_strategy" label={t(locale, 'lbStrategyDefault')}
            extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'lbStrategyDefaultDesc')}</Text>}>
            <Select options={LB_OPTIONS} />
          </Form.Item>
          <Form.Item name="max_retries" label={t(locale, 'maxRetries')}
            extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'maxRetriesDesc')}</Text>}>
            <InputNumber min={0} max={10} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="retry_backoff_ms" label={t(locale, 'retryBackoffMs')}
            extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'retryBackoffMsDesc')}</Text>}>
            <InputNumber min={0} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="request_timeout_secs" label={t(locale, 'requestTimeoutSecs')}
            extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'requestTimeoutSecsDesc')}</Text>}>
            <InputNumber min={1} style={{ width: '100%' }} />
          </Form.Item>
        </Card>

        {/* Test Connection Section */}
        <Card style={cardStyle} styles={{ body: { padding: '16px 24px' } }} loading={loading}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
            {sectionIcon('#13c2c2', '#36cfc9', <ThunderboltOutlined style={{ color: '#fff', fontSize: 14 }} />)}
            <Title level={5} style={{ margin: 0 }}>{t(locale, 'testConnection')}</Title>
          </div>
          <Form.Item name="test_connection_timeout_secs" label={t(locale, 'testConnTimeoutSecs')}
            extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'testConnTimeoutSecsDesc')}</Text>}>
            <InputNumber min={1} max={120} style={{ width: '100%' }} />
          </Form.Item>
        </Card>

        <Divider style={{ margin: '8px 0 16px' }} />
        <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
          <Button type="primary" htmlType="submit" icon={<SaveOutlined />} loading={saving} style={{ borderRadius: 8, minWidth: 120 }}>
            {t(locale, 'save')}
          </Button>
        </div>
      </Form>
    </div>
  )
}
