import { useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Button, SegmentedControl, Switch } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import {
  IconAlertTriangle,
  IconCheck,
  IconDownload,
  IconSearch,
  IconUpload,
} from '@tabler/icons-react'
import {
  applyConfigImport,
  exportConfig,
  planConfigImport,
  type ConfigIssue,
  type ImportModeName,
  type ImportPlan,
  type ImportResult,
} from '../api'
import { useAppContext } from '../ThemeContext'
import { t, type Locale, type TranslationKey } from '../i18n'

type PlanTableField = 'platforms' | 'models' | 'proxies' | 'routes' | 'backends' | 'api_keys'

const PLAN_TABLES: { field: PlanTableField; labelKey: TranslationKey }[] = [
  { field: 'platforms', labelKey: 'tablePlatforms' },
  { field: 'models', labelKey: 'tableModels' },
  { field: 'proxies', labelKey: 'tableProxies' },
  { field: 'routes', labelKey: 'tableRoutes' },
  { field: 'backends', labelKey: 'tableBackends' },
  { field: 'api_keys', labelKey: 'tableApiKeys' },
]

const serverMessage = (err: any): string => err?.response?.data?.error?.message ?? ''

// 导出时间戳带纳秒和时区，塞在标题行里读不动；核对"是不是这份备份"只需要到分钟。
const shownAt = (iso: string): string => {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

// 后端只发 code + 参数，句子在前端字典里；查不到的 code 也要能把参数说清楚，
// 不能让一个新增的检查项在界面上退化成裸字符串。
function issueText(locale: Locale, issue: ConfigIssue): string {
  const text = t(locale, issue.code as TranslationKey, ...issue.args)
  if (text === issue.code) return [issue.code, ...issue.args].join(' · ')
  return text
}

const totalAdd = (result: ImportResult) =>
  PLAN_TABLES.reduce((sum, table) => sum + result[table.field].add, 0)

// 全是跳过＝这份文件相对本机什么都没做，说出来比让人自己对四列数字更省事
const isNoChange = (plan: ImportPlan) =>
  plan.ok &&
  PLAN_TABLES.every(({ field }) => {
    const cell = plan[field]
    return cell.add === 0 && cell.overwrite === 0 && cell.drop === 0
  })

export function ConfigTransfer() {
  const { locale } = useAppContext()
  const queryClient = useQueryClient()
  const fileRef = useRef<HTMLInputElement>(null)

  const [includeSecrets, setIncludeSecrets] = useState(false)
  const [exporting, setExporting] = useState(false)

  const [doc, setDoc] = useState<Record<string, unknown> | null>(null)
  const [fileName, setFileName] = useState('')
  const [mode, setMode] = useState<ImportModeName>('merge')
  const [plan, setPlan] = useState<ImportPlan | null>(null)
  const [previewedKey, setPreviewedKey] = useState('')
  const [previewing, setPreviewing] = useState(false)
  const [applying, setApplying] = useState(false)

  const draftKey = `${fileName}:${mode}`
  const planStale = plan !== null && previewedKey !== draftKey

  const reset = () => {
    setDoc(null)
    setFileName('')
    setPlan(null)
    setPreviewedKey('')
    if (fileRef.current) fileRef.current.value = ''
  }

  const onExport = async () => {
    setExporting(true)
    try {
      const resp = await exportConfig(includeSecrets)
      const header = String(resp.headers?.['content-disposition'] ?? '')
      const name = /filename="?([^";]+)"?/.exec(header)?.[1] ?? 'aigw-config.json'
      const url = URL.createObjectURL(resp.data as Blob)
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = name
      anchor.click()
      URL.revokeObjectURL(url)
      notifications.show({
        message: t(locale, 'exportReady', name),
        color: includeSecrets ? 'yellow' : 'green',
        icon: <IconCheck size={18} />,
      })
    } catch {
      notifications.show({ message: t(locale, 'exportFailed'), color: 'red' })
    } finally {
      setExporting(false)
    }
  }

  const onPickFile = async (file: File) => {
    setPlan(null)
    setPreviewedKey('')
    try {
      const parsed = JSON.parse(await file.text())
      if (!parsed || typeof parsed !== 'object' || parsed.format !== 'aigw-config') {
        setDoc(null)
        setFileName('')
        if (fileRef.current) fileRef.current.value = ''
        notifications.show({ message: t(locale, 'notConfigFile'), color: 'red' })
        return
      }
      setDoc(parsed)
      setFileName(file.name)
    } catch (err: any) {
      notifications.show({
        message: t(locale, 'fileUnreadable', err?.message ?? String(err)),
        color: 'red',
      })
    }
  }

  const onPreview = async () => {
    if (!doc) return
    setPreviewing(true)
    try {
      const next = await planConfigImport(doc, mode)
      setPlan(next)
      setPreviewedKey(draftKey)
      if (!next.ok) {
        notifications.show({
          title: t(locale, 'planErrors'),
          message: issueText(locale, next.errors[0]),
          color: 'red',
          icon: <IconAlertTriangle size={18} />,
        })
      }
    } catch (err: any) {
      notifications.show({
        message: `${t(locale, 'planFailed')}: ${serverMessage(err) || err?.message}`,
        color: 'red',
      })
    } finally {
      setPreviewing(false)
    }
  }

  const onApply = async () => {
    if (!doc || !plan || planStale) return
    setApplying(true)
    try {
      const result = await applyConfigImport(doc, mode)
      const added = totalAdd(result)
      notifications.show({
        title: t(locale, 'configTransfer'),
        message: added > 0 ? t(locale, 'importApplied', String(added)) : t(locale, 'importAppliedNothing'),
        color: added > 0 ? 'green' : 'gray',
        icon: <IconCheck size={18} />,
      })
      // 六张表是一起换的，逐个 queryKey 失效迟早会漏掉一处还在显示旧配置
      await queryClient.invalidateQueries()
      reset()
    } catch (err: any) {
      notifications.show({
        message: `${t(locale, 'importFailed')}: ${serverMessage(err) || err?.message}`,
        color: 'red',
      })
    } finally {
      setApplying(false)
    }
  }

  return (
    <div className="console-settings-grid" style={{ marginTop: 14 }}>
      <section className="console-panel">
        <div className="console-panel-head">
          <div className="console-panel-title">{t(locale, 'configExportTitle')}</div>
          <span className="console-panel-sub">{t(locale, 'exportLabel')}</span>
        </div>
        <div className="console-panel-body">
          <div className="console-note">{t(locale, 'exportIntro')}</div>
          <div style={{ marginTop: 12 }}>
            <Switch
              size="xs"
              label={t(locale, 'includePlatformKeys')}
              checked={includeSecrets}
              onChange={(e) => setIncludeSecrets(e.currentTarget.checked)}
            />
          </div>
          {includeSecrets && (
            <div className="console-note warn">{t(locale, 'includePlatformKeysDesc')}</div>
          )}
          <div className="console-route-actions">
            <Button
              size="xs"
              variant="default"
              leftSection={<IconDownload size={13} />}
              loading={exporting}
              onClick={() => void onExport()}
            >
              {t(locale, 'downloadBackup')}
            </Button>
          </div>
        </div>
      </section>

      <section className="console-panel">
        <div className="console-panel-head">
          <div className="console-panel-title">{t(locale, 'configImportTitle')}</div>
          <span className="console-panel-sub">{t(locale, 'importLabel')}</span>
        </div>
        <div className="console-panel-body">
          <div className="console-note">{t(locale, 'importIntro')}</div>

          <input
            ref={fileRef}
            type="file"
            accept="application/json,.json"
            style={{ display: 'none' }}
            onChange={(e) => {
              const file = e.currentTarget.files?.[0]
              if (file) void onPickFile(file)
            }}
          />
          <div className="console-route-actions">
            <Button
              size="xs"
              variant="default"
              leftSection={<IconUpload size={13} />}
              onClick={() => fileRef.current?.click()}
            >
              {t(locale, 'chooseJsonFile')}
            </Button>
            {fileName && <span className="console-mono console-plan-file">{fileName}</span>}
          </div>

          <div className="console-plan-mode">
            <span className="console-plan-mode-label">{t(locale, 'importModeLabel')}</span>
            <SegmentedControl
              size="xs"
              data={[
                { value: 'merge', label: t(locale, 'modeMerge') },
                { value: 'replace', label: t(locale, 'modeReplace') },
              ]}
              value={mode}
              onChange={(value) => setMode(value as ImportModeName)}
            />
          </div>
          <div className="console-hint">
            {mode === 'merge' ? t(locale, 'modeMergeDesc') : t(locale, 'modeReplaceDesc')}
          </div>

          <div className="console-route-actions">
            <Button
              size="xs"
              variant="default"
              leftSection={<IconSearch size={13} />}
              disabled={!doc}
              loading={previewing}
              onClick={() => void onPreview()}
            >
              {t(locale, 'previewPlan')}
            </Button>
          </div>

          {plan && (
            <div className={`console-plan${planStale ? ' console-plan-stale' : ''}`}>
              <div className="console-plan-head">
                <span>{t(locale, 'planTitle')}</span>
                <span className="console-plan-src">
                  {plan.source_version
                    ? t(locale, 'planSource', plan.source_version, shownAt(plan.source_exported_at))
                    : shownAt(plan.source_exported_at)}
                </span>
              </div>
              <div className="console-plan-row head">
                <span>{t(locale, 'planColRows')}</span>
                <span>{t(locale, 'planAdd')}</span>
                <span>{t(locale, 'planOverwrite')}</span>
                <span>{t(locale, 'planSkip')}</span>
                <span>{t(locale, 'planDrop')}</span>
              </div>
              {PLAN_TABLES.map(({ field, labelKey }) => {
                const cell = plan[field]
                return (
                  <div className="console-plan-row" key={field}>
                    <span className="console-plan-name">{t(locale, labelKey)}</span>
                    <span className={cell.add ? 'console-plan-num add' : 'console-plan-num zero'}>{cell.add}</span>
                    <span className={cell.overwrite ? 'console-plan-num' : 'console-plan-num zero'}>{cell.overwrite}</span>
                    <span className={cell.skip ? 'console-plan-num' : 'console-plan-num zero'}>{cell.skip}</span>
                    <span className={cell.drop ? 'console-plan-num drop' : 'console-plan-num zero'}>{cell.drop}</span>
                  </div>
                )
              })}
              {isNoChange(plan) && (
                <div className="console-note">{t(locale, 'planNothing')}</div>
              )}
              {plan.errors.length > 0 && (
                <div className="console-plan-issues">
                  <div className="console-plan-issues-title err">{t(locale, 'planErrors')}</div>
                  <ul>
                    {plan.errors.map((issue, i) => (
                      <li key={`e${i}`}>{issueText(locale, issue)}</li>
                    ))}
                  </ul>
                </div>
              )}
              {plan.warnings.length > 0 && (
                <div className="console-plan-issues">
                  <div className="console-plan-issues-title warn">{t(locale, 'planWarnings')}</div>
                  <ul>
                    {plan.warnings.map((issue, i) => (
                      <li key={`w${i}`}>{issueText(locale, issue)}</li>
                    ))}
                  </ul>
                </div>
              )}
              {planStale && <div className="console-note warn">{t(locale, 'planStale')}</div>}
              <div className="console-route-actions">
                <Button
                  size="xs"
                  variant={mode === 'replace' ? 'filled' : 'default'}
                  color={mode === 'replace' ? 'red' : undefined}
                  leftSection={<IconCheck size={13} />}
                  disabled={!plan.ok || planStale}
                  loading={applying}
                  onClick={() => void onApply()}
                >
                  {t(locale, 'applyImport')}
                </Button>
              </div>
            </div>
          )}
        </div>
      </section>
    </div>
  )
}
