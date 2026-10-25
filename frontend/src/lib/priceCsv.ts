import { parseCsv } from './csv'

// 定价 CSV 的解析与"要改什么"的预演。整段放在这里而不是弹窗里，是因为这份计划
// 就是唯一真相：界面上的计数、差异列表、发给后端的请求体都从它派生，不会出现
// "预览说改 12 个、实际写了 9 个"。后端只负责落库并回报实际写入数，两边对不上时以它为准。

export type PriceCell =
  | { state: 'keep' }
  | { state: 'clear' }
  | { state: 'set'; value: number }

export interface PricePlanRow {
  modelId: string
  input: PriceCell
  output: PriceCell
  credit: PriceCell
  currentInput: number | null
  currentOutput: number | null
  currentCredit: number | null
}

export interface PricePlanIssue {
  line: number
  detail: string
}

export interface PricePlan {
  error?: 'noModelColumn' | 'noPriceColumn' | 'noRows'
  dataRows: number
  matched: number
  rows: PricePlanRow[]
  unchanged: number
  same: number
  duplicates: number
  unmatched: string[]
  invalid: PricePlanIssue[]
}

export interface PriceModelRow {
  model_id: string
  input_price: number | null
  output_price: number | null
  credit_price: number | null
}

// 列名按归一化后的字符串匹配：小写、去掉空格/下划线/连字符/括号。
// 于是 `Input Price`、`input_price`、`INPUT (USD)` 都落到同一档。
const MODEL_HEADERS = ['modelid', 'model', 'id', '模型', '模型id', '模型名称']
const INPUT_HEADERS = ['inputprice', 'input', 'promptprice', 'prompt', 'in', '输入', '输入价格', '输入单价']
const OUTPUT_HEADERS = ['outputprice', 'output', 'completionprice', 'completion', 'out', '输出', '输出价格', '输出单价']
const CREDIT_HEADERS = ['creditprice', 'credits', 'credit', '积分', '积分价格', '积分单价']

function normalizeHeader(cell: string): string {
  return cell.trim().toLowerCase().replace(/[\s_\-()/（）]/g, '')
}

// 先精确匹配，再退到前缀匹配：厂商表里常见 `Input Price (per 1M tokens)` 这种带单位的列名。
// 价格列跳过含 cache 的表头——缓存 token 通常单独计价，把它当普通输入价会静默算错成本。
function findColumn(headers: string[], aliases: string[], skipCache: boolean): number {
  const usable = (header: string) => header !== '' && (!skipCache || !header.includes('cache'))
  const exact = headers.findIndex(header => usable(header) && aliases.includes(header))
  if (exact >= 0) return exact
  return headers.findIndex(header => usable(header) && aliases.some(alias => header.startsWith(alias)))
}

const NUMERIC = /^\+?(\d+(\.\d*)?|\.\d+)$/

// 换算后按 10 位小数收一下：0.0029 × 1000 在浮点里是 2.8999999999999995，
// 那种数字进库既难看又会让"这次到底改没改"的比较失真。
function scaled(raw: string, factor: number): number | null {
  const value = Number(raw)
  if (!Number.isFinite(value)) return null
  return Math.round(value * factor * 1e10) / 1e10
}

function sameAsCurrent(cell: PriceCell, current: number | null): boolean {
  if (cell.state === 'keep') return true
  if (cell.state === 'clear') return current === null
  return current === cell.value
}

