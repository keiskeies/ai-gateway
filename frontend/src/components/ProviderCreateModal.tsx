import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Button, Modal, PasswordInput, Select, TextInput, Tooltip } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconPlugConnected } from '@tabler/icons-react'
import { createPlatform } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
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
}

const emptyDraft: ProviderDraft = { name: '', type: 'OpenAI', base_url: '', api_key: '', organization: '', credit_budget: '', credit_window: 'day' }

const typeOptions = Object.entries(PLATFORM_TYPE_LABELS).map(([value, label]) => ({ value, label }))

// 平台创建表只有这一份：/providers 的新建按钮和首次配置向导共用，避免两处口径分家
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

  const presetOptions = platformPresets.map(p => ({ value: p.name, label: getPresetName(p, locale) }))
  const creditInvalid = draft.credit_budget.trim() !== ''
    && (!Number.isFinite(Number(draft.credit_budget)) || Number(draft.credit_budget) <= 0)

  const applyPreset = (value: string | null) => {
    setPreset(value)
    setTest(null)
    if (!value) return
    const found = platformPresets.find(p => p.name === value)
    if (!found) return
    setDraft(prev => ({ ...prev, name: found.name, type: found.platform_type, base_url: found.base_url }))
  }

  const setField = <K extends keyof ProviderDraft>(key: K, value: ProviderDraft[K]) => {
    setDraft(prev => ({ ...prev, [key]: value }))
    // 连接相关的字段一改，上一次测试结果就不再代表当前配置
    if (key === 'type' || key === 'base_url' || key === 'api_key') setTest(null)
  }

  const reset = () => {
    setDraft(emptyDraft)
    setPreset(null)
    setTest(null)
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

  const create = async () => {
    const name = draft.name.trim()
    const baseUrl = draft.base_url.trim()
    if (!name || !baseUrl) return
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
        credit_budget: budget === '' ? null : Number(budget),
        credit_window: budget === '' ? null : draft.credit_window,
      })
      notifications.show({ message: t(locale, 'createSuccess'), color: 'green' })
      reset()
      await queryClient.invalidateQueries({ queryKey: ['platforms'] })
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
        <Field full label={t(locale, 'baseUrl').toUpperCase()}>
          <TextInput
            size="xs"
            classNames={{ input: 'console-mono' }}
            value={draft.base_url}
            onChange={event => setField('base_url', event.currentTarget.value)}
            placeholder="https://api.openai.com/v1"
          />
        </Field>
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
        <Field label={t(locale, 'creditBudget').toUpperCase()} unit={t(locale, 'creditUnit')}>
          <TextInput
            size="xs"
            inputMode="decimal"
            value={draft.credit_budget}
            onChange={event => setField('credit_budget', event.currentTarget.value)}
            placeholder="250"
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
            disabled={!draft.credit_budget.trim()}
            allowDeselect={false}
          />
        </Field>
        <div className="console-field console-form-full">
          <div className="console-note">{t(locale, 'creditBudgetHint')}</div>
        </div>
      </div>
      <div className="console-note">{t(locale, 'providerSaveHint')}</div>
      {test && (
        <div className={`console-note ${test.ok ? '' : 'warn'}`.trim()}>{test.message}</div>
      )}
      <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
        <Button size="xs" variant="default" onClick={() => { onClose(); setTest(null) }}>{t(locale, 'cancel')}</Button>
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
        <Button
          size="xs"
          loading={saving}
          disabled={!draft.name.trim() || !draft.base_url.trim() || creditInvalid}
          onClick={() => void create()}
        >
          {t(locale, 'create')}
        </Button>
      </div>
    </Modal>
  )
}
