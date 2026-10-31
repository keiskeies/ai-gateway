import { useState } from 'react'
import {
  ActionIcon,
  Button,
  Group,
  Modal,
  MultiSelect,
  Select,
  Switch,
  Table,
  TextInput,
  Tooltip,
} from '@mantine/core'
import { notify } from '../lib/notify'
import { IconPencil, IconPlayerPlay, IconPlus, IconTrash } from '@tabler/icons-react'
import { createCheckinTask, deleteCheckinTask, runCheckinTask, updateCheckinTask } from '../api'
import { useAppContext } from '../ThemeContext'
import { t, type Locale } from '../i18n'
import { Field } from './Field'
import type { CheckinAccount, CheckinTask } from '../types/checkin'

/** 可选账号：value 是平台行 id，label 由平台页拼（平台名 · 账号名） */
export interface CheckinAccountOption {
  value: string
  label: string
  /** 还没登录的账号不能领取，列出来但禁用，免得用户找不到 */
  disabled?: boolean
}

interface Draft {
  id?: string
  name: string
  hour: string
  minute: string
  platform_ids: string[]
  enabled: boolean
}

const HOURS = Array.from({ length: 24 }, (_, i) => String(i).padStart(2, '0'))
const MINUTES = Array.from({ length: 60 }, (_, i) => String(i).padStart(2, '0'))

function errorMessage(error: unknown, fallback: string): string {
  const anyError = error as { response?: { data?: { error?: { message?: string } } }; message?: string }
  return anyError?.response?.data?.error?.message || anyError?.message || fallback
}

/** 与后端 `chrono::Local` 同口径的本地日期，用来判断「今天跑过没有」 */
function localDay(at: Date): string {
  const month = String(at.getMonth() + 1).padStart(2, '0')
  const day = String(at.getDate()).padStart(2, '0')
  return `${at.getFullYear()}-${month}-${day}`
}

function timeOf(raw: string): number {
  const [hour, minute] = raw.split(':').map(Number)
  return (hour || 0) * 60 + (minute || 0)
}

/** 下次运行：停用 / 今天已跑（还有未到账的就说跟进）/ 已过点位（今天补跑）/ 还没到点 */
function nextRunText(task: CheckinTask, locale: Locale, followingUp: boolean): string {
  if (!task.enabled) return t(locale, 'checkinTaskDisabled')
  const now = new Date()
  if (task.last_run_day === localDay(now)) {
    return followingUp ? t(locale, 'checkinTaskFollowingUp') : t(locale, 'checkinTaskNextTomorrow', task.time)
  }
  if (timeOf(task.time) <= now.getHours() * 60 + now.getMinutes()) return t(locale, 'checkinTaskNextSoon')
  return t(locale, 'checkinTaskNextAt', task.time)
}

