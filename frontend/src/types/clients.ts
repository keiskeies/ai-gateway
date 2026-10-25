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
}

export interface ClientApplyBody {
  base_url?: string
  default_model: string
  sonnet_model?: string
  opus_model?: string
  haiku_model?: string
  fable_model?: string
  subagent_model?: string
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
