// 只读表格软件导出的那种 CSV：引号包裹、内部引号翻倍、CRLF、可选 BOM。
// 不做流式也不猜编码——非 UTF-8 的文件读进来是乱码，列名匹配那一步会直接报"没找到列"。
export function parseCsv(text: string): string[][] {
  // BOM 来自 Excel 导出的 UTF-8 CSV；CRLF 先归成 LF，引号里的换行按 LF 保留
  const source = text.replace(/^/, '').replace(/\r\n/g, '\n')
  const rows: string[][] = []
  let row: string[] = []
  let cell = ''
  let quoted = false

  for (let i = 0; i < source.length; i++) {
    const ch = source[i]
    if (quoted) {
      if (ch !== '"') {
        cell += ch
      } else if (source[i + 1] === '"') {
        cell += '"'
        i++
      } else {
        quoted = false
      }
      continue
    }
    if (ch === '"') {
      quoted = true
    } else if (ch === ',') {
      row.push(cell)
      cell = ''
    } else if (ch === '\r') {
      // 落单的 CR（老 Mac 换行）当普通字符跳过，CRLF 已在上面归一
    } else if (ch === '\n') {
      row.push(cell)
      rows.push(row)
      row = []
      cell = ''
    } else {
      cell += ch
    }
  }
  if (cell !== '' || row.length > 0) {
    row.push(cell)
    rows.push(row)
  }
  return rows
}
