// Recharts 画出来的是 path，屏幕阅读器读不到数值。这里给每张图配一张只给 AT 看的表：
// 视觉上一个像素都不占（.console-sr-only），图表本身一字不变。
export function ChartAltTable({ caption, head, rows }: {
  caption: string
  head: string[]
  rows: (string | number)[][]
}) {
  return (
    <table className="console-sr-only">
      <caption>{caption}</caption>
      <thead>
        <tr>{head.map((label, i) => <th key={i} scope="col">{label}</th>)}</tr>
      </thead>
      <tbody>
        {rows.map((row, i) => (
          <tr key={i}>{row.map((cell, j) => <td key={j}>{cell}</td>)}</tr>
        ))}
      </tbody>
    </table>
  )
}

export function peakOf<T>(rows: T[], valueOf: (row: T) => number): T | undefined {
  return rows.reduce((best, row) => (best === undefined || valueOf(row) > valueOf(best) ? row : best), undefined as T | undefined)
}
