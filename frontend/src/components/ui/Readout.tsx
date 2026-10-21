import type { ReactNode } from 'react'

export function Readout({
  label,
  value,
  unit,
  meta,
  spark,
}: {
  label: string
  value: ReactNode
  unit?: string
  meta?: ReactNode
  spark?: ReactNode
}) {
  return (
    <div className="console-readout">
      <div className="console-readout-label">{label}</div>
      <div className="console-readout-main">
        <span className="console-readout-value">{value}</span>
        {unit && <span className="console-readout-unit">{unit}</span>}
      </div>
      <div className="console-readout-foot">
        <span>{meta}</span>
        {spark}
      </div>
    </div>
  )
}
