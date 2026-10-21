import { useState, type ReactNode } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, CopyButton, NumberInput, PasswordInput, Select, TextInput } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import {
  IconArrowLeft,
  IconCheck,
  IconCopy,
  IconExternalLink,
  IconPlus,
  IconPlugConnected,
} from '@tabler/icons-react'
import { useNavigate } from 'react-router-dom'
import {
  createApiKey,
  createProxy,
  createRoute,
  getRequestLogs,
  listApiKeys,
  listModels,
  listPlatforms,
  listProxies,
  syncPlatformModels,
} from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import type { Locale } from '../i18n'
import { EmptyState } from '../components/ui/States'
import { StatusPill } from '../components/ui/StatusPill'
import { ProviderCreateModal } from '../components/ProviderCreateModal'
import type { ApiKeyRow, PlatformDetail, ProxyRow, RequestLogPage } from '../types/stats'
import { Field } from '../components/Field'

interface ModelRow {
  id: string
  platform_id: string
  model_id: string
  display_name?: string | null
}

interface SendResult {
  ok: boolean
  status: number
  latencyMs: number
  text: string
}

// 向导只铺"最小可用"那一条路：完整编辑器仍然在 /providers、/models/:id、/keys
export default function Setup() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { locale } = useAppContext()

  const [step, setStep] = useState(0)
  const [providerOpen, setProviderOpen] = useState(false)
  const [gatewayKey, setGatewayKey] = useState('')
  const [sentOk, setSentOk] = useState(false)

  const platformsQuery = useQuery({ queryKey: ['platforms'], queryFn: () => listPlatforms() as Promise<PlatformDetail[]> })
  const proxiesQuery = useQuery({ queryKey: ['proxies'], queryFn: () => listProxies() as Promise<ProxyRow[]> })
  const keysQuery = useQuery({ queryKey: ['api-keys'], queryFn: () => listApiKeys() as Promise<ApiKeyRow[]> })
  const trafficQuery = useQuery({
    queryKey: ['wizard-traffic'],
    queryFn: () => getRequestLogs({ page: 1, page_size: 1 }) as Promise<RequestLogPage>,
  })

  const platforms = platformsQuery.data ?? []
  const proxies = proxiesQuery.data ?? []
  const keys = keysQuery.data ?? []

  // 步骤状态一律从库里推导，不记本地标记：中途退出再进来，看到的仍是真实进度
  const done = [
    platforms.length > 0,
    proxies.length > 0,
    keys.length > 0,
    sentOk || (trafficQuery.data?.total ?? 0) > 0,
  ]
  const doneCount = done.filter(Boolean).length

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ['platforms'] })
    void queryClient.invalidateQueries({ queryKey: ['proxies'] })
    void queryClient.invalidateQueries({ queryKey: ['models'] })
    void queryClient.invalidateQueries({ queryKey: ['api-keys'] })
    void queryClient.invalidateQueries({ queryKey: ['overview'] })
    void queryClient.invalidateQueries({ queryKey: ['wizard-traffic'] })
  }

  const leave = () => navigate('/')
  const skip = () => {
    localStorage.setItem('wizard', 'skipped')
    navigate('/')
  }

  return (
    <div className="console-wizard">
      <header className="console-wizard-head">
        <div>
          <div className="console-kicker">
            SETUP <span className="console-kicker-sep" /> WIZARD
          </div>
          <h1>{t(locale, 'wizardTitle')}</h1>
          <p>{t(locale, 'wizardDesc')}</p>
        </div>
        <div className="console-wizard-actions">
          <span className="console-wizard-progress">{t(locale, 'wizardProgress', String(doneCount))}</span>
          <Button size="xs" variant="default" onClick={leave}>{t(locale, 'wizardExit')}</Button>
          <Button size="xs" variant="subtle" color="gray" title={t(locale, 'wizardSkipTip')} onClick={skip}>{t(locale, 'wizardSkipAll')}</Button>
        </div>
      </header>

      <div className="console-wizard-body">
        {/* 完成状态来自库里而非进度游标，四步可以乱序勾选，所以不用 Mantine Stepper 的线性样式 */}
        <div className="console-wizard-steps">
          {STEP_KEYS.map((label, index) => (
            <div className="console-wizard-segment" key={label}>
              <button
                type="button"
                className={`console-wizard-step${step === index ? ' active' : ''}`}
                aria-current={step === index ? 'step' : undefined}
                onClick={() => setStep(index)}
              >
                <span className="console-wizard-step-top">
                  <span className="console-wizard-step-index">0{index + 1}</span>
                  <StepState locale={locale} done={done[index]} optional={index === 3} />
                </span>
                <span className="console-wizard-step-label">{t(locale, label)}</span>
              </button>
              {index < STEP_KEYS.length - 1 && <span className="console-wizard-step-link" aria-hidden="true" />}
            </div>
          ))}
        </div>

        <div className="console-wizard-panel-slot">
          {step === 0 && (
            <StepPanel title={t(locale, 'stepPlatforms')} desc={t(locale, 'wizardStep1Desc')}>
              {platforms.length === 0 ? (
                <EmptyState
                  title={t(locale, 'noData')}
                  action={(
                    <Button size="xs" leftSection={<IconPlugConnected size={14} />} onClick={() => setProviderOpen(true)}>
                      {t(locale, 'wizardAddProvider')}
                    </Button>
                  )}
                />
              ) : (
                <>
                  <ChipList names={platforms.map(platform => platform.name)} />
                  <div className="console-note">{t(locale, 'wizardPlatforms', String(platforms.length))}</div>
                  <div className="console-inspector-actions">
                    <Button size="xs" variant="default" leftSection={<IconPlus size={14} />} onClick={() => setProviderOpen(true)}>
                      {t(locale, 'addPlatform')}
                    </Button>
                  </div>
                </>
              )}
            </StepPanel>
          )}

          {step === 1 && (
            <StepPanel title={t(locale, 'stepModels')} desc={t(locale, 'wizardStep2Desc')}>
              {platforms.length === 0 ? (
                <div className="console-note warn">{t(locale, 'wizardNeedPlatformFirst')}</div>
              ) : (
                <ModelStep
                  locale={locale}
                  platforms={platforms}
                  proxies={proxies}
                  onCreated={() => { refresh(); setStep(2) }}
                />
              )}
            </StepPanel>
          )}

          {step === 2 && (
            <StepPanel title={t(locale, 'stepKeys')} desc={t(locale, 'wizardStep3Desc')}>
              <KeyStep
                locale={locale}
                proxies={proxies}
                keyCount={keys.length}
                onCreated={secret => {
                  setGatewayKey(secret)
                  refresh()
                }}
              />
            </StepPanel>
          )}

          {step === 3 && (
            <StepPanel title={t(locale, 'stepRequests')} desc={t(locale, 'wizardStep4Desc')}>
              {proxies.length === 0 ? (
                <div className="console-note warn">{t(locale, 'wizardNeedModelFirst')}</div>
              ) : (
                <RequestStep
                  locale={locale}
                  proxies={proxies}
                  gatewayKey={gatewayKey}
                  onSent={ok => {
                    setSentOk(ok)
                    refresh()
                    if (ok) notifications.show({ message: t(locale, 'wizardDoneToast'), color: 'green' })
                  }}
                  onOpenPlayground={() => navigate('/playground')}
                />
              )}
            </StepPanel>
          )}
        </div>
      </div>

      <footer className="console-wizard-foot">
        <Button
          size="xs"
          variant="default"
          leftSection={<IconArrowLeft size={14} />}
          disabled={step === 0}
          onClick={() => setStep(value => Math.max(0, value - 1))}
        >
          {t(locale, 'wizardPrev')}
        </Button>
        <span className="console-wizard-stepof">{t(locale, 'wizardStepOf', String(step + 1))}</span>
        {step === 3 ? (
          <Button size="xs" leftSection={<IconCheck size={14} />} onClick={leave}>{t(locale, 'wizardFinish')}</Button>
        ) : (
          <Button size="xs" onClick={() => setStep(value => Math.min(3, value + 1))}>{t(locale, 'wizardNext')}</Button>
        )}
      </footer>

      <ProviderCreateModal
        opened={providerOpen}
        onClose={() => setProviderOpen(false)}
        onCreated={() => {
          refresh()
          setStep(1)
        }}
      />
    </div>
  )
}

