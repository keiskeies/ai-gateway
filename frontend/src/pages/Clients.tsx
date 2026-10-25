import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Button, Group, Modal, Select, TextInput } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import {
  IconCheck,
  IconChevronDown,
  IconChevronRight,
  IconHistory,
  IconRefresh,
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
import { ErrorState, PageSkeleton } from '../components/ui/States'
import { PageHeader } from '../components/ui/PageHeader'
import { useAppContext } from '../ThemeContext'
import { t, type Locale, type TranslationKey } from '../i18n'
import type {
  ClientApplyBody,
  ClientApplyResult,
  ClientChange,
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

/** 只有 Claude Code 需要重开终端才读配置，其余按「重启工具」说 */
const CLIENT_NEXT: Record<ClientId, TranslationKey> = {
  claude_code: 'clientNextClaude',
  codex: 'clientNextGeneric',
  opencode: 'clientNextGeneric',
  openclaw: 'clientNextGeneric',
  pi: 'clientNextGeneric',
  hermes: 'clientNextGeneric',
}

const CLAUDE_TIERS: [string, TranslationKey][] = [
  ['sonnet', 'clientSonnet'],
  ['opus', 'clientOpus'],
  ['haiku', 'clientHaiku'],
  ['fable', 'clientFable'],
  ['subagent', 'clientSubagent'],
]

type Busy = 'apply' | 'restore' | 'restore-backup' | null

interface ResultNotice {
  kind: 'apply' | 'restore' | 'restore-backup'
  text: string
  keyMask?: string
  warnings: string[]
}

/** 一条变更在界面上的归法：主行说人话，档位与协议细节折进「另外 N 项」 */
type ChangeKind = 'url' | 'key' | 'model' | 'provider' | 'tier' | 'detail'

const CHANGE_LABEL: Record<ChangeKind, TranslationKey> = {
  url: 'clientChangeUrl',
  key: 'clientChangeKey',
  model: 'clientChangeModel',
  provider: 'clientChangeProvider',
  tier: 'clientChangeTier',
  detail: 'clientChangeDetail',
}

function classifyChange(path: string): ChangeKind {
  const key = path.toUpperCase()
  if (/TOKEN|API_KEY|APIKEY|BEARER/.test(key)) return 'key'
  if (/BASE_URL|BASEURL/.test(key)) return 'url'
  if (/MODEL_PROVIDER/.test(key)) return 'provider'
  if (/SUBAGENT|DEFAULT_[A-Z]+_MODEL/.test(key)) return 'tier'
  if (/MODEL|PRIMARY/.test(key)) return 'model'
  return 'detail'
}

interface ChangeRow {
  kind: ChangeKind
  path: string
  before: string
  after: string
  secret: boolean
}

function splitChanges(changes: ClientChange[]) {
  // 值本来就等于目标的条目不显示：列出来只会让人以为要改（密钥行永远要写，保留）
  const rows: ChangeRow[] = changes
    .filter(change => change.secret || (change.before ?? '') !== change.after)
    .map(change => ({
      kind: classifyChange(change.path),
      path: change.path,
      before: change.before ?? '',
      after: change.after,
      secret: change.secret,
    }))
  // 同一个语义被写进多处（例如 model 与 model_provider 都指向网关）只留第一条
  const seen = new Set<ChangeKind>()
  const primary = rows.filter(row => {
    if (row.kind === 'tier' || row.kind === 'detail') return false
    if (seen.has(row.kind)) return false
    seen.add(row.kind)
    return true
  })
  const primarySet = new Set(primary)
  const extra = rows.filter(row => !primarySet.has(row))
  const shared = extra.length > 1 && new Set(extra.map(row => row.after)).size === 1 ? extra[0].after : null
  return { primary, extra, shared }
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
  { match: 'Codex is configured with wire_api', text: 'Codex 走 Chat 协议；它依赖 Responses 的专属能力可能不可用。' },
  { match: 'This requires a Codex version that reads experimental_bearer_token', text: '需要较新的 Codex 版本才会读取 experimental_bearer_token。' },
  { match: '~/.codex/auth.json was left untouched', text: '~/.codex/auth.json 未改动；官方登录凭据不会发给第三方。' },
  { match: 'OpenCode JSONC comments', text: 'OpenCode 配置里的注释和缩进会被规范化；原文件在备份里。' },
  { match: "OpenCode's newer `providers` block", text: '检测到 OpenCode 新版 providers 块；AI Gateway 写入的是 provider 块。' },
  { match: 'OpenClaw JSON5 comments', text: 'OpenClaw 配置里的注释和缩进会被规范化；原文件在备份里。' },
  { match: 'Pi JSON5 comments', text: 'Pi 配置里的注释和缩进会被规范化；原文件在备份里。' },
  { match: "Pi's active model is stored in settings.json", text: 'Pi 当前用哪个模型由它自己的 settings.json 决定。' },
  { match: 'Select the AI Gateway model inside Pi', text: '接入后请在 Pi 里选一次 AI Gateway 模型；这里不替它改默认模型。' },
  { match: 'Hermes YAML comments', text: 'Hermes 配置里的注释和缩进会被规范化；原文件在备份里。' },
  { match: 'The AI Gateway provider entry was left in place', text: 'AI Gateway 的 provider 条目已保留；要彻底清掉请用应用前备份。' },
  { match: 'The config file no longer exists', text: '配置文件已不存在，无需撤销。' },
  { match: 'Full file restore used the pre-apply backup', text: '已用应用前备份整份覆盖；应用之后做的修改已丢失。' },
  { match: 'Managed state is corrupt', text: '托管状态已损坏；请重新接入，或用应用前备份覆盖。' },
  { match: 'A managed client config is missing on disk', text: '记录里说已接入，但配置文件不在了；请重新接入或恢复备份。' },
  { match: 'The managed gateway key is disabled', text: '这条网关 Key 已被停用；重新接入会签发一条新的。' },
  { match: 'The managed gateway key no longer exists', text: '这条网关 Key 已不存在；重新接入会签发一条新的。' },
  { match: 'Existing provider.ai-gateway', text: '已有 provider.ai-gateway，已被替换；旧文件在备份里。' },
  { match: 'Existing models.providers.ai-gateway', text: '已有 models.providers.ai-gateway，已被替换；旧文件在备份里。' },
  { match: 'Existing providers.ai-gateway', text: '已有 providers.ai-gateway，已被替换；旧文件在备份里。' },
  { match: 'Existing custom_providers.ai-gateway', text: '已有 custom_providers.ai-gateway，已被替换；旧文件在备份里。' },
  { match: 'provider.ai-gateway already exists', text: 'provider.ai-gateway 已存在，且不是 AI Gateway 写的；请先手动处理。' },
  { match: 'models.providers.ai-gateway already exists', text: 'models.providers.ai-gateway 已存在，且不是 AI Gateway 写的；请先手动处理。' },
  { match: 'providers.ai-gateway already exists', text: 'providers.ai-gateway 已存在，且不是 AI Gateway 写的；请先手动处理。' },
  { match: 'custom_providers.ai-gateway already exists', text: 'custom_providers.ai-gateway 已存在，且不是 AI Gateway 写的；请先手动处理。' },
]

function warningText(warning: string, locale: Locale): string {
  if (locale !== 'zh') return warning
  const previousKey = warning.match(/^Previous gateway key '(.+)' was disabled\.$/)
  if (previousKey) return `上一条网关 Key「${previousKey[1]}」已停用。`
  const hit = WARNING_ZH.find(item => warning.includes(item.match))
  return hit ? hit.text : warning
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
  const gatewayUrl = `http://127.0.0.1:${adminPort}`

  const [expanded, setExpanded] = useState<ClientId | null>(null)
  const [showMore, setShowMore] = useState(false)
  const [showExtra, setShowExtra] = useState(false)
  const [baseUrl, setBaseUrl] = useState(gatewayUrl)
  const [defaultModel, setDefaultModel] = useState('')
  const [tiers, setTiers] = useState<Record<string, string | null>>({})
  const [preview, setPreview] = useState<ClientPreview | null>(null)
  const [computing, setComputing] = useState(false)
  const [busy, setBusy] = useState<Busy>(null)
  const [confirmKind, setConfirmKind] = useState<'restore' | 'restore-backup' | null>(null)
  const [notice, setNotice] = useState<ResultNotice | null>(null)
  const previewSeq = useRef(0)

  const active = clients.find(client => client.id === expanded) ?? null

  useEffect(() => {
    if (!modelNames.length) return
    setDefaultModel(previous => (previous && modelNames.includes(previous) ? previous : modelNames[0]))
  }, [modelNames])

  useEffect(() => {
    setBaseUrl(gatewayUrl)
  }, [gatewayUrl])

  const showError = useCallback(
    (error: unknown) => {
      notifications.show({
        title: t(locale, 'clientOperationFailed'),
        message: errorMessage(error, locale),
        color: 'red',
        autoClose: 8000,
        withCloseButton: false,
      })
    },
    [locale],
  )

  const refresh = useCallback(async () => {
    await clientsQuery.refetch()
  }, [clientsQuery])

  const body = (): ClientApplyBody => ({
    base_url: baseUrl.trim() || undefined,
    default_model: defaultModel,
    ...(expanded === 'claude_code'
      ? {
          sonnet_model: tiers.sonnet || undefined,
          opus_model: tiers.opus || undefined,
          haiku_model: tiers.haiku || undefined,
          fable_model: tiers.fable || undefined,
          subagent_model: tiers.subagent || undefined,
        }
      : {}),
  })

  // 展开即算 diff：不必先点「预览」才能看见要改什么
  useEffect(() => {
    const seq = ++previewSeq.current
    if (!active || active.error || !defaultModel) {
      setPreview(null)
      setComputing(false)
      return
    }
    setComputing(true)
    const timer = setTimeout(() => {
      previewClient(active.id, body())
        .then(result => {
          if (seq !== previewSeq.current) return
          setPreview(result)
          setComputing(false)
        })
        .catch(error => {
          if (seq !== previewSeq.current) return
          showError(error)
          setComputing(false)
        })
    }, 300)
    return () => clearTimeout(timer)
  }, [expanded, baseUrl, defaultModel, tiers, modelNames, active])

  const openCard = (id: ClientId) => {
    setNotice(null)
    setShowExtra(false)
    setShowMore(false)
    setExpanded(current => (current === id ? null : id))
  }

  const runApply = async () => {
    if (!active || !defaultModel) return
    const id = active.id
    setBusy('apply')
    try {
      const result: ClientApplyResult = await applyClient(id, body())
      setNotice({
        kind: 'apply',
        text: t(locale, 'clientAppliedShort', clientName(id)),
        keyMask: result.key_mask,
        warnings: result.warnings,
      })
      setPreview(null)
      await refresh()
    } catch (error) {
      showError(error)
    } finally {
      setBusy(null)
    }
  }

  const runRestore = async (fullBackup: boolean) => {
    if (!active) return
    const id = active.id
    setConfirmKind(null)
    setBusy(fullBackup ? 'restore-backup' : 'restore')
    try {
      const result: ClientRestoreResult = fullBackup
        ? await restoreClientBackup(id)
        : await restoreClient(id)
      setNotice({
        kind: fullBackup ? 'restore-backup' : 'restore',
        text: fullBackup ? t(locale, 'clientRestoredBackup') : t(locale, 'clientRestored'),
        warnings: result.warnings,
      })
      setPreview(null)
      await refresh()
    } catch (error) {
      showError(error)
    } finally {
      setBusy(null)
    }
  }

  if (clientsQuery.isPending) return <PageSkeleton rows={6} />
  if (clientsQuery.isError) return <ErrorState onRetry={() => void clientsQuery.refetch()} />

  const present = clients.filter(client => client.detected || client.managed || client.error)
  const absent = clients.filter(client => !client.detected && !client.managed && !client.error)
  const noModels = modelNames.length === 0

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

      {noModels && (
        <div className="console-note warn" style={{ marginTop: 0 }}>
          {t(locale, 'clientNoModelsBanner')}{' '}
          <Button size="compact-xs" variant="light" mt={4} onClick={() => navigate('/models')}>
            {t(locale, 'clientGoModels')}
          </Button>
        </div>
      )}

      <section className="console-panel console-clients">
        {present.length === 0 && (
          <div className="console-clients-empty">{t(locale, 'clientNoneHere')}</div>
        )}
        {present.map(client => {
          const open = expanded === client.id
          const needs = [client.error, ...client.warnings].filter(Boolean) as string[]
          const diff = preview ? splitChanges(preview.changes) : null
          return (
            <article key={client.id} className={`console-client${open ? ' open' : ''}${client.managed ? ' managed' : ''}${client.error ? ' err' : ''}`}>
              <div className="console-client-head">
                <span className="console-client-dot" aria-hidden="true" />
                <div className="console-client-id">
                  <div className="console-client-name">{clientName(client.id)}</div>
                  <div className="console-client-state">
                    {client.error
                      ? t(locale, 'clientCardBroken')
                      : client.managed
                        ? t(locale, 'clientCardOn')
                        : client.current.base_url
                          ? t(locale, 'clientCardPointsTo', client.current.base_url)
                          : t(locale, 'clientCardUnwired')}
                    {client.current.model ? ` · ${t(locale, 'clientCardModel', client.current.model)}` : ''}
                  </div>
                </div>
                <div className="console-client-file" title={client.path}>{shortPath(client.path)}</div>
                <Button
                  size="xs"
                  variant={open ? 'subtle' : client.managed ? 'default' : 'light'}
                  rightSection={open ? <IconChevronDown size={14} /> : <IconChevronRight size={14} />}
                  disabled={busy !== null || noModels}
                  aria-expanded={open}
                  onClick={() => openCard(client.id)}
                >
                  {open
                    ? t(locale, 'clientCollapse')
                    : client.managed
                      ? t(locale, 'clientManage')
                      : t(locale, 'clientTakeover')}
                </Button>
              </div>

              {open && (
                <div className="console-client-body">
                  <p className="console-client-hint">{t(locale, CLIENT_HINT[client.id])}</p>

                  {needs.length > 0 && (
                    <ul className="console-client-needs">
                      {needs.map(item => <li key={item}>{warningText(item, locale)}</li>)}
                    </ul>
                  )}

                  {client.error ? (
                    <Button size="xs" variant="default" onClick={() => void refresh()}>
                      {t(locale, 'clientRefresh')}
                    </Button>
                  ) : (
                    <>
                      <div className="console-client-row">
                        <Field label={t(locale, 'clientDefaultModel')}>
                          <Select
                            size="xs"
                            data={modelNames.map(model => ({ value: model, label: model }))}
                            value={defaultModel || null}
                            onChange={value => setDefaultModel(value || '')}
                            allowDeselect={false}
                            searchable
                            disabled={busy !== null}
                          />
                        </Field>
                        <button
                          type="button"
                          className="console-client-disclose"
                          aria-expanded={showMore}
                          onClick={() => setShowMore(value => !value)}
                        >
                          {showMore ? <IconChevronDown size={13} /> : <IconChevronRight size={13} />}
                          {t(locale, 'clientMoreSettings')}
                        </button>
                      </div>

                      {showMore && (
                        <div className="console-client-more">
                          <Field full label={t(locale, 'clientGatewayUrl')}>
                            <TextInput
                              size="xs"
                              value={baseUrl}
                              onChange={event => setBaseUrl(event.currentTarget.value)}
                              placeholder={gatewayUrl}
                              disabled={busy !== null}
                            />
                          </Field>
                          <div className="console-client-tier-note">{t(locale, 'clientGatewayUrlDesc')}</div>
                          {client.id === 'claude_code' && (
                            <div className="console-client-tiers">
                              {CLAUDE_TIERS.map(([field, label]) => (
                                <Field key={field} label={t(locale, label)}>
                                  <Select
                                    size="xs"
                                    data={modelNames.map(model => ({ value: model, label: model }))}
                                    value={tiers[field] ?? null}
                                    onChange={value => setTiers(current => ({ ...current, [field]: value }))}
                                    placeholder={t(locale, 'clientFollowDefault')}
                                    clearable
                                    clearButtonProps={{ 'aria-label': t(locale, 'clientClearTier') }}
                                    searchable
                                    disabled={busy !== null}
                                  />
                                </Field>
                              ))}
                              <div className="console-client-tier-note">{t(locale, 'clientTierNote')}</div>
                            </div>
                          )}
                        </div>
                      )}

                      <ClientDiff
                        diff={diff}
                        preview={preview}
                        computing={computing}
                        showExtra={showExtra}
                        onToggleExtra={() => setShowExtra(value => !value)}
                        locale={locale}
                      />

                      <div className="console-client-actions">
                        <Button
                          size="xs"
                          leftSection={<IconCheck size={14} />}
                          loading={busy === 'apply'}
                          disabled={busy !== null || !defaultModel || computing || !diff || (diff.primary.length === 0 && diff.extra.length === 0)}
                          onClick={() => void runApply()}
                        >
                          {client.managed ? t(locale, 'clientSave') : t(locale, 'clientTakeoverApply')}
                        </Button>
                      </div>
                      <div className="console-client-foot">
                        {client.backup_path
                          ? t(locale, 'clientBackupFootHave', shortPath(client.backup_path))
                          : t(locale, 'clientBackupFootNew')}
                      </div>

                      {notice && (
                        <div className={`console-client-result ${notice.kind}`}>
                          <div className="console-client-result-text">
                            <IconCheck size={14} stroke={1.9} /> {notice.text}
                          </div>
                          {notice.warnings.length > 0 && (
                            <ul className="console-client-needs">
                              {notice.warnings.map(item => <li key={item}>{warningText(item, locale)}</li>)}
                            </ul>
                          )}
                          {notice.kind === 'apply' && (
                            <div className="console-client-next">
                              <div className="console-client-next-title">{t(locale, 'clientNextTitle')}</div>
                              <p>{t(locale, CLIENT_NEXT[client.id])}</p>
                              <Button size="compact-xs" variant="light" onClick={() => navigate('/traffic')}>
                                {t(locale, 'clientGoTraffic')}
                              </Button>
                            </div>
                          )}
                          {notice.keyMask && (
                            <div className="console-client-keymask">
                              {t(locale, 'clientKeyMask')} <span className="console-mono">{notice.keyMask}</span>
                              <span className="console-note-inline">{t(locale, 'clientKeyMaskNote')}</span>
                            </div>
                          )}
                        </div>
                      )}

                      {client.managed && (
                        <div className="console-client-secondary">
                          <div className="console-client-secondary-title">{t(locale, 'clientSecondaryTitle')}</div>
                          <div className="console-client-secondary-row">
                            <div>
                              <div>{t(locale, 'clientUndo')}</div>
                              <div className="console-client-secondary-desc">{t(locale, 'clientUndoDesc')}</div>
                            </div>
                            <Button
                              size="compact-xs"
                              variant="default"
                              loading={busy === 'restore'}
                              disabled={busy !== null}
                              onClick={() => setConfirmKind('restore')}
                            >
                              {t(locale, 'clientUndoAction')}
                            </Button>
                          </div>
                          {client.backup_path && (
                            <div className="console-client-secondary-row">
                              <div>
                                <div>{t(locale, 'clientBackupOver')}</div>
                                <div className="console-client-secondary-desc">{t(locale, 'clientBackupOverDesc')}</div>
                              </div>
                              <Button
                                size="compact-xs"
                                variant="default"
                                color="red"
                                leftSection={<IconHistory size={13} />}
                                loading={busy === 'restore-backup'}
                                disabled={busy !== null}
                                onClick={() => setConfirmKind('restore-backup')}
                              >
                                {t(locale, 'clientBackupOverAction')}
                              </Button>
                            </div>
                          )}
                        </div>
                      )}
                    </>
                  )}
                </div>
              )}
            </article>
          )
        })}
      </section>

      {absent.length > 0 && (
        <div className="console-clients-absent">
          <div>{t(locale, 'clientAbsentTitle', String(absent.length))}</div>
          <div className="console-clients-absent-list">
            {absent.map(client => (
              <span key={client.id} title={client.path}>{clientName(client.id)}</span>
            ))}
          </div>
          <div className="console-clients-absent-note">{t(locale, 'clientAbsentNote')}</div>
        </div>
      )}

      <Modal
        opened={confirmKind === 'restore'}
        closeButtonProps={{ 'aria-label': t(locale, 'close') }}
        onClose={() => setConfirmKind(null)}
        title={t(locale, 'clientUndo')}
        centered
        closeOnClickOutside={false}
      >
        <div className="console-note" style={{ marginTop: 0 }}>{t(locale, 'clientUndoConfirm')}</div>
        <Group justify="flex-end" mt="md">
          <Button size="xs" variant="default" onClick={() => setConfirmKind(null)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" loading={busy !== null} onClick={() => void runRestore(false)}>
            {t(locale, 'clientUndoAction')}
          </Button>
        </Group>
      </Modal>

      <Modal
        opened={confirmKind === 'restore-backup'}
        closeButtonProps={{ 'aria-label': t(locale, 'close') }}
        onClose={() => setConfirmKind(null)}
        title={t(locale, 'clientBackupOver')}
        centered
        closeOnClickOutside={false}
      >
        <div className="console-note warn" style={{ marginTop: 0 }}>{t(locale, 'clientBackupOverConfirm')}</div>
        <Group justify="flex-end" mt="md">
          <Button size="xs" variant="default" onClick={() => setConfirmKind(null)}>{t(locale, 'cancel')}</Button>
          <Button size="xs" color="red" loading={busy !== null} onClick={() => void runRestore(true)}>
            {t(locale, 'clientBackupOverAction')}
          </Button>
        </Group>
      </Modal>
    </div>
  )
}

function ClientDiff({
  diff,
  preview,
  computing,
  showExtra,
  onToggleExtra,
  locale,
}: {
  diff: ReturnType<typeof splitChanges> | null
  preview: ClientPreview | null
  computing: boolean
  showExtra: boolean
  onToggleExtra: () => void
  locale: Locale
}) {
  const changes = diff
  return (
    <div className="console-client-changes">
      <div className="console-client-changes-head">
        <span>{t(locale, 'clientChangesTitle')}</span>
        {computing && <span className="console-client-computing">{t(locale, 'clientComputing')}</span>}
      </div>
      {!changes ? (
        <div className="console-client-same">
          {t(locale, computing ? 'clientComputing' : 'clientDiffFailed')}
        </div>
      ) : changes.primary.length === 0 && changes.extra.length === 0 ? (
        <div className="console-client-same">{t(locale, 'clientNothingToChange')}</div>
      ) : (
        <>
          <ul className="console-client-diff">
            {changes.primary.map(row => (
              <li key={row.path}>
                <span className="console-client-diff-label">{t(locale, CHANGE_LABEL[row.kind])}</span>
                <span className="console-client-diff-value">
                  {row.before && !row.secret && <span className="console-client-diff-before">{row.before}</span>}
                  {row.secret
                    ? t(locale, preview?.key_action === 'reuse' ? 'clientSecretReuse' : 'clientSecretNew')
                    : row.after}
                </span>
              </li>
            ))}
          </ul>
          {changes.extra.length > 0 && (
            changes.shared !== null ? (
              // 档位与协议细节都指向同一个值：一行说完，不刷 9 行
              <ul className="console-client-diff muted">
                <li>
                  <span className="console-client-diff-label">
                    {t(locale, 'clientExtraAllFollow', String(changes.extra.length))}
                  </span>
                  <span className="console-client-diff-value console-mono">{changes.shared}</span>
                </li>
              </ul>
            ) : (
              <>
                <button type="button" className="console-client-disclose" aria-expanded={showExtra} onClick={onToggleExtra}>
                  {showExtra ? <IconChevronDown size={13} /> : <IconChevronRight size={13} />}
                  {t(locale, 'clientExtraCount', String(changes.extra.length))}
                </button>
                {showExtra && (
                  <ul className="console-client-diff muted">
                    {changes.extra.map(row => (
                      <li key={row.path}>
                        <span className="console-client-diff-label console-mono">{row.path}</span>
                        <span className="console-client-diff-value console-mono">
                          {row.secret ? t(locale, 'clientSecretNew') : row.after}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </>
            )
          )}
        </>
      )}
    </div>
  )
}
