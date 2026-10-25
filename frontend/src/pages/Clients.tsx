import { useEffect, useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import {
  Button,
  Modal,
  Select,
  Table,
  TextInput,
} from '@mantine/core'
import { notifications } from '@mantine/notifications'
import {
  IconCheck,
  IconEye,
  IconHistory,
  IconPlugConnected,
  IconRefresh,
  IconArrowBackUp,
} from '@tabler/icons-react'
import { useNavigate } from 'react-router-dom'
import {
  applyClient,
  getSettings,
  listClients,
  listProxies,
  previewClient,
  restoreClient,
  restoreClientBackup,
} from '../api'
import { Field } from '../components/Field'
import { EmptyState, ErrorState, PageSkeleton } from '../components/ui/States'
import { PageHeader } from '../components/ui/PageHeader'
import { StatusPill } from '../components/ui/StatusPill'
import { useAppContext } from '../ThemeContext'
import { t, type Locale, type TranslationKey } from '../i18n'
import type {
  ClientApplyBody,
  ClientApplyResult,
  ClientId,
  ClientPreview,
  ClientRestoreResult,
  ClientStatus,
} from '../types/clients'
import type { ProxyRow } from '../types/stats'

const CLIENT_NAMES: Record<ClientId, string> = {
  claude_code: 'Claude Code',
  codex: 'Codex',
  opencode: 'OpenCode',
  openclaw: 'OpenClaw',
  pi: 'Pi',
  hermes: 'Hermes',
}

const CLIENT_HINT: Record<ClientId, TranslationKey> = {
  claude_code: 'clientHintClaude',
  codex: 'clientHintCodex',
  opencode: 'clientHintOpencode',
  openclaw: 'clientHintOpenclaw',
  pi: 'clientHintPi',
  hermes: 'clientHintHermes',
}

type ConfirmKind = 'apply' | 'restore' | 'restore-backup' | null
type Busy = 'preview' | 'apply' | 'restore' | 'restore-backup' | null

interface ResultNotice {
  kind: 'apply' | 'restore' | 'restore-backup'
  text: string
  keyMask?: string
  backup?: string
  warnings: string[]
}

function clientName(id: ClientId): string {
  return CLIENT_NAMES[id] || id
}

function errorMessage(error: unknown, locale: Locale): string {
  const anyError = error as { response?: { data?: { error?: { message?: string } } }; message?: string }
  return anyError?.response?.data?.error?.message || anyError?.message || t(locale, 'requestFailed')
}

function shortPath(path: string): string {
  const separator = path.includes('\\') ? '\\' : '/'
  const parts = path.split(/[\\/]+/).filter(Boolean)
  if (parts.length <= 2) return path
  return `…${separator}${parts.slice(-2).join(separator)}`
}

const WARNING_ZH: { match: string; text: string }[] = [
  { match: 'An existing ANTHROPIC_AUTH_TOKEN was replaced', text: '原有 ANTHROPIC_AUTH_TOKEN 已被替换；旧值只保留在备份里。' },
  { match: 'The previous ANTHROPIC_AUTH_TOKEN cannot be restored', text: '原有 ANTHROPIC_AUTH_TOKEN 无法原地恢复；需要时请使用应用前备份。' },
  { match: 'ANTHROPIC_API_KEY is also set', text: '同时设置了 ANTHROPIC_API_KEY；Claude Code 可能优先使用它。' },
  { match: 'Existing [model_providers.ai-gateway] was replaced', text: '已有 [model_providers.ai-gateway]，已被替换；旧文件在备份里。' },
  { match: 'An existing AI Gateway token was replaced', text: '已有 AI Gateway Token 被替换；旧值只在备份里。' },
  { match: 'An existing AI Gateway API key was replaced', text: '已有 AI Gateway API Key 被替换；旧值只在备份里。' },
  { match: 'Codex is configured with wire_api', text: 'Codex 使用 Chat wire API；Responses 专属能力可能不可用。' },
  { match: 'This requires a Codex version that reads experimental_bearer_token', text: '需要支持 experimental_bearer_token 的 Codex 版本。' },
  { match: '~/.codex/auth.json was left untouched', text: '~/.codex/auth.json 未改动；requires_openai_auth=false，官方登录不会发给第三方。' },
  { match: 'OpenCode JSONC comments', text: 'OpenCode JSONC 注释/格式会被规范化；原文件在备份里。' },
  { match: "OpenCode's newer `providers` block", text: '检测到 OpenCode 新版 providers 块；AI Gateway 写入的是 provider 块。' },
  { match: 'OpenClaw JSON5 comments', text: 'OpenClaw JSON5 注释/格式会被规范化；原文件在备份里。' },
  { match: 'Pi JSON5 comments', text: 'Pi JSON5 注释/格式会被规范化；原文件在备份里。' },
  { match: "Pi's active model is stored in settings.json", text: 'Pi 的当前模型在 settings.json，由 Pi 自己管理。' },
  { match: 'Select the AI Gateway model inside Pi', text: '请在 Pi 里选择 AI Gateway 模型；这里不写默认模型。' },
  { match: 'Hermes YAML comments', text: 'Hermes YAML 注释/格式会被规范化；原文件在备份里。' },
  { match: 'The AI Gateway provider entry was left in place', text: 'AI Gateway provider 条目已保留；如需完全回退请使用应用前备份。' },
  { match: 'The config file no longer exists', text: '配置文件已不存在，无需撤销。' },
  { match: 'Full file restore used the pre-apply backup', text: '已用应用前备份整份覆盖；应用后的修改已丢失。' },
  { match: 'Managed state is corrupt', text: '托管状态已损坏；请重新应用或恢复应用前备份。' },
  { match: 'A managed client config is missing on disk', text: '托管状态存在但配置文件已丢失；请重新应用或恢复备份。' },
  { match: 'The managed gateway key is disabled', text: '托管网关 Key 已停用；请重新应用以签发新 Key。' },
  { match: 'The managed gateway key no longer exists', text: '托管网关 Key 已不存在；请重新应用以签发新 Key。' },
  { match: 'Existing provider.ai-gateway', text: '已有 provider.ai-gateway，已被替换；旧文件在备份里。' },
  { match: 'Existing models.providers.ai-gateway', text: '已有 models.providers.ai-gateway，已被替换；旧文件在备份里。' },
  { match: 'Existing providers.ai-gateway', text: '已有 providers.ai-gateway，已被替换；旧文件在备份里。' },
  { match: 'Existing custom_providers.ai-gateway', text: '已有 custom_providers.ai-gateway，已被替换；旧文件在备份里。' },
  { match: 'provider.ai-gateway already exists', text: 'provider.ai-gateway 已存在，且不是 AI Gateway 托管的。' },
  { match: 'models.providers.ai-gateway already exists', text: 'models.providers.ai-gateway 已存在，且不是 AI Gateway 托管的。' },
  { match: 'providers.ai-gateway already exists', text: 'providers.ai-gateway 已存在，且不是 AI Gateway 托管的。' },
  { match: 'custom_providers.ai-gateway already exists', text: 'custom_providers.ai-gateway 已存在，且不是 AI Gateway 托管的。' },
]

function warningText(warning: string, locale: Locale): string {
  if (locale !== 'zh') return warning
  const previousKey = warning.match(/^Previous gateway key '(.+)' was disabled\.$/)
  if (previousKey) return `上一条网关 Key「${previousKey[1]}」已停用。`
  const hit = WARNING_ZH.find(item => warning.includes(item.match))
  return hit ? hit.text : warning
}

function statusPill(status: ClientStatus, locale: Locale) {
  if (status.error) return <StatusPill tone="err">{t(locale, 'clientConfigError')}</StatusPill>
  if (status.managed) return <StatusPill tone="ok">{t(locale, 'clientManaged')}</StatusPill>
  if (status.detected) return <StatusPill tone="warn">{t(locale, 'clientDetected')}</StatusPill>
  return <StatusPill tone="idle">{t(locale, 'clientMissing')}</StatusPill>
}

export default function Clients() {
  const { locale } = useAppContext()
  const navigate = useNavigate()

  const clientsQuery = useQuery({
    queryKey: ['clients'],
    queryFn: () => listClients() as Promise<ClientStatus[]>,
  })
  const proxiesQuery = useQuery({
    queryKey: ['client-proxies'],
    queryFn: () => listProxies() as Promise<ProxyRow[]>,
  })
  const settingsQuery = useQuery({
    queryKey: ['settings'],
    queryFn: () => getSettings(),
    staleTime: 60_000,
  })

  const clients = clientsQuery.data ?? []
  const proxies = proxiesQuery.data ?? []
  const modelNames = useMemo(() => proxies.map(proxy => proxy.name), [proxies])
  const adminPort = settingsQuery.data?.admin_port ?? 1994

  const [selectedId, setSelectedId] = useState<ClientId>('claude_code')
  const [baseUrl, setBaseUrl] = useState('')
  const [defaultModel, setDefaultModel] = useState('')
  const [sonnet, setSonnet] = useState<string | null>(null)
  const [opus, setOpus] = useState<string | null>(null)
  const [haiku, setHaiku] = useState<string | null>(null)
  const [fable, setFable] = useState<string | null>(null)
  const [subagent, setSubagent] = useState<string | null>(null)
  const [preview, setPreview] = useState<ClientPreview | null>(null)
  const [busy, setBusy] = useState<Busy>(null)
  const [confirm, setConfirm] = useState<ConfirmKind>(null)
  const [notice, setNotice] = useState<ResultNotice | null>(null)

  const selected = clients.find(client => client.id === selectedId)

  useEffect(() => {
    setBaseUrl(`http://127.0.0.1:${adminPort}`)
  }, [adminPort])

  useEffect(() => {
    if (!modelNames.length) return
    setDefaultModel(previous => (previous && modelNames.includes(previous) ? previous : modelNames[0]))
  }, [modelNames])

  useEffect(() => {
    setPreview(null)
    setNotice(null)
  }, [selectedId])

  const applyBody = (): ClientApplyBody => ({
    base_url: baseUrl.trim() || undefined,
    default_model: defaultModel,
    ...(selectedId === 'claude_code'
      ? {
          sonnet_model: sonnet || undefined,
          opus_model: opus || undefined,
          haiku_model: haiku || undefined,
          fable_model: fable || undefined,
          subagent_model: subagent || undefined,
        }
      : {}),
  })

  const showError = (error: unknown) => {
    notifications.show({
      title: t(locale, 'clientOperationFailed'),
      message: errorMessage(error, locale),
      color: 'red',
      autoClose: 8000,
      withCloseButton: false,
    })
  }

  const refresh = async () => {
    await clientsQuery.refetch()
  }

  const runPreview = async () => {
    if (!selected || !defaultModel) return
    setBusy('preview')
    try {
      setPreview(await previewClient(selected.id, applyBody()))
      setNotice(null)
    } catch (error) {
      showError(error)
    } finally {
      setBusy(null)
    }
  }

  const runApply = async () => {
    if (!selected || !defaultModel) return
    const id = selected.id
    setConfirm(null)
    setBusy('apply')
    try {
      const result: ClientApplyResult = await applyClient(id, applyBody())
      const text = t(locale, 'clientApplied', clientName(id), result.key_name)
      setPreview(null)
      if (selectedId === id) {
        setNotice({
          kind: 'apply',
          text,
          keyMask: result.key_mask,
          backup: result.backup_path || undefined,
          warnings: result.warnings,
        })
      }
      await refresh()
    } catch (error) {
      showError(error)
    } finally {
      setBusy(null)
    }
  }

  const runRestore = async () => {
    if (!selected) return
    const id = selected.id
    setConfirm(null)
    setBusy('restore')
    try {
      const result: ClientRestoreResult = await restoreClient(id)
      const text = result.disabled_key
        ? t(locale, 'clientRestoredWithKey', result.disabled_key)
        : t(locale, 'clientRestored')
      if (selectedId === id) {
        setNotice({ kind: 'restore', text, backup: result.backup_path || undefined, warnings: result.warnings })
      }
      await refresh()
    } catch (error) {
      showError(error)
    } finally {
      setBusy(null)
    }
  }

  const runRestoreBackup = async () => {
    if (!selected) return
    const id = selected.id
    setConfirm(null)
    setBusy('restore-backup')
    try {
      const result: ClientRestoreResult = await restoreClientBackup(id)
      const text = t(locale, 'clientRestoredBackup')
      if (selectedId === id) {
        setNotice({ kind: 'restore-backup', text, backup: result.backup_path || undefined, warnings: result.warnings })
      }
      await refresh()
    } catch (error) {
      showError(error)
    } finally {
      setBusy(null)
    }
  }

  if (clientsQuery.isPending) return <PageSkeleton rows={6} />
  if (clientsQuery.isError) {
    return <ErrorState onRetry={() => void clientsQuery.refetch()} />
  }

  const canEdit = Boolean(selected && defaultModel && modelNames.length > 0 && !selected.error)
  const hints = selected ? [selected.error, ...selected.warnings].filter(Boolean) as string[] : []

  return (
    <div className="console-page">
      <PageHeader
        kicker={<>{t(locale, 'kickerConfigure')} <span className="console-kicker-sep" /> {t(locale, 'clients')}</>}
        title={t(locale, 'clients')}
        subtitle={t(locale, 'clientsDesc')}
        actions={(
          <Button
            size="xs"
            variant="default"
            leftSection={<IconRefresh size={14} />}
            loading={clientsQuery.isFetching}
            onClick={() => void refresh()}
          >
            {t(locale, 'clientRefresh')}
          </Button>
        )}
      />

      <section className="console-panel">
        <div className="console-panel-head">
          <div className="console-panel-title">{t(locale, 'clientDetectedList')}</div>
          <span className="console-panel-sub">{clients.length} {t(locale, 'clientClients')}</span>
        </div>
        <div className="console-panel-body">
          <Table className="console-clients-table" highlightOnHover>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>{t(locale, 'clientName')}</Table.Th>
                <Table.Th>{t(locale, 'clientPath')}</Table.Th>
                <Table.Th>{t(locale, 'clientStatus')}</Table.Th>
                <Table.Th>{t(locale, 'clientCurrent')}</Table.Th>
                <Table.Th style={{ width: 96 }} />
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {clients.map(client => (
                <Table.Tr key={client.id}>
                  <Table.Td>
                    <span className="console-mono">{clientName(client.id)}</span>
                  </Table.Td>
                  <Table.Td>
                    <span className="console-mono console-clients-path" title={client.path}>{shortPath(client.path)}</span>
                  </Table.Td>
                  <Table.Td>{statusPill(client, locale)}</Table.Td>
                  <Table.Td>
                    <span className="console-clients-current">
                      {client.current.base_url || '—'}
                      {client.current.model ? ` · ${client.current.model}` : ''}
                    </span>
                  </Table.Td>
                  <Table.Td>
                    <Button
                      size="compact-xs"
                      variant={selectedId === client.id ? 'light' : 'default'}
                      disabled={busy !== null}
                      onClick={() => setSelectedId(client.id)}
                    >
                      {t(locale, 'clientConfigure')}
                    </Button>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </div>
      </section>

      {selected && (
        <section className="console-panel" style={{ marginTop: 14 }}>
          <div className="console-panel-head">
            <div className="console-panel-title">
              <IconPlugConnected size={14} stroke={1.7} />
              {clientName(selected.id)}
              {statusPill(selected, locale)}
            </div>
            <span className="console-panel-sub console-mono" title={selected.path}>{shortPath(selected.path)}</span>
          </div>
          <div className="console-panel-body">
            <div className="console-note">
              {t(locale, CLIENT_HINT[selected.id])}
            </div>
            {hints.length > 0 && (
              <div className="console-clients-warnings">
                {hints.map((hint, index) => (
                  <div key={`${hint}-${index}`} className="console-note warn">{warningText(hint, locale)}</div>
                ))}
                {selected.error && (
                  <div className="console-note warn">{t(locale, 'clientErrorHint')}</div>
                )}
              </div>
            )}

            {modelNames.length === 0 ? (
              <EmptyState
                title={t(locale, 'clientNoModels')}
                description={t(locale, 'clientNoModelsDesc')}
                action={<Button size="xs" onClick={() => navigate('/models')}>{t(locale, 'clientGoModels')}</Button>}
              />
            ) : (
              <>
                <div className="console-form-grid">
                  <Field full label={t(locale, 'clientGatewayUrl')}>
                    <TextInput
                      size="xs"
                      value={baseUrl}
                      onChange={event => { setBaseUrl(event.currentTarget.value); setPreview(null) }}
                      placeholder={`http://127.0.0.1:${adminPort}`}
                      disabled={!canEdit}
                    />
                  </Field>
                  <Field label={t(locale, 'clientDefaultModel')}>
                    <Select
                      size="xs"
                      data={modelNames.map(model => ({ value: model, label: model }))}
                      value={defaultModel || null}
                      onChange={value => { setDefaultModel(value || ''); setPreview(null) }}
                      allowDeselect={false}
                      searchable
                      disabled={!canEdit}
                    />
                  </Field>
                  <div className="console-clients-url-hint">
                    {t(locale, 'clientGatewayHint')}
                  </div>
                  {selected.id === 'claude_code' && (
                    <>
                      <Field label={t(locale, 'clientSonnet')}>
                        <Select
                          size="xs"
                          data={modelNames.map(model => ({ value: model, label: model }))}
                          value={sonnet}
                          onChange={value => { setSonnet(value); setPreview(null) }}
                          placeholder={t(locale, 'clientFollowDefault')}
                          clearable
                          clearButtonProps={{ 'aria-label': t(locale, 'clientClearTier') }}
                          searchable
                          disabled={!canEdit}
                        />
                      </Field>
                      <Field label={t(locale, 'clientOpus')}>
                        <Select
                          size="xs"
                          data={modelNames.map(model => ({ value: model, label: model }))}
                          value={opus}
                          onChange={value => { setOpus(value); setPreview(null) }}
                          placeholder={t(locale, 'clientFollowDefault')}
                          clearable
                          clearButtonProps={{ 'aria-label': t(locale, 'clientClearTier') }}
                          searchable
                          disabled={!canEdit}
                        />
                      </Field>
                      <Field label={t(locale, 'clientHaiku')}>
                        <Select
                          size="xs"
                          data={modelNames.map(model => ({ value: model, label: model }))}
                          value={haiku}
                          onChange={value => { setHaiku(value); setPreview(null) }}
                          placeholder={t(locale, 'clientFollowDefault')}
                          clearable
                          clearButtonProps={{ 'aria-label': t(locale, 'clientClearTier') }}
                          searchable
                          disabled={!canEdit}
                        />
                      </Field>
                      <Field label={t(locale, 'clientFable')}>
                        <Select
                          size="xs"
                          data={modelNames.map(model => ({ value: model, label: model }))}
                          value={fable}
                          onChange={value => { setFable(value); setPreview(null) }}
                          placeholder={t(locale, 'clientFollowDefault')}
                          clearable
                          clearButtonProps={{ 'aria-label': t(locale, 'clientClearTier') }}
                          searchable
                          disabled={!canEdit}
                        />
                      </Field>
                      <Field label={t(locale, 'clientSubagent')}>
                        <Select
                          size="xs"
                          data={modelNames.map(model => ({ value: model, label: model }))}
                          value={subagent}
                          onChange={value => { setSubagent(value); setPreview(null) }}
                          placeholder={t(locale, 'clientFollowDefault')}
                          clearable
                          clearButtonProps={{ 'aria-label': t(locale, 'clientClearTier') }}
                          searchable
                          disabled={!canEdit}
                        />
                      </Field>
                      <div className="console-clients-url-hint">
                        {t(locale, 'clientTierHint')}
                      </div>
                    </>
                  )}
                </div>

                <div className="console-note">
                  {t(locale, 'clientKeyNote')}
                </div>

                <div className="console-clients-actions">
                  <Button
                    size="xs"
                    variant="default"
                    leftSection={<IconEye size={14} />}
                    loading={busy === 'preview'}
                    disabled={!canEdit || busy !== null}
                    onClick={() => void runPreview()}
                  >
                    {t(locale, 'clientPreview')}
                  </Button>
                  <Button
                    size="xs"
                    leftSection={<IconCheck size={14} />}
                    loading={busy === 'apply'}
                    disabled={!canEdit || busy !== null}
                    onClick={() => setConfirm('apply')}
                  >
                    {t(locale, 'clientApply')}
                  </Button>
                  <Button
                    size="xs"
                    color="red"
                    variant="default"
                    leftSection={<IconArrowBackUp size={14} />}
                    loading={busy === 'restore'}
                    disabled={!selected.managed || busy !== null}
                    onClick={() => setConfirm('restore')}
                  >
                    {t(locale, 'clientRestore')}
                  </Button>
                  <Button
                    size="xs"
                    color="red"
                    variant="subtle"
                    leftSection={<IconHistory size={14} />}
                    loading={busy === 'restore-backup'}
                    disabled={!selected.managed || !selected.backup_path || busy !== null}
                    onClick={() => setConfirm('restore-backup')}
                  >
                    {t(locale, 'clientRestoreBackup')}
                  </Button>
                </div>

                <div className="console-clients-backup">
                  <strong>{t(locale, 'clientBackup')}</strong>{' '}
                  <span className="console-mono">{selected.backup_path || t(locale, 'clientNoBackup')}</span>
                  {selected.backup_path && <span className="console-note-inline">{t(locale, 'clientBackupWarning')}</span>}
                </div>
              </>
            )}

            {preview && (
              <div className="console-clients-preview">
                <div className="console-panel-head">
                  <div className="console-panel-title">{t(locale, 'clientPreviewTitle')}</div>
                  <span className="console-panel-sub">
                    {preview.key_action === 'reuse' ? t(locale, 'clientKeyReuse') : t(locale, 'clientKeyCreate')}
                  </span>
                </div>
                {preview.warnings.length > 0 && (
                  <div className="console-clients-warnings">
                    {preview.warnings.map((warning, index) => (
                      <div key={`${warning}-${index}`} className="console-note warn">{warningText(warning, locale)}</div>
                    ))}
                  </div>
                )}
                {preview.changes.length === 0 ? (
                  <div className="console-note">{t(locale, 'clientNoChanges')}</div>
                ) : (
                  <div className="console-clients-changes-wrap">
                    <Table className="console-clients-changes">
                      <Table.Thead>
                        <Table.Tr>
                          <Table.Th>{t(locale, 'clientPath')}</Table.Th>
                          <Table.Th>{t(locale, 'clientBefore')}</Table.Th>
                          <Table.Th>{t(locale, 'clientAfter')}</Table.Th>
                        </Table.Tr>
                      </Table.Thead>
                      <Table.Tbody>
                        {preview.changes.map(change => (
                          <Table.Tr key={change.path}>
                            <Table.Td><span className="console-mono">{change.path}</span></Table.Td>
                            <Table.Td><span className="console-mono">{change.before || '—'}</span></Table.Td>
                            <Table.Td>
                              <span className={`console-mono${change.secret ? ' console-clients-secret' : ''}`}>
                                {change.secret
                                  ? t(locale, preview.key_action === 'reuse' ? 'clientSecretReuse' : 'clientSecretNew')
                                  : change.after}
                              </span>
                            </Table.Td>
                          </Table.Tr>
                        ))}
                      </Table.Tbody>
                    </Table>
                  </div>
                )}
                <div className="console-note">{t(locale, 'clientSecretPlaceholderNote')}</div>
                {preview.backup_path && (
                  <div className="console-clients-backup">
                    <strong>{t(locale, 'clientBackup')}</strong>{' '}
                    <span className="console-mono">{preview.backup_path}</span>
                  </div>
                )}
              </div>
            )}

            {notice && (
              <div className="console-clients-result">
                <div className="console-note">
                  <IconCheck size={14} stroke={1.8} /> {notice.text}
                </div>
                {notice.keyMask && (
                  <div className="console-clients-backup">
                    <strong>{t(locale, 'clientKeyMask')}</strong>{' '}
                    <span className="console-mono">{notice.keyMask}</span>
                  </div>
                )}
                {notice.backup && (
                  <div className="console-clients-backup">
                    <strong>{t(locale, 'clientBackup')}</strong>{' '}
                    <span className="console-mono">{notice.backup}</span>
                  </div>
                )}
                {notice.warnings.length > 0 && (
                  <div className="console-clients-warnings">
                    {notice.warnings.map((warning, index) => (
                      <div key={`${warning}-${index}`} className="console-note warn">{warningText(warning, locale)}</div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        </section>
      )}

      <Modal
        opened={confirm === 'apply'}
        closeButtonProps={{ 'aria-label': t(locale, 'close') }}
        onClose={() => setConfirm(null)}
        title={t(locale, 'clientApplyTitle', selected ? clientName(selected.id) : '')}
        centered
        closeOnClickOutside={false}
      >
        {selected && (
          <>
            <div className="console-note warn">{t(locale, 'clientApplyBody', selected.path)}</div>
            <div className="console-note">{t(locale, 'clientBackupWarning')}</div>
          </>
        )}
        <div className="console-inspector-actions" style={{ paddingTop: 16 }}>
          <Button size="xs" variant="default" onClick={() => setConfirm(null)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" leftSection={<IconCheck size={14} />} onClick={() => void runApply()}>
            {t(locale, 'clientConfirmApply')}
          </Button>
        </div>
      </Modal>

      <Modal
        opened={confirm === 'restore'}
        closeButtonProps={{ 'aria-label': t(locale, 'close') }}
        onClose={() => setConfirm(null)}
        title={t(locale, 'clientRestoreTitle', selected ? clientName(selected.id) : '')}
        centered
        closeOnClickOutside={false}
      >
        <div className="console-note warn">{t(locale, 'clientRestoreBody')}</div>
        <div className="console-inspector-actions" style={{ paddingTop: 16 }}>
          <Button size="xs" variant="default" onClick={() => setConfirm(null)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" leftSection={<IconArrowBackUp size={14} />} onClick={() => void runRestore()}>
            {t(locale, 'clientConfirmRestore')}
          </Button>
        </div>
      </Modal>

      <Modal
        opened={confirm === 'restore-backup'}
        closeButtonProps={{ 'aria-label': t(locale, 'close') }}
        onClose={() => setConfirm(null)}
        title={t(locale, 'clientRestoreBackupTitle')}
        centered
        closeOnClickOutside={false}
      >
        <div className="console-note warn">{t(locale, 'clientRestoreBackupBody')}</div>
        {selected?.backup_path && <div className="console-mono console-clients-path">{selected.backup_path}</div>}
        <div className="console-inspector-actions" style={{ paddingTop: 16 }}>
          <Button size="xs" variant="default" onClick={() => setConfirm(null)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" leftSection={<IconHistory size={14} />} onClick={() => void runRestoreBackup()}>
            {t(locale, 'clientConfirmBackup')}
          </Button>
        </div>
      </Modal>
    </div>
  )
}
