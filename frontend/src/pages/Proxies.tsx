import { useEffect, useState, useCallback } from 'react'
import {
  Table, Modal, Select, TextInput, NumberInput, MultiSelect, Button, Group, Stack,
  Text, Badge, ActionIcon, Card, Grid, Divider, Tooltip, Code, CopyButton, Tabs,
  Combobox, Input, InputBase, useCombobox, Textarea,
} from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import { notifications } from '@mantine/notifications'
import {
  IconPlus, IconTrash, IconCode, IconSettings, IconCopy, IconLoader2, IconRefresh, IconApi, IconCheck,
} from '@tabler/icons-react'
import {
  listProxies, createProxy, updateProxy, deleteProxy,
  listRoutes, createRoute, deleteRoute, deleteBackend,
  listPlatforms, fetchRemoteModels, getSettings, listApiKeys as fetchApiKeys,
  proxyBaseURL,
} from '../api'
import { useAppContext } from '../ThemeContext'
import { t, type Locale, type TranslationKey } from '../i18n'
import { getPresetName, platformPresets, getModelsForPlatform, CAPABILITY_OPTIONS } from '../presets'

// ── Types ──────────────────────────────────────────────────────────

interface BackendRow {
  platform_id: string
  model_id: string
  weight: number
  priority: number
  capabilities: string[]
  /** 未知参数策略：Auto=按平台自动 / Drop=丢弃 / Passthrough=透传 */
  unknown_params: string
  /** 参数覆盖 JSON（编辑框内为字符串，提交时 parse 校验） */
  param_overrides: string
}

interface RemoteModelsCache {
  [platformId: string]: { id: string; owned_by?: string }[]
}

interface FetchingMap {
  [platformId: string]: boolean
}

// ── Model Select with custom input support ─────────────────────────

function ModelSelect({
  value,
  onChange,
  data,
  placeholder,
  disabled,
  fetching,
  label,
}: {
  value: string
  onChange: (val: string) => void
  data: { value: string; label: string }[]
  placeholder: string
  disabled?: boolean
  fetching?: boolean
  label?: string
}) {
  const combobox = useCombobox({
    onDropdownClose: () => combobox.resetSelectedOption(),
  })

  const [search, setSearch] = useState(value)

  useEffect(() => {
    setSearch(value)
  }, [value])

  const filteredOptions = data.filter(item =>
    item.label.toLowerCase().includes((search || '').toLowerCase()) ||
    item.value.toLowerCase().includes((search || '').toLowerCase())
  )

  const exactMatch = data.some(item => item.value === search)

  const options = filteredOptions.map(item => (
    <Combobox.Option key={item.value} value={item.value}>
      {item.label}
    </Combobox.Option>
  ))

  return (
    <Combobox
      store={combobox}
      onOptionSubmit={(val) => {
        onChange(val)
        setSearch(val)
        combobox.closeDropdown()
      }}
    >
      <Combobox.Target>
        <InputBase
          size="sm"
          label={label}
          rightSection={fetching ? <IconLoader2 size={14} className="animate-spin" /> : <Combobox.Chevron />}
          value={search}
          onChange={(event) => {
            setSearch(event.currentTarget.value)
            onChange(event.currentTarget.value)
            combobox.openDropdown()
          }}
          onClick={() => combobox.openDropdown()}
          onFocus={() => combobox.openDropdown()}
          onBlur={() => {
            combobox.closeDropdown()
            setSearch(value)
          }}
          placeholder={placeholder}
          disabled={disabled}
        />
      </Combobox.Target>

      <Combobox.Dropdown>
        <Combobox.Options>
          {options}
          {!exactMatch && search && search.trim() !== '' && (
            <Combobox.Option value={search}>
              {search}
            </Combobox.Option>
          )}
          {filteredOptions.length === 0 && !search && (
            <Combobox.Empty>No options</Combobox.Empty>
          )}
        </Combobox.Options>
      </Combobox.Dropdown>
    </Combobox>
  )
}

// ── LB Options ─────────────────────────────────────────────────────

function getLbOptions(locale: Locale) {
  return [
    { value: 'RoundRobin', label: t(locale, 'roundRobin') },
    { value: 'WeightedRandom', label: t(locale, 'weightedRandom') },
    { value: 'LeastConnections', label: t(locale, 'leastConnections') },
    { value: 'Priority', label: t(locale, 'priorityMode') },
    { value: 'LatencyBased', label: t(locale, 'latencyBased') },
  ]
}

