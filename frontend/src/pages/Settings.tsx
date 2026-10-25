import { useEffect, useMemo, useRef, useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, Modal, NumberInput, PasswordInput, Select, Switch, TextInput } from '@mantine/core'
import { useForm } from '@mantine/form'
import { notifications } from '@mantine/notifications'
import {
  IconAlertTriangle,
  IconCheck,
  IconDatabase,
  IconExternalLink,
  IconLock,
  IconRefresh,
  IconTrash,
} from '@tabler/icons-react'
import {
  changePassword,
  cleanupStats,
  deleteRequestBodies,
  getDbInfo,
  getSettings,
  purgeStats,
  updateSettings,
} from '../api'
import { useAppContext } from '../ThemeContext'
import { t, type Locale } from '../i18n'
import { formatNumber } from '../lib/format'
import { PageHeader } from '../components/ui/PageHeader'
import { ConfigTransfer } from '../components/ConfigTransfer'
import { DatabaseBackup } from '../components/DatabaseBackup'
import { ErrorState, PageSkeleton } from '../components/ui/States'
import { Field } from '../components/Field'

const LOG_OPTIONS = [
  { value: 'error', label: 'Error' },
  { value: 'warn', label: 'Warn' },
  { value: 'info', label: 'Info' },
  { value: 'debug', label: 'Debug' },
  { value: 'trace', label: 'Trace' },
]

// 锚点顺序就是页面顺序；labelKey 复用各面板自己的标题，导航与面板不会各说一套
const SECTIONS = [
  { id: 'server', labelKey: 'serverSettings' },
  { id: 'defaults', labelKey: 'defaultSettings' },
  { id: 'account', labelKey: 'accountSettings' },
  { id: 'data', labelKey: 'dataSection' },
  { id: 'transfer', labelKey: 'configExportTitle' },
  { id: 'backup', labelKey: 'backupTitle' },
  { id: 'about', labelKey: 'aboutSection' },
] as const

const REPO_URL = 'https://github.com/keiskeies/ai-gateway'

// 锚点落点 = 固定表头 48px + .console-anchor 的 scroll-margin-top 12px
const ANCHOR_LAND = 60
// 阅读线：落点再往下半格，滚过去后高亮才跟滚动位置一致
const READING_LINE = 72

function getLbOptions(locale: Locale) {
  return [
    { value: 'RoundRobin', label: t(locale, 'roundRobin') },
    { value: 'WeightedRandom', label: t(locale, 'weightedRandom') },
    { value: 'LeastConnections', label: t(locale, 'leastConnections') },
    { value: 'Priority', label: t(locale, 'priorityMode') },
    { value: 'LatencyBased', label: t(locale, 'latencyBased') },
  ]
}

// 保留窗口只有四档：够覆盖"留一个季度 / 留一年"，又不诱导用户填一个会误删全部历史的数字
function getPurgeOptions(locale: Locale) {
  const unit = t(locale, 'unitDays')
  return [30, 90, 180, 365].map(days => ({ value: String(days), label: `${days} ${unit}` }))
}

function getAutoPurgeOptions(locale: Locale) {
  return [
    { value: '0', label: t(locale, 'autoPurgeOff') },
    ...[30, 90, 180, 365].map(days => ({ value: String(days), label: `${days} ${t(locale, 'unitDays')}` })),
  ]
}

// 跨度与构建时间要带日期：明细跨度两端可能不同日，只给时分的话"至 06:34:41"没有出处
function formatStamp(iso: string, locale: Locale) {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  return date.toLocaleString(locale === 'zh' ? 'zh-CN' : 'en-US', { hour12: false })
}

function formatBytes(bytes?: number | null) {
  if (!bytes) return '—'
  const units = ['B', 'KB', 'MB', 'GB']
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  const value = bytes / 1024 ** index
  return `${value >= 10 || index === 0 ? Math.round(value) : value.toFixed(1)} ${units[index]}`
}

interface SettingsValues {
  admin_port: number
  listen_host: string
  log_level: string
  max_retries: number
  retry_backoff_ms: number
  request_timeout_secs: number
  test_connection_timeout_secs: number
  lb_strategy: string
}

