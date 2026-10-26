// 请求/响应正文的结构化解析。
//
// 详情页面对的是“上游原始响应”，不是网关转换后的结果：
// - 非流式：OpenAI / Anthropic 的 JSON；
// - 流式：原始 SSE（`data: {...}` 行），需要把增量拼回完整文本、思考与工具调用。
//
// 解析失败不是异常路径：截断的正文、未知供应商格式都会回退到原始文本，
// 由调用方把 `parseError` 显示出来。这里不产生任何用户可见的自然语言。

export type PayloadBlock =
  | { kind: 'text'; text: string }
  | { kind: 'thinking'; text: string }
  | { kind: 'tool_use'; name: string; id?: string; input: string }
  | { kind: 'tool_result'; text: string; toolUseId?: string }
  | { kind: 'image'; label: string }
  | { kind: 'unknown'; label: string; json: string }

export interface PayloadMessage {
  role: string
  name?: string
  toolCallId?: string
  blocks: PayloadBlock[]
}

export interface PayloadTool {
  name: string
  description?: string
  schema?: string
}

export interface PayloadKeyValue {
  name: string
  value: string
}

export interface ParsedRequest {
  model?: string
  stream: boolean
  system?: string
  messages: PayloadMessage[]
  tools: PayloadTool[]
  params: PayloadKeyValue[]
  /** 原始请求正文（截断时只有前缀） */
  raw: string
  truncated: boolean
  parseError?: 'empty' | 'invalid-json'
}

export interface ParsedResponse {
  stream: boolean
  model?: string
  blocks: PayloadBlock[]
  finishReason?: string
  usage: PayloadKeyValue[]
  /** 流式响应解析成功的事件数；非流式为 undefined */
  events?: number
  raw: string
  truncated: boolean
  parseError?: 'empty' | 'invalid-json' | 'unknown-format'
}

type Dict = Record<string, any>

function asDict(value: unknown): Dict | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Dict : null
}

function str(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined
}

