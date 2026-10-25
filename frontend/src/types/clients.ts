export type ClientId = 'claude_code' | 'codex' | 'opencode' | 'openclaw' | 'pi' | 'hermes'

export interface ClientCurrent {
  base_url?: string | null
  model?: string | null
  key_present: boolean
  provider?: string | null
}

export interface ClientStatus {
  id: ClientId
  name: string
  path: string
  exists: boolean
  detected: boolean
  managed: boolean
  backup_path?: string | null
  current: ClientCurrent
  error?: string | null
  warnings: string[]
}

export interface ClientChange {
  path: string
  before?: string | null
  after: string
  secret: boolean
}

export interface ClientPreview {
  id: ClientId
  path: string
  exists: boolean
  managed: boolean
  backup_path?: string | null
  key_action: 'create' | 'reuse' | string
  changes: ClientChange[]
  warnings: string[]
  current: ClientCurrent
  /** 附加项当前值（Claude Code 才有），用于表单回填 */
  env: Record<string, string>
  tier_names: Record<string, string>
}

export interface ClientApplyBody {
  base_url?: string
  default_model: string
  sonnet_model?: string
  opus_model?: string
  haiku_model?: string
  fable_model?: string
  subagent_model?: string
  tier_names?: Record<string, string>
  env?: Record<string, string>
}

export interface ClientApplyResult {
  id: ClientId
  path: string
  backup_path?: string | null
  key_id: string
  key_name: string
  key_mask: string
  created_key: boolean
  changes: ClientChange[]
  warnings: string[]
}

export interface KeptChange {
  path: string
  reason: string
}

export interface ClientRestoreResult {
  id: ClientId
  path: string
  backup_path?: string | null
  restored: string[]
  kept: KeptChange[]
  disabled_key?: string | null
  warnings: string[]
}

/** 配置文件原文（逃生口） */
export interface ClientConfigFile {
  id: string
  path: string
  exists: boolean
  content: string
}

export interface ClientConfigWrite {
  path: string
  backup_path?: string | null
  replaced: boolean
}

/** 服务端下发的附加字段：类型由后端说，前端只渲染 */
export interface ExtraSpec {
  key: string
  kind: 'text' | 'number' | 'bool' | 'choice'
  options?: string[]
  /** i18n 键；服务端只下发键名，文案仍在前端 */
  label: import('../i18n').TranslationKey
  path: string
}
