import { useEffect, useState } from 'react'
import { Card, Form, InputNumber, Input, Select, Button, message, Typography, Col, Row } from 'antd'
import { SaveOutlined, DesktopOutlined, ControlOutlined, ThunderboltOutlined } from '@ant-design/icons'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { getSettings, updateSettings } from '../api'
import PageLayout from '../components/PageLayout'

const { Title, Text } = Typography

export default function Settings() {
  const [form] = Form.useForm()
  const { locale } = useAppContext()
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

  const SectionIcon = ({ color, icon }: { color: string; icon: React.ReactNode }) => (
    <div className="form-section-icon" style={{ background: color }}>
      {icon}
    </div>
  )

  return (
    <PageLayout title={t(locale, 'settings')}>
      <Form form={form} layout="vertical" onFinish={onSave}>
        {/* Server Settings */}
        <Card className="section-card settings-card" bordered loading={loading}>
          <div className="form-section">
            <SectionIcon color="#2563eb" icon={<DesktopOutlined style={{ color: '#fff', fontSize: 14 }} />} />
            <Title level={5} style={{ margin: 0 }}>{t(locale, 'serverSettings')}</Title>
          </div>
          <Row gutter={[24, 0]}>
            <Col xs={24} sm={12} lg={8}>
              <Form.Item name="admin_port" label={t(locale, 'adminPort')}
                extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'adminPortDesc')}</Text>}>
                <InputNumber min={1024} max={65535} style={{ width: '100%' }} />
              </Form.Item>
            </Col>
            <Col xs={24} sm={12} lg={8}>
              <Form.Item name="host" label={t(locale, 'listenHost')}
                extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'listenHostDesc')}</Text>}>
                <Input placeholder="0.0.0.0" />
              </Form.Item>
            </Col>
            <Col xs={24} sm={12} lg={8}>
              <Form.Item name="log_level" label={t(locale, 'logLevel')}
                extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'logLevelDesc')}</Text>}>
                <Select options={LOG_OPTIONS} />
              </Form.Item>
            </Col>
          </Row>
          <Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'settingsNote')}</Text>
        </Card>

        {/* Request Policy */}
        <Card className="section-card settings-card" bordered loading={loading}>
          <div className="form-section">
            <SectionIcon color="#7c3aed" icon={<ControlOutlined style={{ color: '#fff', fontSize: 14 }} />} />
            <Title level={5} style={{ margin: 0 }}>{t(locale, 'defaultSettings')}</Title>
          </div>
          <Row gutter={[24, 0]}>
            <Col xs={24} sm={12} lg={8}>
              <Form.Item name="lb_strategy" label={t(locale, 'lbStrategyDefault')}
                extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'lbStrategyDefaultDesc')}</Text>}>
                <Select options={LB_OPTIONS} />
              </Form.Item>
            </Col>
            <Col xs={24} sm={12} lg={8}>
              <Form.Item name="max_retries" label={t(locale, 'maxRetries')}
                extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'maxRetriesDesc')}</Text>}>
                <InputNumber min={0} max={10} style={{ width: '100%' }} />
              </Form.Item>
            </Col>
            <Col xs={24} sm={12} lg={8}>
              <Form.Item name="retry_backoff_ms" label={t(locale, 'retryBackoffMs')}
                extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'retryBackoffMsDesc')}</Text>}>
                <InputNumber min={0} style={{ width: '100%' }} />
              </Form.Item>
            </Col>
          </Row>
          <Row gutter={[24, 0]}>
            <Col xs={24} sm={12} lg={8}>
              <Form.Item name="request_timeout_secs" label={t(locale, 'requestTimeoutSecs')}
                extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'requestTimeoutSecsDesc')}</Text>}>
                <InputNumber min={1} style={{ width: '100%' }} />
              </Form.Item>
            </Col>
          </Row>
        </Card>

        {/* Test Connection */}
        <Card className="section-card settings-card" bordered loading={loading}>
          <div className="form-section">
            <SectionIcon color="#0891b2" icon={<ThunderboltOutlined style={{ color: '#fff', fontSize: 14 }} />} />
            <Title level={5} style={{ margin: 0 }}>{t(locale, 'testConnection')}</Title>
          </div>
          <Row gutter={[24, 0]}>
            <Col xs={24} sm={12} lg={8}>
              <Form.Item name="test_connection_timeout_secs" label={t(locale, 'testConnTimeoutSecs')}
                extra={<Text type="secondary" style={{ fontSize: 11 }}>{t(locale, 'testConnTimeoutSecsDesc')}</Text>}>
                <InputNumber min={1} max={120} style={{ width: '100%' }} />
              </Form.Item>
            </Col>
          </Row>
        </Card>

        {/* Save Button */}
        <div className="settings-actions">
          <Button type="primary" htmlType="submit" icon={<SaveOutlined />} loading={saving} style={{ minWidth: 120 }}>
            {t(locale, 'save')}
          </Button>
        </div>
      </Form>
    </PageLayout>
  )
}
