import { useEffect, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import {
  Button,
  Group,
  NumberInput,
  SegmentedControl,
  Select,
  Textarea,
  TextInput,
} from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { IconAlertTriangle, IconCopy, IconKey, IconPlayerPlay, IconStack2 } from '@tabler/icons-react'
import { useNavigate } from 'react-router-dom'
import { createApiKey, getSettings, listProxies } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import { PageHeader } from '../components/ui/PageHeader'
import { EmptyState, PageSkeleton } from '../components/ui/States'
import { StatusPill } from '../components/ui/StatusPill'
import { formatDuration, formatNumber, httpTone } from '../lib/format'
import type { ProxyRow } from '../types/stats'
import { Field } from '../components/Field'

type Protocol = 'openai' | 'anthropic'

interface PlaygroundResult {
  ok: boolean
  status: number
  latency: number
  content?: string
  error?: string
  usage?: { input?: number; output?: number }
  model: string
  protocol: Protocol
}

export default function Playground() {
  const navigate = useNavigate()
  const { locale } = useAppContext()
  const [model, setModel] = useState('')
  const [protocol, setProtocol] = useState<Protocol>('openai')
  const [systemPrompt, setSystemPrompt] = useState(() => t(locale, 'sampleSystemPrompt'))
  const [message, setMessage] = useState(() => t(locale, 'sampleMessage'))
  const [maxTokens, setMaxTokens] = useState(256)
  const [temperature, setTemperature] = useState(0.7)
  const [loading, setLoading] = useState(false)
  const [result, setResult] = useState<PlaygroundResult | null>(null)
  // 网关 Key 的明文只能取一次（创建/轮换那次响应），所以这里由用户粘贴或现场新建，
  // 只存在组件 state 里：不写 localStorage、不进 query cache，刷新即消失。
  const [apiKey, setApiKey] = useState('')
  const [keyBusy, setKeyBusy] = useState(false)

  const proxiesQuery = useQuery({
    queryKey: ['playground-proxies'],
    queryFn: async (): Promise<ProxyRow[]> => listProxies() as Promise<ProxyRow[]>,
  })

  const settingsQuery = useQuery({
    queryKey: ['settings'],
    queryFn: () => getSettings(),
    staleTime: 60_000,
  })

  const adminPort = settingsQuery.data?.admin_port ?? 1994
  const proxies = proxiesQuery.data ?? []
  const proxy = proxies.find(item => item.name === model) || proxies[0]

  useEffect(() => {
    if (!model && proxies[0]) setModel(proxies[0].name)
  }, [model, proxies])

  // 现场新建一条只给本页用的 Key：返回里的 secret 是这条凭据唯一一次露面。
  const createAndFill = async () => {
    setKeyBusy(true)
    try {
      const stamp = new Date().toTimeString().slice(0, 5).replace(':', '')
      const created = await createApiKey({ name: `playground-${stamp}`, proxy_id: proxy?.id ?? null })
      setApiKey(created.secret || '')
      notifications.show({ message: t(locale, 'playgroundKeyFilled'), color: 'green' })
    } catch {
      notifications.show({ title: t(locale, 'createFailed'), message: '', color: 'red' })
    } finally {
      setKeyBusy(false)
    }
  }

  const send = async () => {
    if (!proxy || !apiKey.trim() || !message.trim()) return
    setLoading(true)
    setResult(null)
    const started = performance.now()
    const isAnthropic = protocol === 'anthropic'
    try {
      const messages = systemPrompt.trim()
        ? [{ role: 'system', content: systemPrompt.trim() }, { role: 'user', content: message.trim() }]
        : [{ role: 'user', content: message.trim() }]
      const body = isAnthropic
        ? {
          model: proxy.name,
          max_tokens: maxTokens,
          system: systemPrompt.trim() || undefined,
          messages: [{ role: 'user', content: message.trim() }],
        }
        : {
          model: proxy.name,
          max_tokens: maxTokens,
          temperature,
          messages,
          stream: false,
        }
      const headers: Record<string, string> = { 'Content-Type': 'application/json' }
      const secret = apiKey.trim()
      if (isAnthropic) {
        headers['x-api-key'] = secret
        headers['anthropic-version'] = '2023-06-01'
      } else {
        headers.Authorization = `Bearer ${secret}`
      }
      const response = await fetch(isAnthropic ? '/v1/messages' : '/v1/chat/completions', {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
      })
      const latency = performance.now() - started
      const data = await response.json().catch(() => null)
      if (!response.ok) {
        setResult({
          ok: false,
          status: response.status,
          latency,
          error: data?.error?.message || data?.error?.type || `HTTP ${response.status}`,
          model: proxy.name,
          protocol,
        })
        return
      }
      const content = isAnthropic
        ? (data?.content || []).map((part: { text?: string }) => part.text).filter(Boolean).join('\n')
        : data?.choices?.[0]?.message?.content
      const usage = isAnthropic
        ? { input: data?.usage?.input_tokens, output: data?.usage?.output_tokens }
        : { input: data?.usage?.prompt_tokens, output: data?.usage?.completion_tokens }
      setResult({
        ok: true,
        status: response.status,
        latency,
        content: content || t(locale, 'emptyResponse'),
        usage,
        model: proxy.name,
        protocol,
      })
    } catch (error) {
      setResult({
        ok: false,
        status: 0,
        latency: performance.now() - started,
        error: error instanceof Error ? error.message : t(locale, 'requestFailed'),
        model: proxy.name,
        protocol,
      })
    } finally {
      setLoading(false)
    }
  }

  if (proxiesQuery.isPending) return <PageSkeleton rows={4} />

  const canSend = Boolean(proxy && apiKey.trim() && message.trim()) && !loading
  const curl = proxy
    ? protocol === 'anthropic'
      ? `curl http://localhost:${adminPort}/v1/messages \\
  -H "Content-Type: application/json" \\
  -H "anthropic-version: 2023-06-01" \\
  -H "x-api-key: <your-api-key>" \\
  --data '{"model":"${proxy.name}","max_tokens":${maxTokens},"messages":[{"role":"user","content":"${message.replace(/'/g, "\\'")}"}]}'`
      : `curl http://localhost:${adminPort}/v1/chat/completions \\
  -H "Content-Type: application/json" \\
  -H "Authorization: Bearer <your-api-key>" \\
  --data '{"model":"${proxy.name}","messages":[{"role":"user","content":"${message.replace(/'/g, "\\'")}"}],"max_tokens":${maxTokens}}'`
    : ''

  return (
    <div className="console-page">
      <PageHeader
        kicker={<>{t(locale, 'kickerTools')} <span className="console-kicker-sep" /> {t(locale, 'kickerPlayground')}</>}
        title={t(locale, 'playground')}
        subtitle={t(locale, 'playgroundDesc')}
        actions={
          <>
            <Button size="xs" variant="default" leftSection={<IconStack2 size={14} />} onClick={() => navigate('/models')}>
              {t(locale, 'openModel')}
            </Button>
            <Button
              size="xs"
              variant="default"
              leftSection={<IconCopy size={14} />}
              disabled={!proxy}
              onClick={() => {
                void navigator.clipboard.writeText(curl)
              }}
            >
              {t(locale, 'copyCurl')}
            </Button>
          </>
        }
      />

      {!proxy ? (
        <section className="console-panel">
          <EmptyState
            title={t(locale, 'noProxyYet')}
            description={t(locale, 'noProxyYetDesc')}
            action={<Button size="xs" onClick={() => navigate('/models')}>{t(locale, 'openVirtualModels')}</Button>}
          />
        </section>
      ) : (
        <>
          <div className="console-note warn">
            <IconAlertTriangle size={13} style={{ marginRight: 6, verticalAlign: -2 }} />
            {t(locale, 'playgroundKeyNote')}
          </div>
          {!apiKey.trim() && (
            <div className="console-note">
              <IconKey size={13} style={{ marginRight: 6, verticalAlign: -2 }} />
              <span>{t(locale, 'noKeyYet')} · {t(locale, 'noKeyYetDesc')}</span>
            </div>
          )}

          <div className="console-playground">
            <section className="console-panel">
              <div className="console-panel-head"><div className="console-panel-title">{t(locale, 'requests')}</div><span className="console-panel-sub">{protocol.toUpperCase()}</span></div>
              <div className="console-panel-body">
                <Field label={t(locale, 'proxy')}>
                  <Select
                    size="xs"
                    data={proxies.map(item => ({ value: item.name, label: item.name }))}
                    value={proxy.name}
                    onChange={value => setModel(value || proxy.name)}
                    allowDeselect={false}
                  />
                </Field>
                <div className="console-field">
                  <label>{t(locale, 'playgroundKeyLabel')}</label>
                  <Group gap={6} wrap="nowrap">
                    <TextInput
                      size="xs"
                      style={{ flexGrow: 1 }}
                      autoComplete="off"
                      placeholder={t(locale, 'playgroundKeyPlaceholder')}
                      value={apiKey}
                      onChange={event => setApiKey(event.currentTarget.value)}
                    />
                    <Button
                      size="xs"
                      variant="default"
                      style={{ flexShrink: 0 }}
                      leftSection={<IconKey size={13} />}
                      loading={keyBusy}
                      onClick={() => void createAndFill()}
                    >
                      {t(locale, 'playgroundKeyGenerate')}
                    </Button>
                  </Group>
                </div>
                <div className="console-field">
                  <label>{t(locale, 'protocolLabel')}</label>
                  <SegmentedControl
                    size="xs"
                    value={protocol}
                    onChange={value => setProtocol(value as Protocol)}
                    data={[
                      { value: 'openai', label: 'OpenAI' },
                      { value: 'anthropic', label: 'Anthropic' },
                    ]}
                  />
                </div>
                <Field label={t(locale, 'systemPrompt')}>
                  <Textarea
                    size="xs"
                    autosize
                    minRows={2}
                    maxRows={5}
                    value={systemPrompt}
                    onChange={event => setSystemPrompt(event.currentTarget.value)}
                  />
                </Field>
                <Field label={t(locale, 'messageLabel')}>
                  <Textarea
                    size="xs"
                    autosize
                    minRows={3}
                    maxRows={7}
                    value={message}
                    onChange={event => setMessage(event.currentTarget.value)}
                  />
                </Field>
                <div className="console-field-row">
                  <Field label={t(locale, 'maxTokensLabel')}>
                    <NumberInput size="xs" min={1} max={8192} value={maxTokens} onChange={value => setMaxTokens(Number(value) || 256)} />
                  </Field>
                  <Field label={t(locale, 'temperatureLabel')}>
                    <NumberInput size="xs" min={0} max={2} step={0.1} value={temperature} onChange={value => setTemperature(Number(value) || 0)} disabled={protocol === 'anthropic'} />
                  </Field>
                </div>
                <div className="console-route-actions">
                  <Button
                    size="sm"
                    leftSection={<IconPlayerPlay size={14} />}
                    loading={loading}
                    disabled={!canSend}
                    onClick={() => void send()}
                  >
                    {t(locale, 'sendTestRequest')}
                  </Button>
                </div>
              </div>
            </section>

            <section className="console-panel console-playground-response">
              <div className="console-panel-head">
                <div className="console-panel-title">{t(locale, 'responses')}</div>
                {result && <StatusPill tone={httpTone(result.status)}>{result.status || t(locale, 'errShort')}</StatusPill>}
              </div>
              <div className="console-panel-body">
                {loading ? (
                  <div className="console-response-box">{t(locale, 'requesting')}</div>
                ) : result ? (
                  <>
                    <div className={`console-response-box${result.ok ? '' : ' error'}`}>
                      {result.ok ? result.content : result.error}
                    </div>
                    <div className="console-metrics">
                      <div className="console-metric"><div className="k">{t(locale, 'thLatency')}</div><div className="v">{formatDuration(result.latency)}</div></div>
                      <div className="console-metric"><div className="k">{t(locale, 'inputTokens')}</div><div className="v">{formatNumber(result.usage?.input ?? 0)}</div></div>
                      <div className="console-metric"><div className="k">{t(locale, 'outputTokens')}</div><div className="v">{formatNumber(result.usage?.output ?? 0)}</div></div>
                      <div className="console-metric"><div className="k">{t(locale, 'model')}</div><div className="v console-mono">{result.model}</div></div>
                    </div>
                    <div className="console-trace-compact">
                      <div className="row"><span className={`console-dot ${result.ok ? 'ok' : 'err'}`} /><span>{t(locale, 'requestLabel')}</span><span>{result.protocol} · {result.status || t(locale, 'errShort')} · {formatDuration(result.latency)}</span></div>
                      <div className="row"><span className="console-dot idle" /><span>{t(locale, 'traceLabel')}</span><span>{t(locale, 'tracePending')}</span></div>
                    </div>
                  </>
                ) : (
                  <EmptyState title={t(locale, 'noResponseYet')} description={t(locale, 'noResponseYetDesc')} />
                )}
              </div>
            </section>
          </div>
        </>
      )}
    </div>
  )
}