const isPublicHost = (host: string) => host !== '127.0.0.1' && host !== 'localhost' && host !== '::1'

export default function Settings() {
  const { locale } = useAppContext()
  const queryClient = useQueryClient()
  const [baseline, setBaseline] = useState<string>('')
  const [saving, setSaving] = useState(false)
  const [cleanOpen, setCleanOpen] = useState(false)
  const [cleanPreview, setCleanPreview] = useState<{ deleted: number; orphan_attempts: number; orphan_bodies: number; orphan_headers: number; orphan_responses: number } | null>(null)
  const [cleaning, setCleaning] = useState(false)
  const [purgeDays, setPurgeDays] = useState('90')
  const [purgeOpen, setPurgeOpen] = useState(false)
  const [purgePreview, setPurgePreview] = useState<{ rows: number; attempts: number; bodies: number; headers: number; responses: number; cutoff: string } | null>(null)
  const [purging, setPurging] = useState(false)
  const [captureOn, setCaptureOn] = useState(false)
  const [captureResponseOn, setCaptureResponseOn] = useState(false)
  const [autoPurgeDays, setAutoPurgeDays] = useState('0')
  const [captureSaving, setCaptureSaving] = useState(false)
  const [wipeOpen, setWipeOpen] = useState(false)
  const [wiping, setWiping] = useState(false)
  const [activeSection, setActiveSection] = useState<string>('server')
  const pinnedRef = useRef<string | null>(null)
  const [passwordDraft, setPasswordDraft] = useState({ current: '', next: '', confirm: '' })
  const [changingPassword, setChangingPassword] = useState(false)

  const form = useForm<SettingsValues>({
    initialValues: {
      admin_port: 1994,
      listen_host: '127.0.0.1',
      log_level: 'info',
      max_retries: 3,
      retry_backoff_ms: 1000,
      request_timeout_secs: 120,
      test_connection_timeout_secs: 10,
      lb_strategy: 'RoundRobin',
    },
  })

  const settingsQuery = useQuery({
    queryKey: ['settings'],
    queryFn: () => getSettings(),
  })

  const dbQuery = useQuery({
    queryKey: ['db-info'],
    queryFn: () => getDbInfo(),
  })

  useEffect(() => {
    const data = settingsQuery.data
    if (!data) return
    const values: SettingsValues = {
      admin_port: data.admin_port ?? 1994,
      listen_host: data.listen_host ?? data.host ?? '127.0.0.1',
      log_level: data.log_level ?? 'info',
      max_retries: data.max_retries ?? 3,
      retry_backoff_ms: data.retry_backoff_ms ?? 1000,
      request_timeout_secs: data.request_timeout_secs ?? 120,
      test_connection_timeout_secs: data.test_connection_timeout_secs ?? data.test_conn_timeout_secs ?? 10,
      lb_strategy: data.lb_strategy ?? data.default_lb_strategy ?? 'RoundRobin',
    }
    form.setValues(values)
    form.resetDirty(values)
    setBaseline(JSON.stringify(values))
    // 正文采集不在整表保存里：它是隐私开关，改完立刻生效，不该等用户去点"保存"
    setCaptureOn(data.capture_request_bodies ?? false)
    setCaptureResponseOn(data.capture_response_bodies ?? false)
    setAutoPurgeDays(String(data.auto_purge_days ?? 0))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settingsQuery.data])

  const contentReady = !settingsQuery.isPending && !settingsQuery.isError

  // 高亮当前分组：读面板相对视口的位置，所以主区换哪个容器滚动都跟着走。
  // 末组比视口矮、滚不动到阅读线，所以它一进下沿就点亮；服务配置与请求策略同一行、位置相同，
  // 位置分不出用户点了哪个，因此点击落位期间以点击为准。
  useEffect(() => {
    const nodes = SECTIONS
      .map(section => document.getElementById(`set-${section.id}`))
      .filter((node): node is HTMLElement => node !== null)
    if (!nodes.length) return
    const evaluate = () => {
      const pinned = pinnedRef.current
      if (pinned) {
        const node = document.getElementById(`set-${pinned}`)
        if (node && Math.abs(node.getBoundingClientRect().top - ANCHOR_LAND) <= 8) return
        pinnedRef.current = null
      }
      let current = nodes[0]
      for (const node of nodes) if (node.getBoundingClientRect().top <= READING_LINE) current = node
      const last = nodes[nodes.length - 1]
      if (last.getBoundingClientRect().top <= window.innerHeight * 0.7) current = last
      setActiveSection(current.id.replace('set-', ''))
    }
    evaluate()
    // scroll 不冒泡，但捕获阶段能收到任意容器的滚动
    window.addEventListener('scroll', evaluate, true)
    window.addEventListener('resize', evaluate)
    return () => {
      window.removeEventListener('scroll', evaluate, true)
      window.removeEventListener('resize', evaluate)
    }
    // 骨架屏阶段没有面板可观察，内容渲染出来后必须重来一次
  }, [contentReady])

  const dirty = baseline !== '' && JSON.stringify(form.values) !== baseline
  const addressChanged = useMemo(() => {
    if (!baseline) return false
    const saved = JSON.parse(baseline) as SettingsValues
    return form.values.admin_port !== saved.admin_port || form.values.listen_host !== saved.listen_host
  }, [baseline, form.values.admin_port, form.values.listen_host])
  const exposed = isPublicHost(form.values.listen_host)

  const onSave = async (values: SettingsValues) => {
    setSaving(true)
    try {
      await updateSettings({ ...values })
      setBaseline(JSON.stringify(values))
      form.resetDirty(values)
      await queryClient.invalidateQueries({ queryKey: ['settings'] })
      if (addressChanged) {
        notifications.show({
          title: t(locale, 'updateSuccess'),
          message: t(locale, 'portChangeHint'),
          color: 'yellow',
          icon: <IconAlertTriangle size={18} />,
        })
      } else {
        notifications.show({ message: t(locale, 'updateSuccess'), color: 'green', icon: <IconCheck size={18} /> })
      }
    } catch {
      notifications.show({ message: t(locale, 'updateFailed'), color: 'red' })
    } finally {
      setSaving(false)
    }
  }

  const onChangePassword = async () => {
    if (passwordDraft.next.length < 8) {
      notifications.show({ message: t(locale, 'passwordTooShort'), color: 'red' })
      return
    }
    if (passwordDraft.next !== passwordDraft.confirm) {
      notifications.show({ message: t(locale, 'passwordMismatch'), color: 'red' })
      return
    }
    setChangingPassword(true)
    try {
      await changePassword(passwordDraft.current, passwordDraft.next)
      setPasswordDraft({ current: '', next: '', confirm: '' })
      notifications.show({ message: t(locale, 'passwordChanged'), color: 'green', icon: <IconCheck size={18} /> })
    } catch (err: any) {
      notifications.show({
        message: err?.response?.status === 401 ? t(locale, 'currentPasswordWrong') : t(locale, 'passwordChangeFailed'),
        color: 'red',
      })
    } finally {
      setChangingPassword(false)
    }
  }

  // 两个清理动作都先预演：确认弹窗上的行数来自服务端同一条 WHERE，不是猜的
  const openCleanup = async () => {
    try {
      const preview = await cleanupStats({ dry_run: true })
      setCleanPreview({
        deleted: preview?.deleted ?? 0,
        orphan_attempts: preview?.orphan_attempts ?? 0,
        orphan_bodies: preview?.orphan_bodies ?? 0,
        orphan_headers: preview?.orphan_headers ?? 0,
        orphan_responses: preview?.orphan_responses ?? 0,
      })
      setCleanOpen(true)
    } catch {
      notifications.show({ message: t(locale, 'loadFailed'), color: 'red' })
    }
  }

  const onCleanup = async () => {
    setCleaning(true)
    try {
      const result = await cleanupStats()
      const deleted: number = result?.deleted ?? 0
      notifications.show({
        title: t(locale, 'statsCleanup'),
        message: deleted > 0 ? t(locale, 'statsCleanupSuccess', String(deleted)) : t(locale, 'statsCleanupNothing'),
        color: deleted > 0 ? 'green' : 'gray',
        icon: <IconCheck size={18} />,
      })
      await queryClient.invalidateQueries({ queryKey: ['overview'] })
      await queryClient.invalidateQueries({ queryKey: ['traffic'] })
      await queryClient.invalidateQueries({ queryKey: ['db-info'] })
      setCleanOpen(false)
    } catch {
      notifications.show({ message: t(locale, 'loadFailed'), color: 'red' })
    } finally {
      setCleaning(false)
    }
  }

  const openPurge = async () => {
    try {
      const preview = await purgeStats({ older_than_days: Number(purgeDays), dry_run: true })
      setPurgePreview({
        rows: preview?.rows ?? 0,
        attempts: preview?.attempts ?? 0,
        bodies: preview?.bodies ?? 0,
        headers: preview?.headers ?? 0,
        responses: preview?.responses ?? 0,
        cutoff: preview?.cutoff ?? '',
      })
      setPurgeOpen(true)
    } catch {
      notifications.show({ message: t(locale, 'loadFailed'), color: 'red' })
    }
  }

  const onPurge = async () => {
    setPurging(true)
    try {
      const result = await purgeStats({ older_than_days: Number(purgeDays) })
      const rows: number = result?.rows ?? 0
      notifications.show({
        title: t(locale, 'purgeTitle'),
        message: rows > 0 ? t(locale, 'purgeDone', String(rows)) : t(locale, 'purgeNothing'),
        color: rows > 0 ? 'green' : 'gray',
        icon: <IconCheck size={18} />,
      })
      await queryClient.invalidateQueries({ queryKey: ['overview'] })
      await queryClient.invalidateQueries({ queryKey: ['traffic'] })
      await queryClient.invalidateQueries({ queryKey: ['db-info'] })
      setPurgeOpen(false)
    } catch {
      notifications.show({ message: t(locale, 'loadFailed'), color: 'red' })
    } finally {
      setPurging(false)
    }
  }

  // 正文采集只发这一个字段：UpdateConfigRequest 全是 Option，缺字段就是"不改"，
  // 所以隐私开关不必等整表保存，也不会顺手覆盖别人正在改的别的设置。
  const onToggleCapture = async (next: boolean) => {
    setCaptureSaving(true)
    try {
      await updateSettings({ capture_request_bodies: next })
      setCaptureOn(next)
      await queryClient.invalidateQueries({ queryKey: ['settings'] })
      notifications.show({
        message: t(locale, next ? 'captureBodiesOn' : 'captureBodiesOff'),
        color: next ? 'yellow' : 'green',
        icon: next ? <IconAlertTriangle size={18} /> : <IconCheck size={18} />,
      })
    } catch {
      notifications.show({ message: t(locale, 'loadFailed'), color: 'red' })
    } finally {
      setCaptureSaving(false)
    }
  }

  const onToggleResponseCapture = async (next: boolean) => {
    setCaptureSaving(true)
    try {
      await updateSettings({ capture_response_bodies: next })
      setCaptureResponseOn(next)
      await queryClient.invalidateQueries({ queryKey: ['settings'] })
      notifications.show({
        message: t(locale, next ? 'captureResponseOn' : 'captureResponseOff'),
        color: next ? 'yellow' : 'green',
        icon: next ? <IconAlertTriangle size={18} /> : <IconCheck size={18} />,
      })
    } catch {
      notifications.show({ message: t(locale, 'loadFailed'), color: 'red' })
    } finally {
      setCaptureSaving(false)
    }
  }

  const onAutoPurgeChange = async (value: string) => {
    const days = Number(value)
    try {
      await updateSettings({ auto_purge_days: days })
      setAutoPurgeDays(value)
      await queryClient.invalidateQueries({ queryKey: ['settings'] })
      notifications.show({ message: t(locale, 'autoPurgeSaved'), color: 'green' })
    } catch {
      notifications.show({ message: t(locale, 'loadFailed'), color: 'red' })
    }
  }

  const onWipeBodies = async () => {
    setWiping(true)
    try {
      const deleted: number = (await deleteRequestBodies())?.deleted ?? 0
      notifications.show({
        title: t(locale, 'captureBodiesWipe'),
        message: deleted > 0 ? t(locale, 'captureBodiesWipeDone', String(deleted)) : t(locale, 'captureBodiesNone'),
        color: deleted > 0 ? 'green' : 'gray',
        icon: <IconCheck size={18} />,
      })
      await queryClient.invalidateQueries({ queryKey: ['db-info'] })
      await queryClient.invalidateQueries({ queryKey: ['traffic'] })
      setWipeOpen(false)
    } catch {
      notifications.show({ message: t(locale, 'loadFailed'), color: 'red' })
    } finally {
      setWiping(false)
    }
  }

  if (settingsQuery.isPending) return <PageSkeleton rows={6} />
  if (settingsQuery.isError) return <ErrorState onRetry={() => void settingsQuery.refetch()} />

  const db = dbQuery.data
  const buildTime = typeof __BUILD_TIME__ === 'string' && __BUILD_TIME__ ? formatStamp(__BUILD_TIME__, locale) : '—'

  return (
    <div className="console-page">
      <PageHeader
        kicker={<>{t(locale, 'kickerConfigure')} <span className="console-kicker-sep" /> {t(locale, 'kickerSystem')}</>}
        title={t(locale, 'settings')}
        subtitle={`${form.values.listen_host}:${form.values.admin_port}`}
        actions={
          <>
            <Button size="xs" variant="default" leftSection={<IconRefresh size={14} />} loading={settingsQuery.isFetching} onClick={() => void settingsQuery.refetch()}>
              {t(locale, 'refresh')}
            </Button>
            <Button size="xs" type="submit" form="settings-form" loading={saving} disabled={!dirty} leftSection={<IconCheck size={14} />}>
              {t(locale, 'save')}
            </Button>
          </>
        }
      />

      <div className="console-settings-layout">
        <nav className="console-settings-nav" aria-label={t(locale, 'settingsNav')}>
          {SECTIONS.map(section => (
            <button
              key={section.id}
              type="button"
              data-active={activeSection === section.id ? 'yes' : undefined}
              onClick={() => {
                const node = document.getElementById(`set-${section.id}`)
                if (!node) return
                pinnedRef.current = section.id
                setActiveSection(section.id)
                node.scrollIntoView({ block: 'start' })
              }}
            >
              {t(locale, section.labelKey)}
            </button>
          ))}
        </nav>

        <div className="console-settings-main">
          <form id="settings-form" onSubmit={form.onSubmit(onSave)}>
            <div className="console-settings-grid">
              <section className="console-panel console-anchor" id="set-server">
                <div className="console-panel-head">
                  <div className="console-panel-title">{t(locale, 'serverSettings')}</div>
                  <span className="console-panel-sub">{dirty ? t(locale, 'unsaved') : t(locale, 'saved')}</span>
                </div>
                <div className="console-panel-body">
                  <div className="console-form-grid">
                    <Field label={t(locale, 'adminPort')}>
                      <NumberInput size="xs" min={1024} max={65535} {...form.getInputProps('admin_port')} />
                    </Field>
                    <Field label={t(locale, 'listenHost')}>
                      <TextInput size="xs" classNames={{ input: 'console-mono' }} placeholder="127.0.0.1" {...form.getInputProps('listen_host')} />
                    </Field>
                    <Field full label={t(locale, 'logLevel')}>
                      <Select size="xs" data={LOG_OPTIONS} allowDeselect={false} {...form.getInputProps('log_level')} />
                    </Field>
                  </div>
                  {exposed && (
                    <div className="console-note warn">
                      <IconAlertTriangle size={13} style={{ verticalAlign: -2 }} /> {t(locale, 'hostExposedWarning')}
                    </div>
                  )}
                  {addressChanged && dirty && (
                    <div className="console-note">{t(locale, 'portChangeHint')}</div>
                  )}
                </div>
              </section>

              <section className="console-panel console-anchor" id="set-defaults">
                <div className="console-panel-head">
                  <div className="console-panel-title">{t(locale, 'defaultSettings')}</div>
                  <span className="console-panel-sub">{t(locale, 'defaults')}</span>
                </div>
                <div className="console-panel-body">
                  <div className="console-form-grid">
                    <Field label={t(locale, 'maxRetries')}>
                      <NumberInput size="xs" min={0} max={10} {...form.getInputProps('max_retries')} />
                    </Field>
                    <Field label={t(locale, 'retryBackoffMs')}>
                      <NumberInput size="xs" min={100} {...form.getInputProps('retry_backoff_ms')} />
                    </Field>
                    <Field label={t(locale, 'requestTimeoutSecs')}>
                      <NumberInput size="xs" min={1} {...form.getInputProps('request_timeout_secs')} />
                    </Field>
                    <Field label={t(locale, 'testConnTimeoutSecs')}>
                      <NumberInput size="xs" min={1} max={120} {...form.getInputProps('test_connection_timeout_secs')} />
                    </Field>
                    <Field full label={t(locale, 'lbStrategyDefault')}>
                      <Select size="xs" data={getLbOptions(locale)} allowDeselect={false} {...form.getInputProps('lb_strategy')} />
                    </Field>
                  </div>
                  <div className="console-note">{t(locale, 'lbStrategyDefaultDesc')}</div>
                </div>
              </section>
            </div>

            <section className="console-panel console-anchor" id="set-account" style={{ marginTop: 14 }}>
              <div className="console-panel-head">
                <div className="console-panel-title">{t(locale, 'accountSettings')}</div>
                <span className="console-panel-sub">{t(locale, 'accountSecurityDesc')}</span>
              </div>
              <div className="console-panel-body">
                <div className="console-form-grid">
                  <Field label={t(locale, 'currentPassword')}>
                    <PasswordInput size="xs" autoComplete="current-password" value={passwordDraft.current} onChange={e => setPasswordDraft(prev => ({ ...prev, current: e.currentTarget.value }))} />
                  </Field>
                  <Field label={t(locale, 'newPassword')}>
                    <PasswordInput size="xs" autoComplete="new-password" value={passwordDraft.next} onChange={e => setPasswordDraft(prev => ({ ...prev, next: e.currentTarget.value }))} />
                  </Field>
                  <Field label={t(locale, 'confirmPassword')}>
                    <PasswordInput size="xs" autoComplete="new-password" value={passwordDraft.confirm} onChange={e => setPasswordDraft(prev => ({ ...prev, confirm: e.currentTarget.value }))} />
                  </Field>
                </div>
                <div className="console-note">{t(locale, 'passwordMinHint')}</div>
                <div className="console-route-actions">
                  <Button size="xs" variant="default" leftSection={<IconLock size={13} />} loading={changingPassword} disabled={!passwordDraft.current || !passwordDraft.next} onClick={() => void onChangePassword()}>
                    {t(locale, 'changePassword')}
                  </Button>
                </div>
              </div>
            </section>
          </form>

          <section className="console-panel console-anchor" id="set-data" style={{ marginTop: 14 }}>
            <div className="console-panel-head">
              <div className="console-panel-title">{t(locale, 'dataSection')}</div>
              <span className="console-panel-sub">
                {db ? `${formatBytes(db.size_bytes)} · ${t(locale, 'rowsCount', formatNumber(db.request_rows))}` : t(locale, 'dataLabel')}
              </span>
            </div>
            <div className="console-panel-body">
              <div className="console-db-grid">
                <div className="console-db-cell">
                  <span>{t(locale, 'dbSize')}</span>
                  <strong>{formatBytes(db?.size_bytes)}</strong>
                </div>
                <div className="console-db-cell">
                  <span>{t(locale, 'dbRequestRows')}</span>
                  <strong>{formatNumber(db?.request_rows)}</strong>
                </div>
                <div className="console-db-cell">
                  <span>{t(locale, 'dbAttemptRows')}</span>
                  <strong>{formatNumber(db?.attempt_rows)}</strong>
                </div>
                <div className="console-db-cell">
                  <span>{t(locale, 'dbAlertRows')}</span>
                  <strong>{formatNumber(db?.alert_rows)}</strong>
                </div>
                <div className="console-db-cell">
                  <span>{t(locale, 'dbDangling')}</span>
                  <strong>{formatNumber(db?.dangling_rows)}</strong>
                </div>
                <div className="console-db-cell">
                  <span>{t(locale, 'dbBodies')}</span>
                  <strong>{db?.body_rows ? `${formatNumber(db.body_rows)} · ${formatBytes(db.body_bytes)}` : '0'}</strong>
                </div>
                <div className="console-db-cell">
                  <span>{t(locale, 'dbHeaders')}</span>
                  <strong>{db?.header_rows ? `${formatNumber(db.header_rows)} · ${formatBytes(db.header_bytes)}` : '0'}</strong>
                </div>
                <div className="console-db-cell">
                  <span>{t(locale, 'dbResponses')}</span>
                  <strong>{db?.response_rows ? `${formatNumber(db.response_rows)} · ${formatBytes(db.response_bytes)}` : '0'}</strong>
                </div>
              </div>
              <div className="console-db-line">
                <span>{t(locale, 'dbSpan')}</span>
                <span className="console-mono">
                  {db?.oldest_request_at && db?.newest_request_at
                    ? t(locale, 'dbSpanValue', formatStamp(db.oldest_request_at, locale), formatStamp(db.newest_request_at, locale))
                    : t(locale, 'dbEmpty')}
                </span>
              </div>
              <div className="console-db-line">
                <span>{t(locale, 'dbPath')}</span>
                <code className="console-mono">{db?.path ?? '—'}</code>
              </div>

              <div className="console-capture">
                <Switch
                  size="xs"
                  checked={captureOn}
                  disabled={captureSaving}
                  label={t(locale, 'captureBodies')}
                  onChange={event => void onToggleCapture(event.currentTarget.checked)}
                />
                <div className="console-note">{t(locale, 'captureBodiesDesc')}</div>
                <Switch
                  size="xs"
                  checked={captureResponseOn}
                  disabled={captureSaving}
                  label={t(locale, 'captureResponseBodies')}
                  onChange={event => void onToggleResponseCapture(event.currentTarget.checked)}
                />
                <div className="console-note">{t(locale, 'captureResponseBodiesDesc')}</div>
                <div className="console-route-actions">
                  <Button
                    size="xs"
                    variant="default"
                    color="red"
                    leftSection={<IconTrash size={13} />}
                    disabled={!db?.body_rows && !db?.header_rows && !db?.response_rows}
                    onClick={() => setWipeOpen(true)}
                  >
                    {t(locale, 'captureBodiesWipe')}
                  </Button>
                </div>
              </div>

              <div className="console-capture">
                <Select
                  size="xs"
                  w={180}
                  data={getAutoPurgeOptions(locale)}
                  allowDeselect={false}
                  value={autoPurgeDays}
                  onChange={value => void onAutoPurgeChange(value ?? '0')}
                  label={t(locale, 'autoPurge')}
                />
                <div className="console-note">{t(locale, 'autoPurgeDesc')}</div>
              </div>

              <div className="console-note warn">{t(locale, 'statsCleanupDesc')}</div>
              <div className="console-note">{t(locale, 'purgeDesc')}</div>
              <div className="console-route-actions">
                <Select
                  size="xs"
                  w={140}
                  data={getPurgeOptions(locale)}
                  allowDeselect={false}
                  value={purgeDays}
                  onChange={value => setPurgeDays(value ?? '90')}
                  aria-label={t(locale, 'purgeWindow')}
                />
                <Button size="xs" variant="default" color="red" leftSection={<IconTrash size={13} />} onClick={() => void openPurge()}>
                  {t(locale, 'purgeTitle')}
                </Button>
                <Button size="xs" variant="default" leftSection={<IconDatabase size={13} />} onClick={() => void openCleanup()}>
                  {t(locale, 'statsCleanup')}
                </Button>
                <Button size="xs" variant="default" leftSection={<IconRefresh size={13} />} loading={dbQuery.isFetching} onClick={() => void dbQuery.refetch()}>
                  {t(locale, 'refresh')}
                </Button>
              </div>
            </div>
          </section>

          <div id="set-transfer" className="console-anchor">
            <ConfigTransfer />
          </div>

          <div id="set-backup" className="console-anchor">
            <DatabaseBackup />
          </div>

          <section className="console-panel console-anchor" id="set-about" style={{ marginTop: 14 }}>
            <div className="console-panel-head">
              <div className="console-panel-title">{t(locale, 'aboutSection')}</div>
              <span className="console-panel-sub">v{__APP_VERSION__}</span>
            </div>
            <div className="console-panel-body">
              <div className="console-db-line">
                <span>{t(locale, 'aboutVersion')}</span>
                <span className="console-mono">v{__APP_VERSION__}</span>
              </div>
              <div className="console-db-line">
                <span>{t(locale, 'aboutBuild')}</span>
                <span className="console-mono">{buildTime}</span>
              </div>
              <div className="console-note">{t(locale, 'aboutHint')}</div>
              <div className="console-route-actions">
                <Button
                  size="xs"
                  variant="default"
                  leftSection={<IconExternalLink size={13} />}
                  onClick={() => window.open(REPO_URL, '_blank', 'noopener,noreferrer')}
                >
                  {t(locale, 'aboutRepo')}
                </Button>
              </div>
            </div>
          </section>
        </div>
      </div>

      <Modal opened={cleanOpen} onClose={() => setCleanOpen(false)} title={t(locale, 'statsCleanup')} size="sm">
        <div className="console-note">
          {t(locale, 'cleanupPreviewBody', String(cleanPreview?.deleted ?? 0), String(cleanPreview?.orphan_attempts ?? 0), String(cleanPreview?.orphan_bodies ?? 0), String(cleanPreview?.orphan_headers ?? 0), String(cleanPreview?.orphan_responses ?? 0))}
        </div>
        <div className="console-note warn">{t(locale, 'statsCleanupDesc')}</div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setCleanOpen(false)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" loading={cleaning} disabled={!cleanPreview?.deleted && !cleanPreview?.orphan_attempts && !cleanPreview?.orphan_bodies} onClick={() => void onCleanup()}>
            {t(locale, 'confirmDelete')}
          </Button>
        </div>
      </Modal>

      <Modal opened={purgeOpen} onClose={() => setPurgeOpen(false)} title={t(locale, 'purgeTitle')} size="sm">
        <div className="console-note">
          {t(locale, 'purgePreviewBody', String(purgePreview?.rows ?? 0), String(purgePreview?.attempts ?? 0), String(purgePreview?.bodies ?? 0), String(purgePreview?.headers ?? 0), String(purgePreview?.responses ?? 0), purgePreview?.cutoff ?? '')}
        </div>
        <div className="console-note warn">{t(locale, 'purgeConfirmHint')}</div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setPurgeOpen(false)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" loading={purging} disabled={!purgePreview?.rows && !purgePreview?.attempts} onClick={() => void onPurge()}>
            {t(locale, 'confirmDelete')}
          </Button>
        </div>
      </Modal>

      <Modal opened={wipeOpen} onClose={() => setWipeOpen(false)} title={t(locale, 'captureBodiesWipeTitle')} size="sm">
        <div className="console-note">
          {t(locale, 'capturePayloadsWipeBody', String((db?.body_rows ?? 0) + (db?.header_rows ?? 0) + (db?.response_rows ?? 0)), formatBytes((db?.body_bytes ?? 0) + (db?.header_bytes ?? 0) + (db?.response_bytes ?? 0)))}
        </div>
        <div className="console-note warn">{t(locale, 'captureBodiesWipeHint')}</div>
        <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
          <Button size="xs" variant="default" onClick={() => setWipeOpen(false)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" loading={wiping} disabled={!db?.body_rows && !db?.header_rows && !db?.response_rows} onClick={() => void onWipeBodies()}>
            {t(locale, 'confirmDelete')}
          </Button>
        </div>
      </Modal>
    </div>
  )
}
