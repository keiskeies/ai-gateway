import { NumberInput, Select, Stack, TextInput } from '@mantine/core'
import { t } from '../i18n'
import type { Locale } from '../i18n'

// 密钥表单只有这一份：/keys 的新建/编辑弹窗和 /keys/:id 详情页共用，
// 否则「新建能设、详情改不了」这类口径分家迟早会出现。
export interface KeyFormValues {
  name: string
  proxy_id: string
  expires_at: string
  quota_requests: string
  quota_tokens: string
  quota_cost: string
  quota_window: string
}

export const emptyKeyForm: KeyFormValues = {
  name: '',
  proxy_id: '',
  expires_at: '',
  quota_requests: '',
  quota_tokens: '',
  quota_cost: '',
  quota_window: 'lifetime',
}

// datetime-local 要的是本地墙上时间；库里存的是带时区的 RFC3339。
// 交给 Date 来回换算，别自己拼字符串——拼出来的时区会差一天。
export function toLocalInput(iso?: string | null): string {
  if (!iso) return ''
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return ''
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

export function fromLocalInput(value: string): string | null {
  if (!value.trim()) return null
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? null : date.toISOString()
}

// 空值＝不限，数字＝设值：写 0 会被后端当成"没设"，所以这里也不给填 0 的机会
export function quotaValue(value: string | number | null | undefined): number | null {
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed > 0 ? Math.floor(parsed) : null
}

// 金额配额不能取整：预算设 2.5 结果落成 2 会让人在还剩 0.5 的时候被拒
export function costValue(value: string | number | null | undefined): number | null {
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null
}

// 库里只存 day/week/month 或 NULL；表单里的空串与 lifetime 都发 null
export function windowValue(value: string | null | undefined): string | null {
  const normalized = (value ?? '').trim().toLowerCase()
  return ['day', 'week', 'month'].includes(normalized) ? normalized : null
}

// 窗口在列表、详情、告警里都要能读成一个词，口径放一处
export function windowLabel(locale: Locale, window?: string | null): string {
  switch (windowValue(window)) {
    case 'day': return t(locale, 'quotaWindowDay')
    case 'week': return t(locale, 'quotaWindowWeek')
    case 'month': return t(locale, 'quotaWindowMonth')
    default: return t(locale, 'quotaWindowLifetime')
  }
}

export function KeyFields(props: {
  getInputProps: (key: keyof KeyFormValues) => any
  locale: Locale
  proxyOptions: { value: string; label: string }[]
}) {
  const { getInputProps, locale, proxyOptions } = props
  return (
    <Stack gap="sm">
      <TextInput
        label={t(locale, 'apiKeyName')}
        placeholder={t(locale, 'apiKeyNamePlaceholder')}
        required
        {...getInputProps('name')}
      />
      <Select
        label={t(locale, 'apiKeyProxy')}
        description={t(locale, 'apiKeyProxyAll')}
        placeholder={t(locale, 'apiKeyProxyAll')}
        data={proxyOptions}
        clearable
        {...getInputProps('proxy_id')}
      />
      <TextInput
        label={t(locale, 'keyExpires')}
        description={t(locale, 'keyExpiresHint')}
        type="datetime-local"
        {...getInputProps('expires_at')}
      />
      <div className="console-field-row">
        <NumberInput
          label={t(locale, 'quotaRequests')}
          placeholder={t(locale, 'quotaUnlimited')}
          min={1}
          hideControls
          {...getInputProps('quota_requests')}
        />
        <NumberInput
          label={t(locale, 'quotaTokens')}
          placeholder={t(locale, 'quotaUnlimited')}
          min={1}
          hideControls
          {...getInputProps('quota_tokens')}
        />
      </div>
      <div className="console-field-row">
        <NumberInput
          label={t(locale, 'quotaCost')}
          placeholder={t(locale, 'quotaUnlimited')}
          min={0}
          hideControls
          {...getInputProps('quota_cost')}
        />
        <Select
          label={t(locale, 'quotaWindow')}
          data={[
            { value: 'lifetime', label: t(locale, 'quotaWindowLifetime') },
            { value: 'day', label: t(locale, 'quotaWindowDay') },
            { value: 'week', label: t(locale, 'quotaWindowWeek') },
            { value: 'month', label: t(locale, 'quotaWindowMonth') },
          ]}
          {...getInputProps('quota_window')}
        />
      </div>
      <div className="ds-hint">{t(locale, 'quotaScopeHint')}</div>
    </Stack>
  )
}
