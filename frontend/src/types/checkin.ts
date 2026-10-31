export type CheckinStatus = 'new' | 'ok' | 'waiting' | 'expired' | 'error'

/** 平台即账号：platform_id 唯一，一个平台行最多一份登录态 */
export interface CheckinAccount {
  platform_id: string
  platform: string
  /** 登录后的账号名（多账号区分用） */
  identity: string
  label: string
  status: CheckinStatus
  /** 最近一次领取的诊断文字（后端生成） */
  message: string
  last_run_at: string | null
  last_run_day: string | null
  last_ok_day: string | null
  balance: number | null
  daily_amount: number | null
  daily_done: boolean
  updated_at: string
}

/** 后端支持的登录/领取能力，前端按 base_url 前缀匹配平台行 */
export interface CheckinCapability {
  key: string
  brand: string
  base_url_prefix: string
  login_supported: boolean
}

/** 一条定时领取任务：本地时间点位 + 选中的平台账号（平台即账号） */
export interface CheckinTask {
  id: string
  name: string
  /** 'HH:MM'，本地时间 */
  time: string
  platform_ids: string[]
  enabled: boolean
  last_run_at: string | null
  /** 本地日期 'YYYY-MM-DD'：今天是否已经跑过 */
  last_run_day: string | null
  last_message: string
}

export interface CheckinTaskInput {
  name: string
  time: string
  platform_ids: string[]
  enabled: boolean
}

export interface CheckinData {
  accounts: CheckinAccount[]
  tasks: CheckinTask[]
  platforms: CheckinCapability[]
}

export type CheckinAttemptStatus = 'pending' | 'ok' | 'error'

/** 一次登录窗口的结果；新建平台流程会带回 api_key 供表单回填 */
export interface CheckinAttempt {
  id: string
  platform: string
  status: CheckinAttemptStatus
  identity: string | null
  label: string | null
  api_key: string | null
  message: string | null
}
