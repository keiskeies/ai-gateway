// 请求体覆盖（平台 custom_body / 路由与后端 param_overrides）在屏上都是一段
// 可编辑 JSON 文本。三个编辑器共用这里的口径。

// 空对象显示成空文本：摆一个 `{}` 在那儿只会让人以为已经填了东西。
export const jsonText = (value: Record<string, unknown> | undefined | null): string =>
  value && Object.keys(value).length ? JSON.stringify(value, null, 2) : ''

// 空串 = {}；非法 JSON 或不是对象则抛错，由调用方提示。
export const parseOverrideText = (text: string): Record<string, unknown> => {
  const trimmed = text.trim()
  if (!trimmed) return {}
  const parsed = JSON.parse(trimmed)
  if (typeof parsed !== 'object' || Array.isArray(parsed) || parsed === null) throw new Error('invalid')
  return parsed as Record<string, unknown>
}

// 键序在 JSON 里没有语义，而后端 serde_json 没开 preserve_order，回读永远是字典序。
// 拿文本框原文与格式化后的服务端值比字符串，会造出永远灭不掉的"未保存"：
// 输入 {"b":1,"a":2}、保存成功、refetch 回 {"a":2,"b":1}，文本不同 → 按钮常亮。
const canonicalJson = (value: unknown): string => {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`
  if (value !== null && typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([k, v]) => `${JSON.stringify(k)}:${canonicalJson(v)}`)
    return `{${entries.join(',')}}`
  }
  return JSON.stringify(value)
}

// 文本与服务端值是否语义一致。文本非法时返回 false——那是"确实没保存"，
// 调用方另有 invalid 分支禁掉保存键并就地报错。
export const sameOverrideText = (
  text: string,
  value: Record<string, unknown> | undefined | null,
): boolean => {
  try {
    return canonicalJson(parseOverrideText(text)) === canonicalJson(value ?? {})
  } catch {
    return false
  }
}

// 文本框当前内容是否是非法覆盖（半截 JSON 在输入过程中是常态，只用于点亮错误态）
export const isOverrideTextInvalid = (text: string): boolean => {
  try {
    parseOverrideText(text)
    return false
  } catch {
    return true
  }
}