// ── Component ──────────────────────────────────────────────────────

export default function Proxies() {
  const { locale } = useAppContext()

  // Data state
  const [proxies, setProxies] = useState<any[]>([])
  const [proxyRoutes, setProxyRoutes] = useState<Record<string, any>>({})
  const [platforms, setPlatforms] = useState<any[]>([])
  const [loading, setLoading] = useState(false)
  const [adminPort, setAdminPort] = useState<number>(1994)
  const [apiKeysList, setApiKeysList] = useState<any[]>([])

  // Create/Edit modal
  const [createOpen, { open: openCreate, close: closeCreate }] = useDisclosure(false)
  const [editingProxy, setEditingProxy] = useState<any>(null)
  const [formName, setFormName] = useState('')
  const [formLbStrategy, setFormLbStrategy] = useState('RoundRobin')
  // 客户端未传 max_tokens 时填充的默认值（空 = 不干预）
  const [formDefaultMaxTokens, setFormDefaultMaxTokens] = useState<number | ''>('')
  const [formBackends, setFormBackends] = useState<BackendRow[]>([
    { platform_id: '', model_id: '', weight: 1, priority: 0, capabilities: [], unknown_params: 'Auto', param_overrides: '' },
  ])
  const [submitting, setSubmitting] = useState(false)

  // Remote models per backend row (keyed by platform_id)
  const [remoteModelsCache, setRemoteModelsCache] = useState<RemoteModelsCache>({})
  const [fetchingMap, setFetchingMap] = useState<FetchingMap>({})

  // Usage modal
  const [usageOpen, { open: openUsage, close: closeUsage }] = useDisclosure(false)
  const [usageProxy, setUsageProxy] = useState<any>(null)

  // Delete confirm modal
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null)
  const [deleteOpen, { open: openDelete, close: closeDelete }] = useDisclosure(false)

  // ── Data Loading ──────────────────────────────────────────────────

  useEffect(() => { loadAll(); loadAdminPort() }, [])

  const loadAdminPort = async () => {
    try {
      const settings = await getSettings()
      setAdminPort(settings.admin_port)
    } catch { /* use default */ }
  }

  const loadAll = async () => {
    setLoading(true)
    try {
      const [p, pl, ak] = await Promise.all([
        listProxies(),
        listPlatforms(),
        fetchApiKeys().catch(() => []),
      ])
      setProxies(p)
      setPlatforms(pl)
      setApiKeysList(ak)

      const routeMap: Record<string, any> = {}
      await Promise.all(p.map(async (proxy: any) => {
        try {
          const routes = await listRoutes(proxy.id)
          if (routes.length > 0) routeMap[proxy.id] = routes[0]
        } catch { /* skip */ }
      }))
      setProxyRoutes(routeMap)
    } catch { /* silent */ }
    setLoading(false)
  }

  // ── Helpers ───────────────────────────────────────────────────────

  const getPlatformDisplayName = useCallback((id: string) => {
    const plat = platforms.find((p: any) => p.id === id)
    if (!plat) return id
    const preset = platformPresets.find(p => p.name === plat.name)
    return preset ? getPresetName(preset, locale) : plat.name
  }, [platforms, locale])

  const getPlatformName = useCallback((id: string) => {
    const plat = platforms.find((p: any) => p.id === id)
    return plat?.name || ''
  }, [platforms])

  const getPresetCapabilities = (platformName: string, modelId: string): string[] => {
    const presetModels = getModelsForPlatform(platformName)
    const preset = presetModels.find(m => m.model_id === modelId)
    return preset?.capabilities || []
  }

  const fetchRemote = async (platformId: string): Promise<{ id: string; owned_by?: string }[]> => {
    if (!platformId) return []
    try {
      const data = await fetchRemoteModels(platformId)
      return data.models || []
    } catch {
      notifications.show({
        title: t(locale, 'fetchRemoteModelsFailed'),
        message: t(locale, 'fetchRemoteModelsFailed'),
        color: 'red',
      })
      return []
    }
  }

  const getModelOptions = useCallback((platformId: string) => {
    const pName = getPlatformName(platformId)
    const remote = remoteModelsCache[platformId] || []
    const presetMs = getModelsForPlatform(pName)
    const remoteIds = new Set(remote.map(m => m.id))
    const presetOnly = presetMs.filter(m => !remoteIds.has(m.model_id))

    const remoteOpts = remote.map(m => {
      const pm = presetMs.find(p => p.model_id === m.id)
      const display = pm
        ? `${locale === 'zh' ? pm.display_name_zh : pm.display_name} (${m.id})`
        : m.id
      return { value: m.id, label: display }
    })
    const presetOpts = presetOnly.map(m => ({
      value: m.model_id,
      label: `${locale === 'zh' ? m.display_name_zh : m.display_name} (${m.model_id})`,
    }))
    return [...remoteOpts, ...presetOpts]
  }, [getPlatformName, remoteModelsCache, locale])

  const capabilityOptions = CAPABILITY_OPTIONS.map(c => ({
    value: c.value,
    label: locale === 'zh' ? c.labelZh : c.labelEn,
  }))

  // ── Form Reset ────────────────────────────────────────────────────

  const resetForm = () => {
    setFormName('')
    setFormLbStrategy('RoundRobin')
    setFormDefaultMaxTokens('')
    setFormBackends([{ platform_id: '', model_id: '', weight: 1, priority: 0, capabilities: [], unknown_params: 'Auto', param_overrides: '' }])
    setEditingProxy(null)
    setRemoteModelsCache({})
    setFetchingMap({})
  }

  // ── Open Create Modal ─────────────────────────────────────────────

  const handleOpenCreate = () => {
    resetForm()
    openCreate()
  }

  // ── Open Edit Modal ───────────────────────────────────────────────

  const handleOpenEdit = async (proxy: any) => {
    setEditingProxy(proxy)
    setFormName(proxy.name)

    // Open modal immediately so user sees feedback
    openCreate()

    let route: any = null
    try {
      const routes = await listRoutes(proxy.id)
      route = routes.length > 0 ? routes[0] : null
    } catch { /* skip */ }

    setFormLbStrategy(route?.lb_strategy || 'RoundRobin')
    setFormDefaultMaxTokens(route?.default_max_tokens ?? '')

    if (route?.backends?.length) {
      const backends: BackendRow[] = route.backends.map((b: any) => ({
        platform_id: b.platform_id || '',
        model_id: b.model_id || '',
        weight: b.weight ?? 1,
        priority: b.priority ?? 0,
        capabilities: b.capabilities || [],
        unknown_params: b.unknown_params || 'Auto',
        param_overrides: b.param_overrides && Object.keys(b.param_overrides).length > 0 ? JSON.stringify(b.param_overrides, null, 2) : '',
      }))
      setFormBackends(backends)

      // Prefetch remote models in the background (don't block modal)
      const newFetching: FetchingMap = {}
      for (const b of backends) {
        if (b.platform_id && !remoteModelsCache[b.platform_id]) {
          newFetching[b.platform_id] = true
        }
      }
      setFetchingMap(newFetching)

      for (const b of backends) {
        if (b.platform_id && !remoteModelsCache[b.platform_id]) {
          fetchRemote(b.platform_id).then(models => {
            setRemoteModelsCache(prev => ({ ...prev, [b.platform_id]: models }))
            setFetchingMap(prev => ({ ...prev, [b.platform_id]: false }))
          }).catch(() => {
            setFetchingMap(prev => ({ ...prev, [b.platform_id]: false }))
          })
        }
      }
    } else {
      setFormBackends([{ platform_id: '', model_id: '', weight: 1, priority: 0, capabilities: [], unknown_params: 'Auto', param_overrides: '' }])
    }
  }

  // ── Handle Platform Change ────────────────────────────────────────

  const handlePlatformChange = async (index: number, platformId: string) => {
    const newBackends = [...formBackends]
    newBackends[index] = {
      ...newBackends[index],
      platform_id: platformId,
      model_id: '',
      capabilities: [],
    }
    setFormBackends(newBackends)

    if (platformId && !remoteModelsCache[platformId]) {
      setFetchingMap(prev => ({ ...prev, [platformId]: true }))
      const models = await fetchRemote(platformId)
      setRemoteModelsCache(prev => ({ ...prev, [platformId]: models }))
      setFetchingMap(prev => ({ ...prev, [platformId]: false }))
    }
  }

  // ── Handle Model Change ───────────────────────────────────────────

  const handleModelChange = (index: number, modelId: string) => {
    const newBackends = [...formBackends]
    newBackends[index] = { ...newBackends[index], model_id: modelId }
    setFormBackends(newBackends)

    // Auto-fill capabilities from preset
    if (modelId) {
      const pid = newBackends[index].platform_id
      const pName = getPlatformName(pid)
      const caps = getPresetCapabilities(pName, modelId)
      if (caps.length > 0) {
        newBackends[index] = { ...newBackends[index], capabilities: caps }
        setFormBackends([...newBackends])
      }
    }
  }

  // ── Backend Row Mutations ─────────────────────────────────────────

  const addBackendRow = () => {
    setFormBackends(prev => [
      ...prev,
      { platform_id: '', model_id: '', weight: 1, priority: 0, capabilities: [], unknown_params: 'Auto', param_overrides: '' },
    ])
  }

  const removeBackendRow = (index: number) => {
    setFormBackends(prev => prev.filter((_, i) => i !== index))
  }

  const updateBackendField = <K extends keyof BackendRow>(index: number, field: K, value: BackendRow[K]) => {
    setFormBackends(prev => {
      const next = [...prev]
      next[index] = { ...next[index], [field]: value }
      return next
    })
  }

  // ── Submit ────────────────────────────────────────────────────────

  const handleSubmit = async () => {
    if (!formName.trim()) {
      notifications.show({ title: t(locale, 'createFailed'), message: t(locale, 'proxyName'), color: 'red' })
      return
    }
    // 参数覆盖 JSON 校验：空串 = 不覆盖，否则必须是合法 JSON object
    const parsedOverrides: Record<string, any>[] = []
    for (const b of formBackends) {
      const text = (b.param_overrides || '').trim()
      if (!text) { parsedOverrides.push({}); continue }
      try {
        const obj = JSON.parse(text)
        if (typeof obj !== 'object' || Array.isArray(obj) || obj === null) throw new Error('not an object')
        parsedOverrides.push(obj)
      } catch {
        notifications.show({ title: t(locale, 'createFailed'), message: `${t(locale, 'paramOverridesInvalid')}: ${b.model_id || t(locale, 'selectModel')}`, color: 'red' })
        return
      }
    }
    const backends = formBackends
      .map((b, i) => ({
        platform_id: b.platform_id,
        model_id: b.model_id,
        weight: b.weight,
        priority: b.priority,
        capabilities: b.capabilities,
        unknown_params: b.unknown_params || 'Auto',
        param_overrides: parsedOverrides[i] ?? {},
      }))
      .filter(b => b.platform_id && b.model_id)

    setSubmitting(true)
    try {
      if (editingProxy) {
        await updateProxy(editingProxy.id, { name: formName.trim() })
        const routes = await listRoutes(editingProxy.id).catch(() => [])
        for (const r of routes) {
          for (const b of (r.backends || [])) {
            await deleteBackend(b.id).catch(() => {})
          }
          await deleteRoute(r.id).catch(() => {})
        }
        if (backends.length > 0) {
          await createRoute(editingProxy.id, {
            lb_strategy: formLbStrategy || 'RoundRobin',
            default_max_tokens: formDefaultMaxTokens === '' ? null : formDefaultMaxTokens,
            backends,
          })
        }
        notifications.show({ title: t(locale, 'updateSuccess'), message: t(locale, 'updateSuccess'), color: 'green' })
      } else {
        const proxy = await createProxy({ name: formName.trim() })
        if (backends.length > 0) {
          await createRoute(proxy.id, {
            lb_strategy: formLbStrategy || 'RoundRobin',
            default_max_tokens: formDefaultMaxTokens === '' ? null : formDefaultMaxTokens,
            backends,
          })
        }
        notifications.show({ title: t(locale, 'createSuccess'), message: t(locale, 'createSuccess'), color: 'green' })
      }
      closeCreate()
      resetForm()
      loadAll()
    } catch {
      notifications.show({
        title: editingProxy ? t(locale, 'updateFailed') : t(locale, 'createFailed'),
        message: editingProxy ? t(locale, 'updateFailed') : t(locale, 'createFailed'),
        color: 'red',
      })
    }
    setSubmitting(false)
  }

  // ── Delete ────────────────────────────────────────────────────────

  const handleDelete = async () => {
    if (!deleteTarget) return
    try {
      await deleteProxy(deleteTarget)
      notifications.show({ title: t(locale, 'deleteSuccess'), message: t(locale, 'deleteSuccess'), color: 'green' })
      loadAll()
    } catch { /* silent */ }
    setDeleteTarget(null)
    closeDelete()
  }

  const confirmDelete = (id: string) => {
    setDeleteTarget(id)
    openDelete()
  }

  // ── Usage Modal ───────────────────────────────────────────────────

  const handleOpenUsage = (proxy: any) => {
    setUsageProxy(proxy)
    openUsage()
  }

  const getUsageSnippets = (proxy: any) => {
    const baseUrl = `http://localhost:${adminPort}`
    const relevantKey = apiKeysList.find((k: any) => !k.proxy_id) || apiKeysList.find((k: any) => k.proxy_id === proxy.id)
    const token = relevantKey?.key || ''
    const modelName = proxy.name

    const curlOpenai = `curl ${baseUrl}/v1/chat/completions \\
  -H "Content-Type: application/json" \\
${token ? `  -H "Authorization: Bearer ${token}" \\
  ` : '  '}--data '{
  "model": "${modelName}",
  "messages": [{"role": "user", "content": "Hello"}],
  "max_tokens": 100
}'`

    const curlAnthropic = `curl ${baseUrl}/v1/messages \\
  -H "Content-Type: application/json" \\
  -H "anthropic-version: 2023-06-01" \\
${token ? `  -H "x-api-key: ${token}" \\
  ` : '  '}--data '{
  "model": "${modelName}",
  "max_tokens": 100,
  "messages": [{"role": "user", "content": "Hello"}]
}'`

    const pythonOpenai = `import openai

client = openai.OpenAI(
    base_url="${baseUrl}/v1",${token ? `\n    api_key="${token}",` : ''}
)

response = client.chat.completions.create(
    model="${modelName}",
    messages=[{"role": "user", "content": "Hello"}],
    max_tokens=100
)
print(response.choices[0].message.content)`

    const pythonAnthropic = `import anthropic

client = anthropic.Anthropic(
    base_url="${baseUrl}/v1/messages",${token ? `\n    api_key="${token}",` : ''}
)

message = client.messages.create(
    model="${modelName}",
    max_tokens=100,
    messages=[{"role": "user", "content": "Hello"}]
)
print(message.content[0].text)`

    const nodeOpenai = `import OpenAI from 'openai';

const client = new OpenAI({
  baseURL: '${baseUrl}/v1',${token ? `\n  apiKey: '${token}',` : ''}
});

const response = await client.chat.completions.create({
  model: '${modelName}',
  messages: [{ role: 'user', content: 'Hello' }],
  max_tokens: 100,
});
console.log(response.choices[0].message.content);`

    const nodeAnthropic = `import Anthropic from '@anthropic-ai/sdk';

const client = new Anthropic({
  baseURL: '${baseUrl}/v1/messages',${token ? `\n  apiKey: '${token}',` : ''}
});

const message = await client.messages.create({
  model: '${modelName}',
  max_tokens: 100,
  messages: [{ role: 'user', content: 'Hello' }],
});
console.log(message.content[0].text);`

    return [
      {
        key: 'curl', label: 'cURL',
        children: [
          { key: 'openai', label: 'OpenAI', code: curlOpenai },
          { key: 'anthropic', label: 'Anthropic', code: curlAnthropic },
        ],
      },
      {
        key: 'python', label: 'Python',
        children: [
          { key: 'openai', label: 'OpenAI', code: pythonOpenai },
          { key: 'anthropic', label: 'Anthropic', code: pythonAnthropic },
        ],
      },
      {
        key: 'node', label: 'Node.js',
        children: [
          { key: 'openai', label: 'OpenAI', code: nodeOpenai },
          { key: 'anthropic', label: 'Anthropic', code: nodeAnthropic },
        ],
      },
    ]
  }

  // ── Platform Select Options ───────────────────────────────────────

  const platformSelectOptions = platforms.map((p: any) => {
    const preset = platformPresets.find(pr => pr.name === p.name)
    return { value: p.id, label: preset ? getPresetName(preset, locale) : p.name }
  })

  // ── Render ────────────────────────────────────────────────────────

  return (
    <div className="page">
      {/* ===== 页头 ===== */}
      <div className="page-head">
        <div>
          <div className="page-title">
            <span className="page-title-chip"><IconApi size={17} stroke={1.7} /></span>
            {t(locale, 'proxies')}
          </div>
          <div className="page-sub">
            {proxies.length} {t(locale, 'total')}
          </div>
        </div>
        <div className="page-actions">
          <Button
            variant="default"
            leftSection={<IconRefresh size={15} />}
            onClick={loadAll}
            loading={loading}
          >
            {t(locale, 'refresh')}
          </Button>
          <Button leftSection={<IconPlus size={15} />} onClick={handleOpenCreate}>
            {t(locale, 'newProxy')}
          </Button>
        </div>
      </div>

      {/* ===== 虚拟模型列表 ===== */}
      <div className="ds-panel">
        <div className="ds-table-wrap">
          <Table stickyHeader>
            <Table.Thead>
              <Table.Tr>
                <Table.Th style={{ width: 200 }}>{t(locale, 'name')}</Table.Th>
                <Table.Th>{t(locale, 'backends')}</Table.Th>
                <Table.Th style={{ width: 150 }}>{t(locale, 'lbStrategy')}</Table.Th>
                <Table.Th className="ds-num" style={{ width: 140 }}>{t(locale, 'action')}</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {loading && proxies.length === 0 && (
                <Table.Tr>
                  <Table.Td colSpan={4}>
                    <div className="ds-empty">{t(locale, 'loading')}</div>
                  </Table.Td>
                </Table.Tr>
              )}
              {!loading && proxies.length === 0 && (
                <Table.Tr>
                  <Table.Td colSpan={4}>
                    <div className="ds-empty">
                      <IconApi size={28} stroke={1.2} />
                      <span>{t(locale, 'noProxies')}</span>
                    </div>
                  </Table.Td>
                </Table.Tr>
              )}
              {proxies.map((proxy: any) => {
                const route = proxyRoutes[proxy.id]
                const backends = route?.backends || []
                const lbValue = route?.lb_strategy || 'RoundRobin'
                const lbLabel = getLbOptions(locale).find(o => o.value === lbValue)?.label || lbValue
                return (
                  <Table.Tr key={proxy.id}>
                    <Table.Td>
                      <span className="ds-pill accent bare ds-mono">{proxy.name}</span>
                    </Table.Td>
                    <Table.Td>
                      {backends.length === 0 ? (
                        <span className="ds-dim">—</span>
                      ) : (
                        <Group gap={6} wrap="wrap">
                          {backends.slice(0, 4).map((b: any) => (
                            <span
                              key={b.id}
                              className="ds-pill neutral bare"
                              title={`${getPlatformDisplayName(b.platform_id)} · ${b.model_id}`}
                            >
                              {getPlatformDisplayName(b.platform_id)}
                              <span className="ds-mono ds-dim" style={{ marginLeft: 4 }}>{b.model_id}</span>
                            </span>
                          ))}
                          {backends.length > 4 && (
                            <Tooltip
                              label={backends.slice(4).map((b: any) => `${getPlatformDisplayName(b.platform_id)} · ${b.model_id}`).join('\n')}
                              multiline
                              w={260}
                            >
                              <span className="ds-pill accent bare">+{backends.length - 4}</span>
                            </Tooltip>
                          )}
                        </Group>
                      )}
                    </Table.Td>
                    <Table.Td>
                      <span className="ds-dim" style={{ fontSize: 12 }}>{lbLabel}</span>
                    </Table.Td>
                    <Table.Td className="ds-num">
                      <Group gap={4} wrap="nowrap" justify="flex-end">
                        <Tooltip label={t(locale, 'usageCode')}>
                          <ActionIcon size="md" onClick={() => handleOpenUsage(proxy)}>
                            <IconCode size={16} />
                          </ActionIcon>
                        </Tooltip>
                        <Tooltip label={t(locale, 'edit')}>
                          <ActionIcon size="md" onClick={() => handleOpenEdit(proxy)}>
                            <IconSettings size={16} />
                          </ActionIcon>
                        </Tooltip>
                        <Tooltip label={t(locale, 'delete')}>
                          <ActionIcon size="md" color="red" onClick={() => confirmDelete(proxy.id)}>
                            <IconTrash size={16} />
                          </ActionIcon>
                        </Tooltip>
                      </Group>
                    </Table.Td>
                  </Table.Tr>
                )
              })}
            </Table.Tbody>
          </Table>
        </div>
      </div>

      {/* ── Create/Edit Modal ────────────────────────────────────── */}
      <Modal
        opened={createOpen}
        onClose={() => { closeCreate(); resetForm() }}
        title={editingProxy ? `${t(locale, 'edit')} - ${editingProxy.name}` : t(locale, 'newProxy')}
        size="lg"
        closeOnClickOutside={false}
        closeOnEscape={!submitting}
      >
        <div>
          <div className="ds-form-section">
            <div className="ds-form-section-title">{t(locale, 'basicInfo')}</div>
            <Stack gap="sm">
              <TextInput
                label={t(locale, 'proxyName')}
                placeholder="qc480"
                required
                value={formName}
                onChange={e => setFormName(e.currentTarget.value)}
                description={t(locale, 'virtualModelPlaceholder')}
              />

              <Select
                label={t(locale, 'lbStrategy')}
                data={getLbOptions(locale)}
                value={formLbStrategy}
                onChange={v => setFormLbStrategy(v || 'RoundRobin')}
              />

              <NumberInput
                label={t(locale, 'defaultMaxTokens')}
                description={t(locale, 'defaultMaxTokensHint')}
                min={1}
                value={formDefaultMaxTokens}
                onChange={v => setFormDefaultMaxTokens(typeof v === 'number' ? v : '')}
              />
            </Stack>
          </div>

          <div className="ds-form-section">
            <div className="ds-form-section-title">{t(locale, 'backendModels')}</div>
            <Stack gap="sm">
              {formBackends.map((backend, index) => (
                <div key={index} className="ds-backend-card">
                  <div className="ds-backend-head">
                    <div className="ds-backend-title">
                      <span className="ds-backend-index">{index + 1}</span>
                      {t(locale, 'backendModel')}
                    </div>
                    {index > 0 && (
                      <Tooltip label={t(locale, 'delete')}>
                        <ActionIcon size="sm" color="red" onClick={() => removeBackendRow(index)}>
                          <IconTrash size={14} />
                        </ActionIcon>
                      </Tooltip>
                    )}
                  </div>

                  <div className="ds-field-grid">
                    <Select
                      label={t(locale, 'selectPlatform')}
                      placeholder={t(locale, 'selectPlatform')}
                      data={platformSelectOptions}
                      value={backend.platform_id}
                      onChange={v => handlePlatformChange(index, v || '')}
                      searchable
                    />
                    <ModelSelect
                      value={backend.model_id}
                      onChange={(v: string) => handleModelChange(index, v)}
                      data={getModelOptions(backend.platform_id)}
                      label={t(locale, 'selectModel')}
                      placeholder={
                        fetchingMap[backend.platform_id]
                          ? t(locale, 'loading')
                          : t(locale, 'selectModel')
                      }
                      disabled={!backend.platform_id}
                      fetching={fetchingMap[backend.platform_id]}
                    />
                    <NumberInput
                      label={t(locale, 'weight')}
                      min={1}
                      value={backend.weight}
                      onChange={v => updateBackendField(index, 'weight', typeof v === 'number' ? v : 1)}
                    />
                    <NumberInput
                      label={t(locale, 'priority')}
                      min={0}
                      value={backend.priority}
                      onChange={v => updateBackendField(index, 'priority', typeof v === 'number' ? v : 0)}
                    />
                  </div>

                  <div style={{ marginTop: 12 }}>
                    <MultiSelect
                      label={t(locale, 'capabilities')}
                      placeholder={t(locale, 'selectCapabilities')}
                      data={capabilityOptions}
                      value={backend.capabilities}
                      onChange={v => updateBackendField(index, 'capabilities', v)}
                    />
                  </div>

                  <div className="ds-field-grid" style={{ marginTop: 12 }}>
                    <Select
                      label={t(locale, 'unknownParams')}
                      description={t(locale, 'unknownParamsHint')}
                      data={[
                        { value: 'Auto', label: t(locale, 'paramStrategyAuto') },
                        { value: 'Drop', label: t(locale, 'paramStrategyDrop') },
                        { value: 'Passthrough', label: t(locale, 'paramStrategyPassthrough') },
                      ]}
                      value={backend.unknown_params}
                      onChange={v => updateBackendField(index, 'unknown_params', v || 'Auto')}
                    />
                    <Textarea
                      label={t(locale, 'paramOverrides')}
                      description={t(locale, 'paramOverridesHint')}
                      placeholder='{"temperature": 0.2, "max_tokens": 8192}'
                      autosize
                      minRows={2}
                      maxRows={6}
                      value={backend.param_overrides}
                      onChange={e => updateBackendField(index, 'param_overrides', e.currentTarget.value)}
                    />
                  </div>
                </div>
              ))}

              <Button
                variant="default"
                className="ds-add-btn"
                onClick={addBackendRow}
                leftSection={<IconPlus size={15} />}
                fullWidth
              >
                {t(locale, 'addBackendModel')}
              </Button>
            </Stack>
          </div>

          <div className="ds-modal-footer">
            <Button variant="default" onClick={() => { closeCreate(); resetForm() }} disabled={submitting}>
              {t(locale, 'cancel')}
            </Button>
            <Button onClick={handleSubmit} loading={submitting}>
              {editingProxy ? t(locale, 'save') : t(locale, 'create')}
            </Button>
          </div>
        </div>
      </Modal>

      {/* ── Usage Modal ──────────────────────────────────────────── */}
      <Modal
        opened={usageOpen}
        onClose={closeUsage}
        title={`${t(locale, 'usageCode')} - ${usageProxy?.name || ''}`}
        size="lg"
      >
        {usageProxy && (
          <Tabs defaultValue="curl">
            <Tabs.List>
              {getUsageSnippets(usageProxy).map(lang => (
                <Tabs.Tab key={lang.key} value={lang.key}>
                  {lang.label}
                </Tabs.Tab>
              ))}
            </Tabs.List>

            {getUsageSnippets(usageProxy).map(lang => (
              <Tabs.Panel key={lang.key} value={lang.key} pt="sm">
                <Tabs defaultValue={lang.children[0]?.key}>
                  <Tabs.List>
                    {lang.children.map(proto => (
                      <Tabs.Tab key={proto.key} value={proto.key}>
                        {proto.label}
                      </Tabs.Tab>
                    ))}
                  </Tabs.List>

                  {lang.children.map(proto => (
                    <Tabs.Panel key={proto.key} value={proto.key} pt="sm">
                      <div style={{ position: 'relative' }}>
                        <pre className="code-block" style={{ maxHeight: 400, overflow: 'auto' }}>
                          {proto.code}
                        </pre>
                        <CopyButton value={proto.code}>
                          {({ copied, copy }) => (
                            <Tooltip label={copied ? (locale === 'zh' ? '已复制' : 'Copied') : (locale === 'zh' ? '复制' : 'Copy')}>
                              <ActionIcon
                                variant="subtle"
                                size="sm"
                                onClick={copy}
                                style={{ position: 'absolute', top: 8, right: 8 }}
                              >
                                {copied ? <IconCheck size={14} style={{ color: 'var(--ok)' }} /> : <IconCopy size={14} />}
                              </ActionIcon>
                            </Tooltip>
                          )}
                        </CopyButton>
                      </div>
                    </Tabs.Panel>
                  ))}
                </Tabs>
              </Tabs.Panel>
            ))}
          </Tabs>
        )}
      </Modal>

      {/* ── Delete Confirm Modal ─────────────────────────────────── */}
      <Modal
        opened={deleteOpen}
        onClose={closeDelete}
        title={t(locale, 'deleteConfirm')}
        size="sm"
      >
        <Text size="sm" c="dimmed">{t(locale, 'deleteConfirm')}</Text>
        <div className="ds-modal-footer">
          <Button variant="default" onClick={closeDelete}>{t(locale, 'cancel')}</Button>
          <Button color="red" onClick={handleDelete}>{t(locale, 'confirm')}</Button>
        </div>
      </Modal>
    </div>
  )
}