const STEP_KEYS = ['stepPlatforms', 'stepModels', 'stepKeys', 'stepRequests'] as const

function StepState({ locale, done, optional }: { locale: Locale; done: boolean; optional: boolean }) {
  if (done) return <StatusPill tone="ok">{t(locale, 'wizardStateDone')}</StatusPill>
  if (optional) return <StatusPill tone="idle">{t(locale, 'wizardStateOptional')}</StatusPill>
  return <StatusPill tone="warn">{t(locale, 'wizardStateTodo')}</StatusPill>
}

function StepPanel({ title, desc, children }: { title: string; desc: string; children: ReactNode }) {
  return (
    <section className="console-panel console-wizard-panel">
      <div className="console-panel-head">
        <div>
          <div className="console-panel-title">{title}</div>
          <div className="console-panel-sub">{desc}</div>
        </div>
      </div>
      <div className="console-panel-body">{children}</div>
    </section>
  )
}

function ChipList({ names }: { names: string[] }) {
  return (
    <div className="console-wizard-list">
      {names.map(name => <span key={name} className="console-wizard-chip">{name}</span>)}
    </div>
  )
}

function ModelStep({
  locale,
  platforms,
  proxies,
  onCreated,
}: {
  locale: Locale
  platforms: PlatformDetail[]
  proxies: ProxyRow[]
  onCreated: () => void
}) {
  const queryClient = useQueryClient()
  const [name, setName] = useState('')
  const [platformId, setPlatformId] = useState<string | null>(platforms[0]?.id ?? null)
  const [modelId, setModelId] = useState<string | null>(null)
  const [weight, setWeight] = useState<string | number>(100)
  const [busy, setBusy] = useState(false)
  const [syncing, setSyncing] = useState(false)

  const modelsQuery = useQuery({
    queryKey: ['platform-models', platformId],
    queryFn: () => listModels() as Promise<ModelRow[]>,
    enabled: Boolean(platformId),
  })
  const models = (modelsQuery.data ?? []).filter(model => model.platform_id === platformId)

  const runSync = async () => {
    if (!platformId) return
    setSyncing(true)
    try {
      const result = await syncPlatformModels(platformId)
      if (result?.ok) {
        notifications.show({
          message: t(locale, 'syncModelsDone', String(result.created ?? 0), String(result.existing ?? 0)),
          color: 'green',
        })
        await queryClient.invalidateQueries({ queryKey: ['platform-models', platformId] })
      } else {
        notifications.show({ message: t(locale, 'syncModelsFailed', result?.error || ''), color: 'red' })
      }
    } catch (error: any) {
      notifications.show({
        message: t(locale, 'syncModelsFailed', error?.response?.data?.error?.message || ''),
        color: 'red',
      })
    } finally {
      setSyncing(false)
    }
  }

  const create = async () => {
    const trimmed = name.trim()
    if (!trimmed || !platformId || !modelId) return
    setBusy(true)
    try {
      const proxy = await createProxy({ name: trimmed }) as ProxyRow
      await createRoute(proxy.id, {
        lb_strategy: 'RoundRobin',
        default_max_tokens: 4096,
        backends: [{
          platform_id: platformId,
          model_id: modelId,
          weight: Number(weight) > 0 ? Math.floor(Number(weight)) : 100,
        }],
      })
      notifications.show({ message: t(locale, 'createSuccess'), color: 'green' })
      setName('')
      onCreated()
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'createFailed'),
        message: error?.response?.data?.error?.message || t(locale, 'requestFailed'),
        color: 'red',
      })
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <div className="console-form-grid">
        <Field label={t(locale, 'wizardExternalName').toUpperCase()}>
          <TextInput size="xs" value={name} onChange={event => setName(event.currentTarget.value)} placeholder="chat-main" />
        </Field>
        <Field label={t(locale, 'wizardPickPlatform').toUpperCase()}>
          <Select
            size="xs"
            data={platforms.map(platform => ({ value: platform.id, label: platform.name }))}
            value={platformId}
            onChange={value => { setPlatformId(value); setModelId(null) }}
            allowDeselect={false}
          />
        </Field>
        <Field label={t(locale, 'wizardPickModel').toUpperCase()}>
          <Select
            size="xs"
            data={models.map(model => ({ value: model.model_id, label: model.display_name || model.model_id }))}
            value={modelId}
            onChange={setModelId}
            disabled={models.length === 0}
            allowDeselect={false}
            placeholder={t(locale, 'wizardPickModel')}
          />
        </Field>
        <Field label={t(locale, 'wizardWeight').toUpperCase()}>
          <NumberInput size="xs" value={weight} onChange={setWeight} min={1} hideControls />
        </Field>
      </div>
      {modelsQuery.isSuccess && models.length === 0 && (
        <div className="console-note warn">
          <span>{t(locale, 'wizardNoModels')}</span>
          <Button size="xs" variant="light" loading={syncing} onClick={() => void runSync()} ml="sm">
            {t(locale, 'wizardSyncModels')}
          </Button>
        </div>
      )}
      {proxies.length > 0 && (
        <>
          <ChipList names={proxies.map(proxy => proxy.name)} />
          <div className="console-note">{t(locale, 'wizardModels', String(proxies.length))}</div>
        </>
      )}
      <div className="console-inspector-actions">
        <Button size="xs" loading={busy} disabled={!name.trim() || !modelId} onClick={() => void create()}>
          {t(locale, 'wizardCreateModelBtn')}
        </Button>
      </div>
    </>
  )
}

