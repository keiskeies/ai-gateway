import { useEffect, useMemo, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Button, Checkbox, Modal, ScrollArea, SegmentedControl, Table } from '@mantine/core'
import { IconUpload } from '@tabler/icons-react'
import { notifications } from '@mantine/notifications'
import { importPlatformPrices } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'
import {
  buildPricePlan,
  pricePlanPayload,
  type PriceCell,
  type PriceModelRow,
  type PricePlanRow,
} from '../lib/priceCsv'

// 文件里的价格单位 → 库里的单位（每 100 万 token）。厂商价目表大多按每 1 千 token 写，
// 直接灌进来会让成本差一千倍，所以换算放在导入这一步，而不是让用户自己乘。
const UNIT_FACTORS: Record<string, number> = { per1m: 1, per1k: 1000, per1: 1_000_000 }

const PREVIEW_ROWS = 60

// 定价 CSV 导入：只改本平台已有模型的单价。模型本身用"导入模型"建，
// 显示名 / 能力 / 状态 / 档位一律不碰——那些列每行各不相同，一份价格表没有权限制改它们。
export function ImportPricesModal({
  platformId,
  models,
  opened,
  onClose,
}: {
  platformId: string
  models: PriceModelRow[]
  opened: boolean
  onClose: () => void
}) {
  const queryClient = useQueryClient()
  const { locale } = useAppContext()
  const fileRef = useRef<HTMLInputElement>(null)
  const [fileName, setFileName] = useState('')
  const [text, setText] = useState('')
  const [unit, setUnit] = useState('per1m')
  const [emptyMeansClear, setEmptyMeansClear] = useState(false)
  const [applying, setApplying] = useState(false)

  useEffect(() => {
    if (opened) {
      setFileName('')
      setText('')
      setUnit('per1m')
      setEmptyMeansClear(false)
    }
  }, [opened])

  const plan = useMemo(() => {
    if (!text) return null
    return buildPricePlan(text, models, { emptyMeansClear, factor: UNIT_FACTORS[unit] })
  }, [text, models, emptyMeansClear, unit])

  const pickFile = async (file: File) => {
    setFileName(file.name)
    try {
      setText(await file.text())
    } catch {
      setText('')
      notifications.show({ message: t(locale, 'pricesReadFailed'), color: 'red' })
    }
  }

  const apply = async () => {
    if (!plan || plan.rows.length === 0) return
    setApplying(true)
    try {
      const result = await importPlatformPrices(platformId, pricePlanPayload(plan.rows))
      const written = Number(result?.written ?? 0)
      if (written === plan.rows.length) {
        notifications.show({ message: t(locale, 'pricesApplied', String(written)), color: 'green' })
      } else {
        // 预览与实际不一致通常意味着库在这期间被别人改过（模型行被删）。如实报，不粉饰
        notifications.show({
          message: t(locale, 'pricesAppliedDiff', String(written), String(plan.rows.length)),
          color: 'yellow',
        })
      }
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['platform-models', platformId] }),
        queryClient.invalidateQueries({ queryKey: ['models'] }),
      ])
      onClose()
    } catch (error: any) {
      notifications.show({
        message: t(locale, 'pricesApplyFailed', error?.response?.data?.error?.message || ''),
        color: 'red',
      })
    } finally {
      setApplying(false)
    }
  }

  const shown = plan?.rows.slice(0, PREVIEW_ROWS) ?? []

  return (
    <Modal opened={opened} onClose={onClose} title={t(locale, 'pricesImportTitle')} size="lg">
      <div className="console-note">{t(locale, 'pricesImportIntro')}</div>

      <input
        ref={fileRef}
        type="file"
        accept="text/csv,text/plain,.csv,.txt"
        style={{ display: 'none' }}
        onChange={event => {
          const file = event.currentTarget.files?.[0]
          if (file) void pickFile(file)
          event.currentTarget.value = ''
        }}
      />
      <div className="console-route-actions">
        <Button size="xs" variant="default" leftSection={<IconUpload size={13} />} onClick={() => fileRef.current?.click()}>
          {t(locale, 'pricesChooseFile')}
        </Button>
        {fileName && <span className="console-mono console-plan-file">{fileName}</span>}
      </div>

      <div className="console-plan-mode">
        <span className="console-plan-mode-label">{t(locale, 'pricesUnitLabel')}</span>
        <SegmentedControl
          size="xs"
          data={[
            { value: 'per1m', label: t(locale, 'pricesUnitPer1m') },
            { value: 'per1k', label: t(locale, 'pricesUnitPer1k') },
            { value: 'per1', label: t(locale, 'pricesUnitPer1') },
          ]}
          value={unit}
          onChange={value => setUnit(value)}
        />
      </div>
      <div className="console-hint">{t(locale, 'pricesUnitDesc')}</div>

      <Checkbox
        size="xs"
        mt={10}
        label={t(locale, 'pricesEmptyClear')}
        checked={emptyMeansClear}
        onChange={event => setEmptyMeansClear(event.currentTarget.checked)}
      />
      <div className="console-hint">{t(locale, 'pricesEmptyDesc')}</div>

      {plan && (
        <div className="console-plan">
          <div className="console-plan-head">
            <span>{t(locale, 'pricesPlanTitle')}</span>
            <span className="console-plan-src">{fileName}</span>
          </div>
          {plan.error ? (
            <div className="console-note">
              {plan.error === 'noModelColumn'
                ? t(locale, 'pricesErrNoModelCol')
                : plan.error === 'noPriceColumn'
                  ? t(locale, 'pricesErrNoPriceCol')
                  : t(locale, 'pricesErrNoRows')}
            </div>
          ) : (
            <>
              <div className="console-plan-row head">
                <span />
                <span>{t(locale, 'pricesColMatched')}</span>
                <span>{t(locale, 'pricesColWrite')}</span>
                <span>{t(locale, 'pricesColUnmatched')}</span>
                <span>{t(locale, 'pricesColInvalid')}</span>
              </div>
              <div className="console-plan-row">
                <span className="console-plan-name">{t(locale, 'pricesRowTotal', String(plan.dataRows))}</span>
                <span className="console-plan-num">{plan.matched}</span>
                <span className={plan.rows.length ? 'console-plan-num add' : 'console-plan-num zero'}>{plan.rows.length}</span>
                <span className={plan.unmatched.length ? 'console-plan-num' : 'console-plan-num zero'}>{plan.unmatched.length}</span>
                <span className={plan.invalid.length ? 'console-plan-num drop' : 'console-plan-num zero'}>{plan.invalid.length}</span>
              </div>
              {(plan.same > 0 || plan.unchanged > 0 || plan.duplicates > 0) && (
                <div className="console-note">
                  {t(locale, 'pricesPlanExtra', String(plan.same), String(plan.unchanged), String(plan.duplicates))}
                </div>
              )}
              {plan.unmatched.length > 0 && (
                <div className="console-plan-issues">
                  <div className="console-plan-issues-title warn">{t(locale, 'pricesUnmatchedTitle')}</div>
                  <ul>
                    <li>{plan.unmatched.slice(0, 8).join('、')}{plan.unmatched.length > 8 ? ` … +${plan.unmatched.length - 8}` : ''}</li>
                    <li>{t(locale, 'pricesUnmatchedDesc')}</li>
                  </ul>
                </div>
              )}
              {plan.invalid.length > 0 && (
                <div className="console-plan-issues">
                  <div className="console-plan-issues-title err">{t(locale, 'pricesInvalidTitle')}</div>
                  <ul>
                    {plan.invalid.slice(0, 8).map(issue => (
                      <li key={`i${issue.line}-${issue.detail}`}>{t(locale, 'pricesInvalidLine', String(issue.line), issue.detail)}</li>
                    ))}
                    {plan.invalid.length > 8 && <li>{t(locale, 'pricesInvalidMore', String(plan.invalid.length - 8))}</li>}
                  </ul>
                </div>
              )}
              {shown.length > 0 && (
                <ScrollArea.Autosize mah={260} type="auto">
                  <Table className="console-table">
                    <Table.Thead>
                      <Table.Tr>
                        <Table.Th>{t(locale, 'modelId')}</Table.Th>
                        <Table.Th className="num">{t(locale, 'pricesColIn')}</Table.Th>
                        <Table.Th className="num">{t(locale, 'pricesColOut')}</Table.Th>
                        <Table.Th className="num">{t(locale, 'pricesColCredit')}</Table.Th>
                      </Table.Tr>
                    </Table.Thead>
                    <Table.Tbody>
                      {shown.map(row => (
                        <Table.Tr key={row.modelId}>
                          <Table.Td><span className="console-mono">{row.modelId}</span></Table.Td>
                          <Table.Td className="num">{priceCell(row, 'input')}</Table.Td>
                          <Table.Td className="num">{priceCell(row, 'output')}</Table.Td>
                          <Table.Td className="num">{priceCell(row, 'credit')}</Table.Td>
                        </Table.Tr>
                      ))}
                    </Table.Tbody>
                  </Table>
                </ScrollArea.Autosize>
              )}
              {plan.rows.length > shown.length && (
                <div className="console-note">{t(locale, 'pricesMoreRows', String(plan.rows.length - shown.length))}</div>
              )}
            </>
          )}
        </div>
      )}

      <div className="console-inspector-actions" style={{ padding: '16px 0 0' }}>
        <Button size="xs" variant="default" onClick={onClose}>{t(locale, 'cancel')}</Button>
        <Button size="xs" loading={applying} disabled={!plan || plan.rows.length === 0} onClick={() => void apply()}>
          {t(locale, 'pricesApply', String(plan?.rows.length ?? 0))}
        </Button>
      </div>
    </Modal>
  )

  function priceCell(row: PricePlanRow, side: 'input' | 'output' | 'credit') {
    const cell: PriceCell = side === 'input' ? row.input : side === 'output' ? row.output : row.credit
    const current = side === 'input' ? row.currentInput : side === 'output' ? row.currentOutput : row.currentCredit
    if (cell.state === 'keep') {
      return <span className="muted">{current ?? '—'}</span>
    }
    return (
      <span>
        <span className="muted">{current ?? '—'}</span>
        {' → '}
        <span className={cell.state === 'clear' ? 'muted' : 'console-strong'}>{cell.state === 'clear' ? '—' : cell.value}</span>
      </span>
    )
  }
}