export function CheckinTasksModal({ opened, onClose, tasks, accounts, options, onChanged }: {
  opened: boolean
  onClose: () => void
  tasks: CheckinTask[]
  /** 用于判断「今天跑过了但还没到账」——那种情况下今天还会再跑，不能说成明天 */
  accounts: CheckinAccount[]
  options: CheckinAccountOption[]
  onChanged: () => void
}) {
  const { locale } = useAppContext()
  const [draft, setDraft] = useState<Draft | null>(null)
  const [saving, setSaving] = useState(false)
  const [busyId, setBusyId] = useState<string | null>(null)

  const labelFor = (id: string) => options.find(option => option.value === id)?.label ?? null
  // 未到账的账号：任务今天跑过之后还会每 3 小时再看一次，「下次」就不能写成明天
  const waiting = new Set(
    accounts.filter(account => account.status === 'waiting').map(account => account.platform_id),
  )

  const startNew = () => {
    setDraft({ name: '', hour: '00', minute: '05', platform_ids: options.filter(o => !o.disabled).map(o => o.value), enabled: true })
  }

  const startEdit = (task: CheckinTask) => {
    const [hour, minute] = task.time.split(':')
    setDraft({ id: task.id, name: task.name, hour, minute, platform_ids: task.platform_ids, enabled: task.enabled })
  }

  const save = async () => {
    if (!draft) return
    const body = {
      name: draft.name,
      time: `${draft.hour}:${draft.minute}`,
      platform_ids: draft.platform_ids,
      enabled: draft.enabled,
    }
    if (body.platform_ids.length === 0) {
      notify(locale, { color: 'red', message: t(locale, 'checkinTaskNeedAccounts') })
      return
    }
    setSaving(true)
    try {
      if (draft.id) await updateCheckinTask(draft.id, body)
      else await createCheckinTask(body)
      setDraft(null)
      onChanged()
    } catch (error) {
      notify(locale, { color: 'red', message: errorMessage(error, t(locale, 'requestFailed')) })
    } finally {
      setSaving(false)
    }
  }

  const remove = async (task: CheckinTask) => {
    setBusyId(task.id)
    try {
      await deleteCheckinTask(task.id)
      if (draft?.id === task.id) setDraft(null)
      onChanged()
    } catch (error) {
      notify(locale, { color: 'red', message: errorMessage(error, t(locale, 'requestFailed')) })
    } finally {
      setBusyId(null)
    }
  }

  const toggle = async (task: CheckinTask, enabled: boolean) => {
    try {
      await updateCheckinTask(task.id, {
        name: task.name,
        time: task.time,
        platform_ids: task.platform_ids,
        enabled,
      })
      onChanged()
    } catch (error) {
      notify(locale, { color: 'red', message: errorMessage(error, t(locale, 'requestFailed')) })
    }
  }

  const runNow = async (task: CheckinTask) => {
    setBusyId(task.id)
    try {
      const result = await runCheckinTask(task.id)
      notify(locale, {
        color: result.message.includes('失败') || result.message.includes('未登录') ? 'yellow' : 'teal',
        message: result.message,
        autoClose: 6000,
      })
      onChanged()
    } catch (error) {
      notify(locale, { color: 'red', message: errorMessage(error, t(locale, 'requestFailed')) })
    } finally {
      setBusyId(null)
    }
  }

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title={t(locale, 'checkinTasks')}
      size="xl"
      closeOnClickOutside={false}
      closeButtonProps={{ 'aria-label': t(locale, 'close') }}
    >
      <div className="checkin-task-note">{t(locale, 'checkinTasksHint')}</div>

      {tasks.length === 0 && !draft && (
        <div className="checkin-task-empty">
          <div>{t(locale, 'checkinTaskNone')}</div>
          <div className="muted">{t(locale, 'checkinTaskNoneDesc')}</div>
          <Button size="xs" leftSection={<IconPlus size={14} />} onClick={startNew}>
            {t(locale, 'checkinTaskNew')}
          </Button>
        </div>
      )}

      {tasks.length > 0 && (
        <Table className="console-table">
          <thead>
            <tr>
              <th>{t(locale, 'checkinTaskColumnTime')}</th>
              <th>{t(locale, 'checkinTaskColumnAccounts')}</th>
              <th>{t(locale, 'checkinTaskColumnLast')}</th>
              <th>{t(locale, 'checkinTaskColumnEnabled')}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {tasks.map(task => (
              <tr key={task.id}>
                <td className="checkin-task-time">
                  {task.time}
                  {task.name && <div className="muted">{task.name}</div>}
                </td>
                <td>
                  <Group gap={4} wrap="nowrap">
                    {task.platform_ids.map(id => {
                      const label = labelFor(id)
                      return label
                        ? <span key={id} className="checkin-task-tag">{label}</span>
                        : <span key={id} className="checkin-task-tag missing">{t(locale, 'checkinTaskGone')}</span>
                    })}
                  </Group>
                </td>
                <td>
                  <div className={task.last_message ? 'checkin-task-last' : 'muted'}>
                    {task.last_message || t(locale, 'checkinTaskNeverRun')}
                  </div>
                  <div className="muted">{nextRunText(task, locale, task.platform_ids.some(id => waiting.has(id)))}</div>
                </td>
                <td>
                  <Switch
                    size="xs"
                    checked={task.enabled}
                    aria-label={t(locale, 'checkinTaskColumnEnabled')}
                    onChange={event => void toggle(task, event.currentTarget.checked)}
                  />
                </td>
                <td>
                  <Group gap={4} justify="flex-end">
                    <Tooltip label={t(locale, 'checkinTaskRunNow')}>
                      <ActionIcon
                        variant="light"
                        size="sm"
                        aria-label={t(locale, 'checkinTaskRunNow')}
                        loading={busyId === task.id}
                        onClick={() => void runNow(task)}
                      >
                        <IconPlayerPlay size={13} />
                      </ActionIcon>
                    </Tooltip>
                    <ActionIcon variant="subtle" size="sm" aria-label={t(locale, 'checkinTaskEdit')} onClick={() => startEdit(task)}>
                      <IconPencil size={13} />
                    </ActionIcon>
                    <ActionIcon
                      variant="subtle"
                      size="sm"
                      color="red"
                      aria-label={t(locale, 'checkinTaskDelete')}
                      onClick={() => void remove(task)}
                    >
                      <IconTrash size={13} />
                    </ActionIcon>
                  </Group>
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}

      {draft && (
        <div className="checkin-task-editor">
          <div className="console-form-grid">
            <Field label={t(locale, 'checkinTaskName')}>
              <TextInput
                size="xs"
                value={draft.name}
                placeholder={t(locale, 'checkinTaskNamePlaceholder')}
                onChange={event => setDraft({ ...draft, name: event.currentTarget.value })}
              />
            </Field>
            <Group gap="xs" align="flex-end" wrap="nowrap">
              <Field label={t(locale, 'checkinTaskHour')}>
                <Select
                  size="xs"
                  w={72}
                  data={HOURS}
                  value={draft.hour}
                  aria-label={t(locale, 'checkinTaskHour')}
                  onChange={value => setDraft({ ...draft, hour: value ?? '00' })}
                />
              </Field>
              <Field label={t(locale, 'checkinTaskMinute')}>
                <Select
                  size="xs"
                  w={72}
                  data={MINUTES}
                  value={draft.minute}
                  aria-label={t(locale, 'checkinTaskMinute')}
                  onChange={value => setDraft({ ...draft, minute: value ?? '00' })}
                />
              </Field>
            </Group>
            <Field full label={t(locale, 'checkinTaskColumnAccounts')}>
              <MultiSelect
                size="xs"
                data={options}
                value={draft.platform_ids}
                searchable
                hidePickedOptions
                placeholder={t(locale, 'checkinTaskPickAccounts')}
                onChange={value => setDraft({ ...draft, platform_ids: value })}
              />
            </Field>
          </div>
          <Group gap="xs" mt="sm">
            <Button size="xs" loading={saving} onClick={() => void save()}>
              {t(locale, 'checkinTaskSave')}
            </Button>
            <Button size="xs" variant="default" onClick={() => setDraft(null)}>
              {t(locale, 'cancel')}
            </Button>
          </Group>
        </div>
      )}

      {tasks.length > 0 && !draft && (
        <Group mt="sm">
          <Button size="xs" variant="default" leftSection={<IconPlus size={14} />} onClick={startNew}>
            {t(locale, 'checkinTaskNew')}
          </Button>
        </Group>
      )}
    </Modal>
  )
}