function KeyStep({
  locale,
  proxies,
  keyCount,
  onCreated,
}: {
  locale: Locale
  proxies: ProxyRow[]
  keyCount: number
  onCreated: (secret: string) => void
}) {
  const queryClient = useQueryClient()
  const [name, setName] = useState('')
  const [proxyId, setProxyId] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [secret, setSecret] = useState('')

  const create = async () => {
    const trimmed = name.trim()
    if (!trimmed) return
    setBusy(true)
    try {
      const result = await createApiKey({ name: trimmed, proxy_id: proxyId || null })
      const plain = result?.secret || ''
      setSecret(plain)
      onCreated(plain)
      await queryClient.invalidateQueries({ queryKey: ['api-keys'] })
    } catch (error: any) {
      notifications.show({
        title: t(locale, 'createFailed'),
        message: error?.response?.data?.error?.message || t(locale, 'requestFailed'),
        color: 'red',
      })
    } finally {
      setBusy(false)
    }
  }

  if (secret) {
    return (
      <>
        <div className="ds-hint">{t(locale, 'apiKeyCreatedTip')}</div>
        <div className="ds-keybox">
          <span className="ds-mono" style={{ color: 'var(--text-1)', wordBreak: 'break-all' }}>{secret}</span>
          <CopyButton value={secret}>
            {({ copied, copy }) => (
              <Button
                size="xs"
                variant={copied ? 'filled' : 'light'}
                leftSection={copied ? <IconCheck size={14} /> : <IconCopy size={14} />}
                onClick={copy}
                style={{ flexShrink: 0 }}
              >
                {copied ? (locale === 'zh' ? '已复制' : 'Copied') : (locale === 'zh' ? '复制' : 'Copy')}
              </Button>
            )}
          </CopyButton>
        </div>
        <div className="console-note warn">{t(locale, 'apiKeyCopyWarning')}</div>
        <div className="console-inspector-actions">
          <Button size="xs" variant="default" onClick={() => { setSecret(''); setName('') }}>
            {t(locale, 'newApiKey')}
          </Button>
        </div>
      </>
    )
  }

  return (
    <>
      <div className="console-form-grid">
        <Field label={t(locale, 'apiKeyName').toUpperCase()}>
          <TextInput
            size="xs"
            value={name}
            onChange={event => setName(event.currentTarget.value)}
            placeholder={t(locale, 'apiKeyNamePlaceholder')}
            data-autofocus
          />
        </Field>
        <Field label={t(locale, 'apiKeyProxy').toUpperCase()}>
          <Select
            size="xs"
            data={proxies.map(proxy => ({ value: proxy.id, label: proxy.name }))}
            value={proxyId}
            onChange={setProxyId}
            clearable
            placeholder={t(locale, 'apiKeyProxyAll')}
          />
        </Field>
      </div>
      <div className="console-note">{t(locale, 'apiKeyProxyAll')}</div>
      {keyCount > 0 && <div className="console-note">{t(locale, 'wizardKeys', String(keyCount))}</div>}
      <div className="console-inspector-actions">
        <Button size="xs" loading={busy} disabled={!name.trim()} onClick={() => void create()}>
          {t(locale, 'stepKeys')}
        </Button>
      </div>
    </>
  )
}

