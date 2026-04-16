import { useEffect, useState } from 'react'
import { Card, Form, InputNumber, Input, Button, message, Typography, Divider, Alert } from 'antd'
import { SettingOutlined, SaveOutlined } from '@ant-design/icons'
import { getSettings, updateSettings } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'

export default function Settings() {
  const [form] = Form.useForm()
  const { locale } = useAppContext()
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [currentPort, setCurrentPort] = useState(1994)

  useEffect(() => { loadSettings() }, [])

  const loadSettings = async () => {
    setLoading(true)
    try {
      const data = await getSettings()
      form.setFieldsValue(data)
      setCurrentPort(data.admin_port)
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
      if (values.admin_port !== currentPort) {
        message.warning(t(locale, 'portChangeHint'), 6)
      }
      setCurrentPort(values.admin_port)
    } catch {
      message.error(t(locale, 'updateFailed'))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div style={{ maxWidth: 640 }}>
      <Card title={<><SettingOutlined /> {t(locale, 'settings')}</>} loading={loading}>
        <Alert
          message={t(locale, 'settingsNote')}
          type="info"
          showIcon
          style={{ marginBottom: 24 }}
        />
        <Form form={form} layout="vertical" onFinish={onSave}>
          <Typography.Title level={5}>{t(locale, 'serverSettings')}</Typography.Title>
          <Form.Item name="admin_port" label={t(locale, 'adminPort')} rules={[{ required: true }]}>
            <InputNumber min={1} max={65535} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="host" label={t(locale, 'listenHost')} rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="log_level" label={t(locale, 'logLevel')}>
            <Input />
          </Form.Item>

          <Divider />
          <Typography.Title level={5}>{t(locale, 'defaultSettings')}</Typography.Title>
          <Form.Item name="lb_strategy" label={t(locale, 'lbStrategy')}>
            <Input />
          </Form.Item>
          <Form.Item name="max_retries" label={t(locale, 'maxRetries')}>
            <InputNumber min={0} max={10} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="retry_backoff_ms" label={t(locale, 'retryBackoffMs')}>
            <InputNumber min={0} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="request_timeout_secs" label={t(locale, 'requestTimeoutSecs')}>
            <InputNumber min={1} style={{ width: '100%' }} />
          </Form.Item>

          <Form.Item>
            <Button type="primary" htmlType="submit" icon={<SaveOutlined />} loading={saving}>
              {t(locale, 'save')}
            </Button>
          </Form.Item>
        </Form>
      </Card>
    </div>
  )
}
