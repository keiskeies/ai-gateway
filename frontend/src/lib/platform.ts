import { testPlatformConfig } from '../api'
import { t, type Locale } from '../i18n'

// 平台类型枚举（后端 PascalCase 变体）→ 展示名，列表页与详情页共用
export const PLATFORM_TYPE_LABELS: Record<string, string> = {
  OpenAI: 'OpenAI Compatible',
  Anthropic: 'Anthropic',
  Ollama: 'Ollama',
  Azure: 'Azure OpenAI',
  Custom: 'Custom',
}

export function platformTypeLabel(value: string | undefined): string {
  if (!value) return '—'
  return PLATFORM_TYPE_LABELS[value] ?? value
}

export interface PlatformTestResult {
  ok: boolean
  message: string
  at: number
}

// 免落库的连通性探测：列表行内测试、新建表单测试与向导共用同一份文案口径
export async function probePlatformConfig(
  locale: Locale,
  payload: { type: string; base_url: string; api_key?: string; platform_id?: string; custom_headers?: Record<string, string> },
): Promise<PlatformTestResult> {
  try {
    const result = await testPlatformConfig(payload)
    const latency = Math.round(result?.latency_ms ?? 0)
    const count = result?.model_count ?? (result?.models?.length ?? 0)
    const message = result?.ok
      ? (count > 0 ? t(locale, 'testOkModels', String(count), String(latency)) : t(locale, 'testOkNoList', String(latency)))
      : (result?.error || t(locale, 'testFailedGeneric'))
    return { ok: Boolean(result?.ok), message, at: Date.now() }
  } catch (error: any) {
    return {
      ok: false,
      message: error?.response?.data?.error?.message || t(locale, 'testFailedGeneric'),
      at: Date.now(),
    }
  }
}