function pretty(value: unknown): string {
  if (value == null) return ''
  if (typeof value === 'string') return value
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

/** 工具参数可能是 JSON 字符串，也可能是对象；能解析就展开，不能就原样保留。 */
function toolInputText(value: unknown): string {
  if (typeof value === 'string') {
    const trimmed = value.trim()
    if (!trimmed) return ''
    try {
      return JSON.stringify(JSON.parse(trimmed), null, 2)
    } catch {
      return value
    }
  }
  return pretty(value ?? {})
}

function imageLabel(o: Dict): string {
  const source = asDict(o.source)
  const mime = str(source?.media_type) ?? str(o.media_type)
  if (mime) return mime
  return o.type === 'image_url' || o.image_url ? 'image_url' : 'image'
}

function blocksFromPart(part: unknown): PayloadBlock[] {
  if (typeof part === 'string') return part ? [{ kind: 'text', text: part }] : []
  const o = asDict(part)
  if (!o) return [{ kind: 'unknown', label: 'part', json: pretty(part) }]
  const type = str(o.type) ?? ''
  switch (type) {
    case 'text':
    case 'input_text':
    case 'output_text': {
      const text = str(o.text) ?? str(o.content)
      return text ? [{ kind: 'text', text }] : []
    }
    case 'thinking': {
      const text = str(o.thinking) ?? str(o.text)
      return text ? [{ kind: 'thinking', text }] : []
    }
    case 'tool_use': {
      return [{
        kind: 'tool_use',
        name: str(o.name) ?? 'tool',
        id: str(o.id),
        input: toolInputText(o.input),
      }]
    }
    case 'tool_result': {
      return [{
        kind: 'tool_result',
        toolUseId: str(o.tool_use_id),
        text: flattenContent(o.content),
      }]
    }
    case 'image':
    case 'image_url':
    case 'input_image':
      return [{ kind: 'image', label: imageLabel(o) }]
    default:
      return [{ kind: 'unknown', label: type || 'block', json: pretty(o) }]
  }
}

function blocksFromContent(content: unknown): PayloadBlock[] {
  if (content == null) return []
  if (typeof content === 'string') return content ? [{ kind: 'text', text: content }] : []
  if (Array.isArray(content)) return content.flatMap(blocksFromPart)
  return blocksFromPart(content)
}

/** 工具结果/多模态内容压成一段可读文本；图片用占位标记，不丢“这里有一张图”的事实。 */
function flattenContent(content: unknown): string {
  if (typeof content === 'string') return content
  if (Array.isArray(content)) {
    return content.map((part) => {
      if (typeof part === 'string') return part
      const o = asDict(part)
      if (!o) return pretty(part)
      if (str(o.text)) return str(o.text)!
      if (str(o.thinking)) return str(o.thinking)!
      if (o.type === 'image' || o.type === 'image_url' || o.type === 'input_image') return `[${imageLabel(o)}]`
      return pretty(o)
    }).filter(Boolean).join('\n')
  }
  return pretty(content)
}

function blocksText(blocks: PayloadBlock[]): string {
  return blocks
    .map((block) => {
      if (block.kind === 'text' || block.kind === 'thinking') return block.text
      if (block.kind === 'tool_result') return block.text
      if (block.kind === 'tool_use') return block.input
      if (block.kind === 'image') return `[${block.label}]`
      return block.json
    })
    .filter(Boolean)
    .join('\n')
}

function messageFromRaw(raw: unknown): PayloadMessage {
  const o = asDict(raw) ?? {}
  const blocks: PayloadBlock[] = []
  const reasoning = str(o.reasoning_content) ?? str(o.reasoning)
  if (reasoning) blocks.push({ kind: 'thinking', text: reasoning })
  blocks.push(...blocksFromContent(o.content))
  // OpenAI assistant 消息里的工具调用不在 content 里
  if (Array.isArray(o.tool_calls)) {
    for (const call of o.tool_calls) {
      const c = asDict(call) ?? {}
      const fn = asDict(c.function) ?? {}
      blocks.push({
        kind: 'tool_use',
        name: str(fn.name) ?? 'tool',
        id: str(c.id),
        input: toolInputText(fn.arguments),
      })
    }
  }
  return {
    role: str(o.role) ?? 'unknown',
    name: str(o.name),
    toolCallId: str(o.tool_call_id),
    blocks,
  }
}

function parseMessages(json: Dict): PayloadMessage[] {
  if (!Array.isArray(json.messages)) return []
  return json.messages.map(messageFromRaw)
}

function parseTools(json: Dict): PayloadTool[] {
  if (!Array.isArray(json.tools)) return []
  return json.tools.map((raw) => {
    const o = asDict(raw) ?? {}
    // OpenAI: {type:'function', function:{name,description,parameters}}；Anthropic: 顶层字段
    const fn = asDict(o.function) ?? o
    const schema = fn.parameters ?? fn.input_schema
    return {
      name: str(fn.name) ?? 'tool',
      description: str(fn.description),
      schema: schema ? pretty(schema) : undefined,
    }
  })
}

function requestParams(json: Dict): PayloadKeyValue[] {
  const entries: PayloadKeyValue[] = []
  const add = (name: string, value: unknown, skip?: boolean) => {
    if (skip || value == null || value === '') return
    entries.push({ name, value: typeof value === 'object' ? pretty(value) : String(value) })
  }
  add('model', json.model)
  add('stream', json.stream)
  add('temperature', json.temperature)
  add('top_p', json.top_p)
  add('max_tokens', json.max_tokens)
  add('max_completion_tokens', json.max_completion_tokens)
  add('n', json.n)
  add('stop', json.stop)
  add('tool_choice', json.tool_choice)
  add('response_format', json.response_format)
  add('reasoning_effort', json.reasoning_effort)
  add('reasoning', json.reasoning)
  add('thinking', json.thinking)
  add('metadata', json.metadata)
  return entries
}

export function parseRequestPayload(
  body: string,
  truncated: boolean,
  _protocol?: string | null,
): ParsedRequest {
  const raw = body
  const text = body.replace(/^\uFEFF/, '')
  if (!text.trim()) {
    return { stream: false, messages: [], tools: [], params: [], raw, truncated, parseError: 'empty' }
  }
  let json: Dict
  try {
    json = JSON.parse(text) as Dict
  } catch {
    return { stream: false, messages: [], tools: [], params: [], raw, truncated, parseError: 'invalid-json' }
  }

  const allMessages = parseMessages(json)
  const systemFromMessages = allMessages
    .filter((message) => message.role === 'system')
    .map((message) => blocksText(message.blocks))
    .filter(Boolean)
    .join('\n')
  const systemField = typeof json.system === 'string'
    ? json.system
    : Array.isArray(json.system)
      ? blocksText(blocksFromContent(json.system))
      : undefined

  return {
    model: str(json.model),
    stream: json.stream === true,
    system: systemField || systemFromMessages || undefined,
    messages: allMessages.filter((message) => message.role !== 'system'),
    tools: parseTools(json),
    params: requestParams(json),
    raw,
    truncated,
  }
}

function usageEntries(usage: unknown): PayloadKeyValue[] {
  const o = asDict(usage)
  if (!o) return []
  const entries: PayloadKeyValue[] = []
  const add = (name: string, value: unknown) => {
    if (typeof value === 'number' && Number.isFinite(value)) {
      entries.push({ name, value: String(value) })
    }
  }
  const known = [
    'prompt_tokens', 'completion_tokens', 'total_tokens',
    'input_tokens', 'output_tokens',
    'cache_read_input_tokens', 'cache_creation_input_tokens',
    'prompt_cache_hit_tokens', 'prompt_cache_miss_tokens', 'cached_tokens',
  ]
  for (const name of known) add(name, o[name])
  for (const detail of ['prompt_tokens_details', 'completion_tokens_details']) {
    const nested = asDict(o[detail])
    if (nested) {
      for (const [key, value] of Object.entries(nested)) add(`${detail}.${key}`, value)
    }
  }
  // 未知供应商的数值字段也保留，但不再展开嵌套对象
  for (const [key, value] of Object.entries(o)) {
    if (typeof value === 'number' && !entries.some((entry) => entry.name === key)) add(key, value)
  }
  return entries
}

/** Anthropic 的 message_start 给输入、message_delta 给输出：按字段合并而不是整份替换。 */
function mergeUsage(base: PayloadKeyValue[], extra: PayloadKeyValue[]): PayloadKeyValue[] {
  if (!extra.length) return base
  const merged = new Map(base.map((entry) => [entry.name, entry.value]))
  for (const entry of extra) merged.set(entry.name, entry.value)
  return [...merged.entries()].map(([name, value]) => ({ name, value }))
}

function responseFromJson(json: Dict, raw: string, truncated: boolean): ParsedResponse {
  const blocks: PayloadBlock[] = []
  let finishReason: string | undefined

  // Anthropic：顶层 content 块数组 + stop_reason
  if (Array.isArray(json.content)) {
    blocks.push(...json.content.flatMap(blocksFromPart))
    finishReason = str(json.stop_reason)
  }

  // OpenAI：choices[].message / legacy completions 的 choices[].text
  if (Array.isArray(json.choices)) {
    for (const rawChoice of json.choices) {
      const choice = asDict(rawChoice) ?? {}
      const message = asDict(choice.message) ?? asDict(choice.delta)
      if (message) {
        const reasoning = str(message.reasoning_content) ?? str(message.reasoning)
        if (reasoning) blocks.push({ kind: 'thinking', text: reasoning })
        blocks.push(...blocksFromContent(message.content))
        if (Array.isArray(message.tool_calls)) {
          for (const rawCall of message.tool_calls) {
            const call = asDict(rawCall) ?? {}
            const fn = asDict(call.function) ?? {}
            blocks.push({
              kind: 'tool_use',
              name: str(fn.name) ?? 'tool',
              id: str(call.id),
              input: toolInputText(fn.arguments),
            })
          }
        }
      } else if (str(choice.text)) {
        blocks.push({ kind: 'text', text: str(choice.text)! })
      }
      finishReason = finishReason ?? str(choice.finish_reason)
    }
  }

  // Responses API 风格的 output 数组（有就顺手解析，没有不影响）
  if (Array.isArray(json.output)) {
    for (const rawItem of json.output) {
      const item = asDict(rawItem) ?? {}
      if (Array.isArray(item.content)) blocks.push(...item.content.flatMap(blocksFromPart))
      else if (str(item.text)) blocks.push({ kind: 'text', text: str(item.text)! })
    }
  }

  if (!blocks.length && json.error) {
    blocks.push({ kind: 'unknown', label: 'error', json: pretty(json.error) })
  }

  return {
    stream: false,
    model: str(json.model),
    blocks,
    finishReason,
    usage: usageEntries(json.usage),
    raw,
    truncated,
    parseError: blocks.length ? undefined : 'unknown-format',
  }
}

interface ToolAccumulator {
  id?: string
  name?: string
  input: string
}

/** 流式解析状态：OpenAI 与 Anthropic 的增量都收敛成 thinking / text / tool_use 三类。 */
function parseSseResponse(raw: string, truncated: boolean, protocol?: string | null): ParsedResponse {
  const lines = raw.split(/\r?\n/)
  let dataLines: string[] = []
  let eventName = ''
  let events = 0
  let model: string | undefined
  let finishReason: string | undefined
  let usage: PayloadKeyValue[] = []
  let text = ''
  let thinking = ''
  const tools = new Map<number, ToolAccumulator>()
  const anthropicBlocks = new Map<number, { type: string; text: string; tool?: ToolAccumulator; raw?: Dict }>()
  let sawAnthropic = false
  let sawOpenAi = false

  const flushEvent = () => {
    if (!dataLines.length) {
      eventName = ''
      return
    }
    const data = dataLines.join('\n').trim()
    dataLines = []
    const name = eventName
    eventName = ''
    if (!data || data === '[DONE]') return
    let json: Dict
    try {
      json = JSON.parse(data) as Dict
    } catch {
      // 截断的最后一帧可能只有半行 JSON；跳过即可，前面完整的部分仍然可用
      return
    }
    events += 1
    if (json.model) model = String(json.model)

    if (Array.isArray(json.choices)) {
      sawOpenAi = true
      for (const rawChoice of json.choices) {
        const choice = asDict(rawChoice) ?? {}
        const delta = asDict(choice.delta) ?? asDict(choice.message) ?? {}
        const reasoning = str(delta.reasoning_content) ?? str(delta.reasoning)
        if (reasoning) thinking += reasoning
        const content = delta.content
        if (typeof content === 'string') text += content
        else if (Array.isArray(content)) {
          text += content.map((part) => typeof part === 'string' ? part : str(asDict(part)?.text) ?? '').join('')
        }
        if (Array.isArray(delta.tool_calls)) {
          for (const rawCall of delta.tool_calls) {
            const call = asDict(rawCall) ?? {}
            const index = Number(call.index ?? 0) || 0
            const acc = tools.get(index) ?? { id: undefined, name: undefined, input: '' }
            if (str(call.id)) acc.id = str(call.id)
            const fn = asDict(call.function)
            if (str(fn?.name)) acc.name = str(fn?.name)
            if (typeof fn?.arguments === 'string') acc.input += fn.arguments
            else if (fn?.arguments != null) acc.input += JSON.stringify(fn.arguments)
            tools.set(index, acc)
          }
        }
        if (str(choice.finish_reason)) finishReason = str(choice.finish_reason)
      }
      if (json.usage) usage = mergeUsage(usage, usageEntries(json.usage))
      if (json.error) {
        const err = asDict(json.error)
        thinking += `\n[error] ${str(err?.message) ?? pretty(json.error)}`
      }
      return
    }

    const type = str(json.type) ?? name
    if (type.startsWith('content_block_') || type === 'message_start' || type === 'message_delta' || type === 'message_stop' || type === 'error') {
      sawAnthropic = true
      if (type === 'message_start') {
        const message = asDict(json.message) ?? {}
        if (str(message.model)) model = str(message.model)
        usage = mergeUsage(usage, usageEntries(message.usage))
      } else if (type === 'content_block_start') {
        const index = Number(json.index ?? 0) || 0
        const block = asDict(json.content_block) ?? {}
        const blockType = str(block.type) ?? 'unknown'
        const state = {
          type: blockType,
          text: str(block.text) ?? str(block.thinking) ?? '',
          tool: blockType === 'tool_use'
            ? { id: str(block.id), name: str(block.name), input: toolInputText(block.input) === '{}' ? '' : toolInputText(block.input) }
            : undefined,
          raw: block,
        }
        anthropicBlocks.set(index, state)
      } else if (type === 'content_block_delta') {
        const index = Number(json.index ?? 0) || 0
        const delta = asDict(json.delta) ?? {}
        const state = anthropicBlocks.get(index)
        if (!state) return
        if (str(delta.text)) state.text += str(delta.text)
        else if (str(delta.thinking)) state.text += str(delta.thinking)
        else if (typeof delta.partial_json === 'string' && state.tool) state.tool.input += delta.partial_json
      } else if (type === 'message_delta') {
        const delta = asDict(json.delta) ?? {}
        if (str(delta.stop_reason)) finishReason = str(delta.stop_reason)
        usage = mergeUsage(usage, usageEntries(json.usage))
      } else if (type === 'error') {
        const err = asDict(json.error)
        thinking += `\n[error] ${str(err?.message) ?? pretty(json.error)}`
      }
      return
    }

    // 未知事件：不参与结构化重建，但事件已被计入
  }

  for (const line of lines) {
    if (line === '') {
      flushEvent()
      continue
    }
    if (line.startsWith('event:')) {
      eventName = line.slice(6).trim()
      continue
    }
    if (line.startsWith('data:')) {
      dataLines.push(line.slice(5).trimStart())
    }
  }
  // 末尾没有空行（截断或供应商省略）时也尝试消费最后一段
  flushEvent()

  const blocks: PayloadBlock[] = []
  if (sawAnthropic) {
    for (const index of [...anthropicBlocks.keys()].sort((a, b) => a - b)) {
      const state = anthropicBlocks.get(index)!
      if (state.type === 'text') {
        if (state.text) blocks.push({ kind: 'text', text: state.text })
      } else if (state.type === 'thinking') {
        if (state.text) blocks.push({ kind: 'thinking', text: state.text })
      } else if (state.type === 'tool_use') {
        blocks.push({
          kind: 'tool_use',
          name: state.tool?.name ?? 'tool',
          id: state.tool?.id,
          input: toolInputText(state.tool?.input ?? ''),
        })
      } else if (state.text) {
        blocks.push({ kind: 'text', text: state.text })
      } else {
        blocks.push({ kind: 'unknown', label: state.type, json: pretty(state.raw) })
      }
    }
  }
  // OpenAI 的 thinking/text 没有块序号：思考在前、正文在后
  if (sawOpenAi || (!sawAnthropic && (thinking || text || tools.size))) {
    if (thinking) blocks.push({ kind: 'thinking', text: thinking })
    if (text) blocks.push({ kind: 'text', text })
    for (const index of [...tools.keys()].sort((a, b) => a - b)) {
      const tool = tools.get(index)!
      blocks.push({
        kind: 'tool_use',
        name: tool.name ?? 'tool',
        id: tool.id,
        input: toolInputText(tool.input),
      })
    }
  }

  const parseError = blocks.length
    ? undefined
    : events > 0
      ? undefined
      : (truncated ? 'invalid-json' : 'unknown-format')

  return {
    stream: true,
    model,
    blocks,
    finishReason,
    usage,
    events,
    raw,
    truncated,
    parseError,
  }
}

export function parseResponsePayload(
  body: string,
  contentType: string | null | undefined,
  truncated: boolean,
  protocol?: string | null,
): ParsedResponse {
  const raw = body
  const text = body.replace(/^\uFEFF/, '')
  if (!text.trim()) {
    return { stream: false, blocks: [], usage: [], raw, truncated, parseError: 'empty' }
  }
  const isSse = Boolean(contentType && contentType.toLowerCase().includes('text/event-stream'))
    || /^\s*(data:|event:)/m.test(text)
  if (isSse) return parseSseResponse(text, truncated, protocol)

  if (/^\s*[{[]/.test(text)) {
    try {
      const json = JSON.parse(text) as Dict
      return responseFromJson(json, raw, truncated)
    } catch {
      return {
        stream: false, blocks: [], usage: [], raw, truncated,
        parseError: truncated ? 'invalid-json' : 'unknown-format',
      }
    }
  }

  // 纯文本上游（少见，但不要把它误报成解析失败）
  return {
    stream: false,
    blocks: [{ kind: 'text', text: body }],
    usage: [],
    raw,
    truncated,
  }
}

/** 原始内容展示用：能当 JSON 解析就缩进，SSE/纯文本原样保留。 */
export function prettyRawBody(body: string): string {
  const trimmed = body.replace(/^\uFEFF/, '').trim()
  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) return body
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2)
  } catch {
    return body
  }
}
