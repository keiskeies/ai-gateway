import { useEffect, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Anchor, Badge, Button, Checkbox, Modal, PasswordInput, Select, TextInput, Tooltip } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconExternalLink, IconGift, IconPlugConnected } from '@tabler/icons-react'
import {
  createPlatform,
  getAuthStatus,
  getCheckin,
  listKeyLinks,
  startCheckinLogin,
  waitForCheckinAttempt,
} from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import type { CheckinCapability } from '../types/checkin'
import { PLATFORM_TYPE_LABELS, probePlatformConfig, type PlatformTestResult } from '../lib/platform'
import { getPresetName, platformPresets } from '../presets'
import { Field } from './Field'

interface ProviderDraft {
  name: string
  type: string
  base_url: string
  api_key: string
  organization: string
  credit_budget: string
  credit_window: string
  /** 请求构造方式：OpenAI / Anthropic / Trae / WorkBuddy */
  adapter: string
  /** 额度计量单位：none / credits / currency / requests */
  quota_unit: string
}

const emptyDraft: ProviderDraft = {
  name: '', type: 'OpenAI', base_url: '', api_key: '', organization: '',
  credit_budget: '', credit_window: 'day', adapter: 'OpenAI', quota_unit: 'none',
}

const typeOptions = Object.entries(PLATFORM_TYPE_LABELS).map(([value, label]) => ({ value, label }))

/** Key 申请入口的响应形态（后端注册表）。
 * preset / base_url_prefix 用于前端本地匹配，避免为每次输入打一次可能 404 的查询。 */
interface KeyLink {
  brand: string
  preset?: string | null
  base_url_prefix?: string | null
  apply_url: string
  console_url?: string | null
  docs_url?: string | null
  verified: string
  note?: string | null
}