export function buildPricePlan(
  text: string,
  models: PriceModelRow[],
  options: { emptyMeansClear: boolean; factor: number },
): PricePlan {
  const plan: PricePlan = {
    dataRows: 0,
    matched: 0,
    rows: [],
    unchanged: 0,
    same: 0,
    duplicates: 0,
    unmatched: [],
    invalid: [],
  }
  const table = parseCsv(text)
  const headers = (table[0] ?? []).map(normalizeHeader)
  const modelCol = findColumn(headers, MODEL_HEADERS, false)
  const inputCol = findColumn(headers, INPUT_HEADERS, true)
  const outputCol = findColumn(headers, OUTPUT_HEADERS, true)
  const creditCol = findColumn(headers, CREDIT_HEADERS, true)
  if (modelCol < 0) return { ...plan, error: 'noModelColumn' }
  if (inputCol < 0 && outputCol < 0 && creditCol < 0) return { ...plan, error: 'noPriceColumn' }

  const byId = new Map(models.map(model => [model.model_id, model]))
  const seen = new Set<string>()

  for (let index = 1; index < table.length; index++) {
    const cells = table[index]
    if (cells.every(cell => cell.trim() === '')) continue
    plan.dataRows += 1

    // 导出时以 = + - @ 开头的模型 ID 会被补一个撇号（防止表格软件当公式执行），读回来时去掉
    const modelId = (cells[modelCol] ?? '').trim().replace(/^'/, '')
    if (!modelId) {
      plan.invalid.push({ line: index + 1, detail: (cells[modelCol] ?? '').trim() || '—' })
      continue
    }
    const model = byId.get(modelId)
    if (!model) {
      if (!plan.unmatched.includes(modelId)) plan.unmatched.push(modelId)
      continue
    }
    plan.matched += 1
    if (seen.has(modelId)) plan.duplicates += 1
    seen.add(modelId)

    const parsed: Record<'input' | 'output' | 'credit', PriceCell | null> = { input: null, output: null, credit: null }
    for (const [side, column] of [['input', inputCol], ['output', outputCol], ['credit', creditCol]] as const) {
      if (column < 0) {
        parsed[side] = { state: 'keep' }
        continue
      }
      const raw = (cells[column] ?? '').trim()
      if (raw === '') {
        parsed[side] = options.emptyMeansClear ? { state: 'clear' } : { state: 'keep' }
        continue
      }
      if (!NUMERIC.test(raw)) {
        plan.invalid.push({ line: index + 1, detail: `${modelId} · ${raw}` })
        parsed[side] = null
        continue
      }
      // 积分单价是“每次请求”的单位，不跟 token 单价一起按千/百万换算
      const value = side === 'credit' ? Number(raw) : scaled(raw, options.factor)
      if (value === null || !Number.isFinite(value) || value < 0) {
        plan.invalid.push({ line: index + 1, detail: `${modelId} · ${raw}` })
        parsed[side] = null
      } else {
        parsed[side] = { state: 'set', value }
      }
    }
    // 一行里任何一侧读不出来就整行跳过：只写一半的价格会造出"只填一侧＝整单不估算"的模型，
    // 那种行在成本列里显示 —，用户看不出是导入失败还是本来就没定价。
    if (parsed.input === null || parsed.output === null || parsed.credit === null) continue

    const row: PricePlanRow = {
      modelId,
      input: parsed.input,
      output: parsed.output,
      credit: parsed.credit,
      currentInput: model.input_price,
      currentOutput: model.output_price,
      currentCredit: model.credit_price,
    }
    if (row.input.state === 'keep' && row.output.state === 'keep' && row.credit.state === 'keep') {
      plan.unchanged += 1
      continue
    }
    // 数值与库里一模一样的行不进 payload：写下去是空写，
    // 而且会让"预览说改 N 个"和实际变化的行数对不上。
    if (
      sameAsCurrent(row.input, model.input_price) &&
      sameAsCurrent(row.output, model.output_price) &&
      sameAsCurrent(row.credit, model.credit_price)
    ) {
      plan.same += 1
      continue
    }
    plan.rows.push(row)
  }

  if (plan.dataRows === 0) return { ...plan, error: 'noRows' }
  return plan
}

// 发给后端的请求体：字段不出现＝不改、null＝清空、数字＝设值——与单行/批量编辑同一套三态。
export function pricePlanPayload(rows: PricePlanRow[]) {
  return rows.map(row => {
    const item: Record<string, unknown> = { model_id: row.modelId }
    if (row.input.state !== 'keep') item.input_price = row.input.state === 'clear' ? null : row.input.value
    if (row.output.state !== 'keep') item.output_price = row.output.state === 'clear' ? null : row.output.value
    if (row.credit.state !== 'keep') item.credit_price = row.credit.state === 'clear' ? null : row.credit.value
    return item
  })
}