function RequestStep({
  locale,
  proxies,
  gatewayKey,
  onSent,
  onOpenPlayground,
}: {
  locale: Locale
  proxies: ProxyRow[]
  gatewayKey: string
  onSent: (ok: boolean) => void
  onOpenPlayground: () => void
}) {
  const [model, setModel] = useState<string | null>(proxies[0]?.name ?? null)
  const [message, setMessage] = useState(locale === 'zh' ? '只回复一个字：好' : 'Reply with a single word.')
  const [pasted, setPasted] = useState('')
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<SendResult | null>(null)
  const key = gatewayKey || pasted.trim()

  const send = async () => {
    if (!model || !key) return
    setBusy(true)
    const started = performance.now()
    try {
      const response = await fetch('/v1/chat/completions', {
        method: 'POST',
        headers: { 'content-type': 'application/json', Authorization: `Bearer ${key}` },
        body: JSON.stringify({ model, messages: [{ role: 'user', content: message }], max_tokens: 64 }),
      })
      const data = await response.json().catch(() => null)
      const latencyMs = Math.round(performance.now() - started)
      if (!response.ok) {
        setResult({
          ok: false,
          status: response.status,
          latencyMs,
          text: data?.error?.message || data?.error?.type || `HTTP ${response.status}`,
        })
        onSent(false)
      } else {
        const text: string = data?.choices?.[0]?.message?.content
          || data?.choices?.[0]?.text
          || JSON.stringify(data).slice(0, 400)
        setResult({ ok: true, status: response.status, latencyMs, text })
        onSent(true)
      }
    } catch (error: any) {
      setResult({ ok: false, status: 0, latencyMs: 0, text: error?.message || t(locale, 'testFailedGeneric') })
      onSent(false)
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <div className="console-form-grid">
        <Field label={t(locale, 'model').toUpperCase()}>
          <Select
            size="xs"
            data={proxies.map(proxy => ({ value: proxy.name, label: proxy.name }))}
            value={model}
            onChange={setModel}
            allowDeselect={false}
          />
        </Field>
        <Field label={t(locale, 'wizardKeyField').toUpperCase()}>
          <PasswordInput
            size="xs"
            value={gatewayKey || pasted}
            onChange={event => setPasted(event.currentTarget.value)}
            disabled={Boolean(gatewayKey)}
            placeholder="sk-aigw-..."
          />
        </Field>
        <Field full label={t(locale, 'wizardMessage').toUpperCase()}>
          <TextInput size="xs" value={message} onChange={event => setMessage(event.currentTarget.value)} />
        </Field>
      </div>
      {!gatewayKey && keyCountHint(locale, pasted)}
      <div className="console-note">{t(locale, 'wizardStep4Cost')}</div>
      {result && (
        <div className={`console-note ${result.ok ? '' : 'warn'}`.trim()}>
          {`${result.ok ? t(locale, 'wizardSentOk') : t(locale, 'wizardSentFail')} · HTTP ${result.status || '—'} · ${result.latencyMs} ms`}
          {result.text ? ` · ${result.ok ? `${t(locale, 'wizardReply')}: ` : ''}${result.text}` : ''}
        </div>
      )}
      <div className="console-inspector-actions">
        <Button size="xs" loading={busy} disabled={!key || !model} onClick={() => void send()}>
          {t(locale, 'wizardSend')}
        </Button>
        <Button size="xs" variant="default" leftSection={<IconExternalLink size={14} />} onClick={onOpenPlayground}>
          {t(locale, 'wizardOpenPlayground')}
        </Button>
      </div>
    </>
  )
}

function keyCountHint(locale: Locale, pasted: string): ReactNode {
  if (pasted.trim()) return null
  return <div className="console-note warn">{t(locale, 'wizardNoKeyYet')}</div>
}
