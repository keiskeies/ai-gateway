import { ActionIcon, CopyButton, Tooltip } from '@mantine/core'
import { IconCheck, IconCopy } from '@tabler/icons-react'

/**
 * 小号复制图标按钮：复制成功后短暂显示勾号与浅绿底色反馈。
 * 只用于"原样复制一份内容"的低频动作（请求详情各字段）；label 是 aria-label。
 * 直接内联在文本旁，配合 .console-copy-inline（flex）水平对齐。
 */
export function CopyIconButton({ value, label, size = 12 }: { value: string; label: string; size?: number }) {
  return (
    <Tooltip label={label} withArrow>
      <span className="console-copy-btn">
        <CopyButton value={value}>
          {({ copied, copy }) => (
            <ActionIcon
              size="xs"
              variant={copied ? 'light' : 'subtle'}
              color={copied ? 'green' : undefined}
              aria-label={label}
              onClick={copy}
            >
              {copied ? <IconCheck size={size} /> : <IconCopy size={size} />}
            </ActionIcon>
          )}
        </CopyButton>
      </span>
    </Tooltip>
  )
}
