import { useEffect, useState } from 'react'
import {
  NumberInput,
  TextInput,
  Select,
  Button,
} from '@mantine/core'
import { useForm } from '@mantine/form'
import { notifications } from '@mantine/notifications'
import {
  IconDeviceDesktop,
  IconAdjustments,
  IconAlertTriangle,
  IconCheck,
  IconRefresh,
  IconSettings,
} from '@tabler/icons-react'
import { getSettings, updateSettings, cleanupStats } from '../api'
import { useAppContext } from '../ThemeContext'
import { t, type Locale } from '../i18n'

const LOG_OPTIONS = [
  { value: 'error', label: 'Error' },
  { value: 'warn', label: 'Warn' },
  { value: 'info', label: 'Info' },
  { value: 'debug', label: 'Debug' },
  { value: 'trace', label: 'Trace' },
]

function getLbOptions(locale: Locale) {
  return [
    { value: 'RoundRobin', label: t(locale, 'roundRobin') },
    { value: 'WeightedRandom', label: t(locale, 'weightedRandom') },
    { value: 'LeastConnections', label: t(locale, 'leastConnections') },
    { value: 'Priority', label: t(locale, 'priorityMode') },
    { value: 'LatencyBased', label: t(locale, 'latencyBased') },
  ]
}

export default function Settings() {
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const { locale } = useAppContext()

  const form = useForm({
    initialValues: {
      admin_port: 1994,
      listen_host: '0.0.0.0',
      log_level: 'info',
      max_retries: 3,
      retry_backoff_ms: 1000,
      request_timeout_secs: 120,
      test_connection_timeout_secs: 10,
      lb_strategy: 'RoundRobin',
    },
  })

  const [originalPort, setOriginalPort] = useState<number>(1994)
  const [originalHost, setOriginalHost] = useState<string>('0.0.0.0')
  const [cleaningStats, setCleaningStats] = useState(false)

  const onCleanupStats = async () => {
    setCleaningStats(true)
    try {
      const result = await cleanupStats()
      const deleted: number = result?.deleted ?? 0
      notifications.show({
        title: t(locale, 'statsCleanup'),
        message: deleted > 0 ? t(locale, 'statsCleanupSuccess', String(deleted)) : t(locale, 'statsCleanupNothing'),
        color: deleted > 0 ? 'green' : 'gray',
        icon: <IconCheck size={18} />,
      })
    } catch {
      notifications.show({
        message: t(locale, 'loadFailed'),
        color: 'red',
      })
    } finally {
      setCleaningStats(false)
    }
  }

  useEffect(() => { loadSettings() }, [])

  const loadSettings = async () => {
    setLoading(true)
    try {
      const data = await getSettings()
      form.setValues({
        admin_port: data.admin_port ?? 1994,
        listen_host: data.listen_host ?? data.host ?? '0.0.0.0',
        log_level: data.log_level ?? 'info',
        max_retries: data.max_retries ?? 3,
        retry_backoff_ms: data.retry_backoff_ms ?? 1000,
        request_timeout_secs: data.request_timeout_secs ?? 120,
        test_connection_timeout_secs: data.test_connection_timeout_secs ?? data.test_conn_timeout_secs ?? 10,
        lb_strategy: data.lb_strategy ?? data.default_lb_strategy ?? 'RoundRobin',
      })
      setOriginalPort(data.admin_port ?? 1994)
      setOriginalHost(data.listen_host ?? data.host ?? '0.0.0.0')
    } catch {
      notifications.show({
        message: t(locale, 'loadFailed'),
        color: 'red',
      })
    } finally {
      setLoading(false)
    }
  }

  const onSave = async (values: typeof form.values) => {
    setSaving(true)
    try {
      const portChanged = values.admin_port !== originalPort
      const hostChanged = values.listen_host !== originalHost

      await updateSettings({
        admin_port: values.admin_port,
        listen_host: values.listen_host,
        log_level: values.log_level,
        max_retries: values.max_retries,
        retry_backoff_ms: values.retry_backoff_ms,
        request_timeout_secs: values.request_timeout_secs,
        test_connection_timeout_secs: values.test_connection_timeout_secs,
        lb_strategy: values.lb_strategy,
      })

      if (portChanged || hostChanged) {
        notifications.show({
          title: t(locale, 'updateSuccess'),
          message: t(locale, 'portChangeHint'),
          color: 'yellow',
          icon: <IconAlertTriangle size={18} />,
        })
        setOriginalPort(values.admin_port)
        setOriginalHost(values.listen_host)
      } else {
        notifications.show({
          message: t(locale, 'updateSuccess'),
          color: 'green',
          icon: <IconCheck size={18} />,
        })
      }
    } catch {
      notifications.show({
        message: t(locale, 'updateFailed'),
        color: 'red',
      })
    } finally {
      setSaving(false)
    }
  }

  const lbOptions = getLbOptions(locale)

  return (
    <div className="page">
      {/* ===== 页头 ===== */}
      <div className="page-head">
        <div>
          <div className="page-title">
            <span className="page-title-chip"><IconSettings size={17} stroke={1.7} /></span>
            {t(locale, 'settings')}
          </div>
          <div className="page-sub ds-num">
            {form.values.listen_host}:{form.values.admin_port}
          </div>
        </div>
        <div className="page-actions">
          <Button
            variant="default"
            leftSection={<IconRefresh size={15} />}
            onClick={loadSettings}
            loading={loading}
          >
            {t(locale, 'refresh')}
          </Button>
          <Button
            type="submit"
            form="settings-form"
            loading={saving}
            leftSection={<IconCheck size={15} />}
          >
            {t(locale, 'save')}
          </Button>
        </div>
      </div>

      {/* ===== 设置分区 ===== */}
      <form id="settings-form" onSubmit={form.onSubmit(onSave)}>
        <div className="ds-section-grid">
          {/* 服务端设置 */}
          <section className="ds-panel">
            <header className="ds-section-head">
              <span className="ds-chip"><IconDeviceDesktop size={16} /></span>
              <div>
                <div className="ds-section-title">{t(locale, 'serverSettings')}</div>
                <div className="ds-hint">{t(locale, 'settingsNote')}</div>
              </div>
            </header>
            <div className="ds-section-body">
              <div className="ds-field-grid">
                <NumberInput
                  label={t(locale, 'adminPort')}
                  description={t(locale, 'adminPortDesc')}
                  min={1024}
                  max={65535}
                  {...form.getInputProps('admin_port')}
                />
                <TextInput
                  label={t(locale, 'listenHost')}
                  description={t(locale, 'listenHostDesc')}
                  placeholder="0.0.0.0"
                  {...form.getInputProps('listen_host')}
                />
              </div>
              <div style={{ marginTop: 14 }}>
                <Select
                  label={t(locale, 'logLevel')}
                  description={t(locale, 'logLevelDesc')}
                  data={LOG_OPTIONS}
                  {...form.getInputProps('log_level')}
                />
              </div>
              <div className="ds-note warn">
                <IconAlertTriangle size={14} />
                <span>{t(locale, 'settingsNote')}</span>
              </div>
            </div>
          </section>

          {/* 请求策略 */}
          <section className="ds-panel">
            <header className="ds-section-head">
              <span className="ds-chip"><IconAdjustments size={16} /></span>
              <div>
                <div className="ds-section-title">{t(locale, 'defaultSettings')}</div>
                <div className="ds-hint">{t(locale, 'lbStrategyDefaultDesc')}</div>
              </div>
            </header>
            <div className="ds-section-body">
              <div className="ds-field-grid">
                <NumberInput
                  label={t(locale, 'maxRetries')}
                  description={t(locale, 'maxRetriesDesc')}
                  min={0}
                  max={10}
                  {...form.getInputProps('max_retries')}
                />
                <NumberInput
                  label={t(locale, 'retryBackoffMs')}
                  description={t(locale, 'retryBackoffMsDesc')}
                  min={100}
                  {...form.getInputProps('retry_backoff_ms')}
                />
                <NumberInput
                  label={t(locale, 'requestTimeoutSecs')}
                  description={t(locale, 'requestTimeoutSecsDesc')}
                  min={1}
                  {...form.getInputProps('request_timeout_secs')}
                />
                <NumberInput
                  label={t(locale, 'testConnTimeoutSecs')}
                  description={t(locale, 'testConnTimeoutSecsDesc')}
                  min={1}
                  max={120}
                  {...form.getInputProps('test_connection_timeout_secs')}
                />
              </div>
              <div style={{ marginTop: 14 }}>
                <Select
                  label={t(locale, 'lbStrategyDefault')}
                  description={t(locale, 'lbStrategyDefaultDesc')}
                  data={lbOptions}
                  {...form.getInputProps('lb_strategy')}
                />
              </div>
            </div>
          </section>

          {/* 数据维护 */}
          <section className="ds-panel">
            <header className="ds-section-head">
              <span className="ds-chip"><IconAlertTriangle size={16} /></span>
              <div>
                <div className="ds-section-title">{t(locale, 'statsCleanup')}</div>
                <div className="ds-hint">{t(locale, 'statsCleanupDesc')}</div>
              </div>
            </header>
            <div className="ds-section-body">
              <Button
                variant="subtle"
                leftSection={<IconRefresh size={15} />}
                onClick={onCleanupStats}
                loading={cleaningStats}
              >
                {t(locale, 'statsCleanup')}
              </Button>
            </div>
          </section>
        </div>
      </form>
    </div>
  )
}