/** 平台创建表只有这一份：/providers 的新建按钮和首次配置向导共用，避免两处口径分家 */
export function ProviderCreateModal({
  opened,
  onClose,
  onCreated,
}: {
  opened: boolean
  onClose: () => void
  onCreated?: (platform: any) => void
}) {
  const queryClient = useQueryClient()
  const { locale } = useAppContext()
  const [draft, setDraft] = useState<ProviderDraft>(emptyDraft)
  const [preset, setPreset] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [testing, setTesting] = useState(false)
  const [test, setTest] = useState<PlatformTestResult | null>(null)
  const [keyLink, setKeyLink] = useState<KeyLink | null>(null)
  const [riskAccepted, setRiskAccepted] = useState(false)
  // 登录换 Key：能力表与「本机桌面端」由后端给，服务器模式不显示这个入口
  const [capabilities, setCapabilities] = useState<CheckinCapability[]>([])
  const [checkinLogin, setCheckinLogin] = useState(false)
  const [loginBusy, setLoginBusy] = useState(false)
  const [loginAttemptId, setLoginAttemptId] = useState<string | null>(null)
  const [loginNote, setLoginNote] = useState<string | null>(null)

  const presetOptions = platformPresets.map(p => ({ value: p.name, label: getPresetName(p, locale) }))
  const creditInvalid = draft.credit_budget.trim() !== ''
    && (!Number.isFinite(Number(draft.credit_budget)) || Number(draft.credit_budget) <= 0)
  // 私有协议上游（Trae / WorkBuddy）没有官方 API，必须让用户明确知情
  const isProprietary = draft.adapter === 'Trae' || draft.adapter === 'WorkBuddy'
  // 只有 credits 才用积分台账，其余单位不接受预算
  const usesLedger = draft.quota_unit === 'credits'

  // Key 申请入口：拉一次全量注册表在本地匹配。
  // 曾经每改一次 base_url 就 GET /platforms/key-link，未命中时后端按设计回 404，
  // 浏览器因此每次都记一条 console error（E2E 的 errors 断言会因此失败）。
  // 注册表很小且基本不变，全量拉一次 + 本地前缀匹配既没有 404 噪音，也少打一轮请求。
  const [keyLinks, setKeyLinks] = useState<KeyLink[]>([])
  useEffect(() => {
    if (!opened) return
    let cancelled = false
    listKeyLinks()
      .then((all: KeyLink[]) => { if (!cancelled) setKeyLinks(all) })
      .catch(() => { if (!cancelled) setKeyLinks([]) })
    return () => { cancelled = true }
  }, [opened])

  useEffect(() => {
    if (!draft.base_url.trim() || isProprietary) {
      setKeyLink(null)
      return
    }
    const byPreset = preset
      ? keyLinks.find(link => link.preset === preset)
      : undefined
    const normalized = draft.base_url.trim().replace(/\/+$/, '')
    const byUrl = keyLinks.find(link =>
      link.base_url_prefix ? normalized.startsWith(link.base_url_prefix) : false,
    )
    setKeyLink(byPreset ?? byUrl ?? null)
  }, [preset, draft.base_url, isProprietary, keyLinks])

  // 打开弹窗时问一次：哪些平台支持「登录并获取 Key」、本机能不能开登录窗口
  useEffect(() => {
    if (!opened) return
    let cancelled = false
    Promise.all([getCheckin().catch(() => null), getAuthStatus().catch(() => null)]).then(
      ([checkin, auth]) => {
        if (cancelled) return
        setCapabilities(checkin?.platforms ?? [])
        setCheckinLogin(Boolean(auth?.checkin_login && auth?.local_console))
      },
    )
    return () => { cancelled = true }
  }, [opened])

  const loginCapability = capabilities.find(cap =>
    cap.login_supported
    && draft.base_url.trim().toLowerCase().startsWith(cap.base_url_prefix.toLowerCase()),
  )

  const applyPreset = (value: string | null) => {
    setPreset(value)
    setTest(null)
    setRiskAccepted(false)
    setLoginAttemptId(null)
    setLoginNote(null)
    if (!value) return
    const found = platformPresets.find(p => p.name === value)
    if (!found) return
    setDraft(prev => ({
      ...prev,
      name: found.name,
      type: found.platform_type,
      base_url: found.base_url,
      adapter: found.adapter ?? 'OpenAI',
      quota_unit: found.credit_unit ?? 'none',
    }))
  }

  const setField = <K extends keyof ProviderDraft>(key: K, value: ProviderDraft[K]) => {
    setDraft(prev => ({ ...prev, [key]: value }))
    // 连接相关的字段一改，上一次测试结果就不再代表当前配置
    if (key === 'type' || key === 'base_url' || key === 'api_key') setTest(null)
    // 换了平台类型或地址，之前那次登录的绑定就作废了
    if (key === 'type' || key === 'base_url') {
      setLoginAttemptId(null)
      setLoginNote(null)
    }
  }

  const reset = () => {
    setDraft(emptyDraft)
    setPreset(null)
    setTest(null)
    setKeyLink(null)
    setRiskAccepted(false)
    setLoginAttemptId(null)
    setLoginNote(null)
  }

  const runTest = async () => {
    setTesting(true)
    const result = await probePlatformConfig(locale, {
      type: draft.type,
      base_url: draft.base_url.trim(),
      api_key: draft.api_key.trim(),
    })
    setTest(result)
    setTesting(false)
  }

  // 登录窗口里完成登录 → 后端读账号名和 SDK 令牌 → 回填表单；保存时再绑定会话
  const loginAndFill = async () => {
    if (!loginCapability) return
    setLoginBusy(true)
    try {
      const { attempt_id } = await startCheckinLogin(loginCapability.key)
      setLoginNote(t(locale, 'checkinLoginWaiting'))
      const attempt = await waitForCheckinAttempt(attempt_id)
      if (!attempt) {
        setLoginNote(t(locale, 'checkinLoginTimeout'))
        return
      }
      if (attempt.status !== 'ok') {
        setLoginNote(attempt.message || t(locale, 'checkinLoginFailed'))
        return
      }
      setLoginAttemptId(attempt_id)
      if (attempt.api_key) {
        setField('api_key', attempt.api_key)
        setLoginNote(t(locale, 'checkinKeyFilled'))
      } else {
        setLoginNote(t(locale, 'checkinKeyMissing'))
      }
      if (attempt.identity) {
        setDraft(prev => ({
          ...prev,
          name: prev.name.trim() ? prev.name : `${loginCapability.brand} · ${attempt.identity}`,
        }))
      }
    } catch (error: any) {
      setLoginNote(error?.response?.data?.error?.message || t(locale, 'checkinLoginFailed'))
    } finally {
      setLoginBusy(false)
    }
  }

  // 私有协议上游没有 API Key 可测，且必须勾选风险确认才能保存
  const canCreate = Boolean(draft.name.trim() && draft.base_url.trim())
    && !creditInvalid
    && (!isProprietary || riskAccepted)

  const create = async () => {
    const name = draft.name.trim()
    const baseUrl = draft.base_url.trim()
    if (!name || !baseUrl) return
    if (isProprietary && !riskAccepted) return
    const budget = draft.credit_budget.trim()
    if (budget !== '' && (!Number.isFinite(Number(budget)) || Number(budget) <= 0)) return
    setSaving(true)
    try {
      const platform = await createPlatform({
        name,
        type: draft.type,
        base_url: baseUrl,
        api_key: draft.api_key.trim(),
        organization: draft.organization.trim() || undefined,
        // 只有 credits 才带预算；其他单位带了会被后端拒绝（那是刻意的，避免留下永不生效的值）
        credit_budget: usesLedger && budget !== '' ? Number(budget) : null,
        credit_window: usesLedger && budget !== '' ? draft.credit_window : null,
        credit_unit: draft.quota_unit === 'none' ? null : draft.quota_unit,
        adapter: draft.adapter,
        login_attempt: loginAttemptId ?? undefined,
      })
      notifications.show({ message: t(locale, 'createSuccess'), color: 'green' })
      reset()
      await queryClient.invalidateQueries({ queryKey: ['platforms'] })
      await queryClient.invalidateQueries({ queryKey: ['checkin'] })
      onCreated?.(platform)
      onClose()
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'createFailed'),
        message: error?.response?.data?.error?.message || t(locale, 'requestFailed'),
        color: 'red',
      })
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal
      opened={opened}
      onClose={() => { onClose(); setTest(null) }}
      title={t(locale, 'addPlatform')}
      size="lg"
      closeOnClickOutside={false}
      closeButtonProps={{ 'aria-label': t(locale, 'close') }}
    >
      <div className="console-form-grid">
        <Field full label={t(locale, 'quickPreset').toUpperCase()}>
          <Select
            size="xs"
            data={presetOptions}
            value={preset}
            onChange={applyPreset}
            searchable
            clearable
            placeholder={t(locale, 'quickPreset')}
          />
        </Field>
        <Field label={t(locale, 'platformName').toUpperCase()}>
          <TextInput
            size="xs"
            value={draft.name}
            onChange={event => setField('name', event.currentTarget.value)}
            placeholder="DeepSeek"
            data-autofocus
          />
        </Field>
        <Field label={t(locale, 'platformType').toUpperCase()}>
          <Select
            size="xs"
            data={typeOptions}
            value={draft.type}
            onChange={value => setField('type', value || 'OpenAI')}
            allowDeselect={false}
          />
        </Field>
        <Field label={t(locale, 'adapter').toUpperCase()}>
          <Select
            size="xs"
            data={[
              { value: 'OpenAI', label: t(locale, 'adapterOpenai') },
              { value: 'Anthropic', label: t(locale, 'adapterAnthropic') },
              { value: 'Trae', label: t(locale, 'adapterTrae') },
              { value: 'WorkBuddy', label: t(locale, 'adapterWorkbuddy') },
            ]}
            value={draft.adapter}
            onChange={value => {
              setField('adapter', value || 'OpenAI')
              setRiskAccepted(false)
            }}
            allowDeselect={false}
            description={t(locale, 'adapterHint')}
          />
        </Field>
        <Field
          full
          label={
            <span className="console-inline-row">
              {t(locale, 'baseUrl').toUpperCase()}
              {keyLink && (
                <Anchor
                  href={keyLink.apply_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="console-keylink"
                >
                  <IconExternalLink size={12} /> {t(locale, 'applyApiKey')}
                  {keyLink.verified !== 'ok' && (
                    <Badge size="xs" color="gray" variant="light" ml={4}>
                      {t(locale, 'keyLinkUnverified')}
                    </Badge>
                  )}
                </Anchor>
              )}
            </span>
          }
        >
          <TextInput
            size="xs"
            classNames={{ input: 'console-mono' }}
            value={draft.base_url}
            onChange={event => setField('base_url', event.currentTarget.value)}
            placeholder="https://api.openai.com/v1"
          />
        </Field>
        {isProprietary ? (
          <div className="console-field console-form-full">
            <div className="console-note warn" role="note">
              <strong>{t(locale, 'proprietaryWarning')}</strong>
            </div>
            {draft.adapter === 'WorkBuddy' && (
              <div className="console-note warn" role="note">
                {t(locale, 'proprietaryExperimentalHint')}
              </div>
            )}
            <Checkbox
              size="xs"
              mt={8}
              checked={riskAccepted}
              onChange={event => setRiskAccepted(event.currentTarget.checked)}
              label={t(locale, 'proprietaryAcknowledge')}
            />
          </div>
        ) : (
          <>
            {loginCapability && checkinLogin && (
              <div className="console-field console-form-full">
                <div className="checkin-login-row">
                  <Button
                    size="xs"
                    variant="default"
                    leftSection={<IconGift size={14} />}
                    loading={loginBusy}
                    onClick={() => void loginAndFill()}
                  >
                    {t(locale, 'checkinGetKey')}
                  </Button>
                  <span className={loginAttemptId ? 'checkin-login-note ok' : 'checkin-login-note'}>
                    {loginNote || t(locale, 'checkinGetKeyHint')}
                  </span>
                </div>
              </div>
            )}
            <Field label={t(locale, 'apiKey').toUpperCase()}>
              <PasswordInput
                size="xs"
                value={draft.api_key}
                onChange={event => setField('api_key', event.currentTarget.value)}
                placeholder="sk-..."
                description={t(locale, 'localKeyHint')}
              />
            </Field>
            <Field label={t(locale, 'organization').toUpperCase()}>
              <TextInput
                size="xs"
                value={draft.organization}
                onChange={event => setField('organization', event.currentTarget.value)}
                placeholder="org-xxx"
              />
            </Field>
          </>
        )}
        <Field label={t(locale, 'quotaUnit').toUpperCase()}>
          <Select
            size="xs"
            data={[
              { value: 'none', label: t(locale, 'quotaUnitNone') },
              { value: 'credits', label: t(locale, 'quotaUnitCredits') },
              { value: 'currency', label: t(locale, 'quotaUnitCurrency') },
              { value: 'requests', label: t(locale, 'quotaUnitRequests') },
            ]}
            value={draft.quota_unit}
            onChange={value => setField('quota_unit', value || 'none')}
            allowDeselect={false}
          />
        </Field>
        <Field label={t(locale, 'creditBudget').toUpperCase()} unit={t(locale, 'creditUnit')}>
          <TextInput
            size="xs"
            inputMode="decimal"
            value={draft.credit_budget}
            onChange={event => setField('credit_budget', event.currentTarget.value)}
            placeholder="250"
            disabled={!usesLedger}
          />
        </Field>
        <Field label={t(locale, 'creditWindow').toUpperCase()}>
          <Select
            size="xs"
            data={[
              { value: 'day', label: t(locale, 'creditWindowDay') },
              { value: 'week', label: t(locale, 'creditWindowWeek') },
              { value: 'month', label: t(locale, 'creditWindowMonth') },
            ]}
            value={draft.credit_window}
            onChange={value => setField('credit_window', value || 'day')}
            disabled={!usesLedger || !draft.credit_budget.trim()}
            allowDeselect={false}
          />
        </Field>
        <div className="console-field console-form-full">
          <div className="console-note">
            {draft.quota_unit === 'credits' && t(locale, 'quotaUnitHintCredits')}
            {draft.quota_unit === 'currency' && t(locale, 'quotaUnitHintCurrency')}
            {draft.quota_unit === 'requests' && t(locale, 'quotaUnitHintRequests')}
            {draft.quota_unit === 'none' && t(locale, 'quotaUnitHintNone')}
          </div>
          {usesLedger && (
            <div className="console-note">{t(locale, 'creditBudgetHint')}</div>
          )}
        </div>
      </div>
      <div className="console-note">{t(locale, 'providerSaveHint')}</div>
      {test && (
        <div className={`console-note ${test.ok ? '' : 'warn'}`.trim()}>{test.message}</div>
      )}
      <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
        <Button size="xs" variant="default" onClick={() => { onClose(); setTest(null) }}>{t(locale, 'cancel')}</Button>
        {/* 私有协议上游没有标准探活接口（base_url 根路径一律 404），不显示「测试配置」 */}
        {!isProprietary && (
          <Tooltip label={draft.base_url.trim() ? t(locale, 'testConfig') : t(locale, 'testNeedsUrl')}>
            <span>
              <Button
                size="xs"
                variant="default"
                leftSection={<IconPlugConnected size={14} />}
                loading={testing}
                disabled={!draft.base_url.trim()}
                onClick={() => void runTest()}
              >
                {t(locale, 'testConfig')}
              </Button>
            </span>
          </Tooltip>
        )}
        <Button
          size="xs"
          loading={saving}
          disabled={!canCreate}
          onClick={() => void create()}
        >
          {t(locale, 'create')}
        </Button>
      </div>
    </Modal>
  )
}
