import { cloneElement, useId } from 'react'
import type { ReactElement, ReactNode } from 'react'

/**
 * 表单字段外壳：label 与控件靠 id 绑定，而不是只做视觉上的邻居。
 * 读屏焦点停在输入框上时只会念出与它绑定的 label，兄弟节点里的 label 读不到。
 */
export function Field({ label, unit, full, children }: {
  label: ReactNode
  unit?: ReactNode
  full?: boolean
  children: ReactElement<{ id?: string }>
}) {
  const id = useId()
  return (
    <div className={full ? 'console-field console-form-full' : 'console-field'}>
      <label htmlFor={id}>
        {label}
        {unit && <span className="console-field-unit"> · {unit}</span>}
      </label>
      {cloneElement(children, { id: children.props.id ?? id })}
    </div>
  )
}
