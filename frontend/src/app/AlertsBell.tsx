import { useState } from 'react'
import { ActionIcon, Popover, ScrollArea } from '@mantine/core'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { IconBell } from '@tabler/icons-react'
import { useNavigate } from 'react-router-dom'
import { listAlerts, readAlert, readAllAlerts, type AlertItem } from '../api'
import { t, type TranslationKey } from '../i18n'
import { useAppContext } from '../ThemeContext'
import { formatLogTime } from '../lib/format'

/**
 * 顶栏铃铛：通知中心。
 *
 * 告警由 `GET /api/alerts` 读时扫描得到（见 src/db/alert.rs），所以这里不订阅"告警事件流"，
 * 只是在 SSE 信号或用户点开时重新拉一次；已读状态是持久的，刷新页面不会又变成未读。
 */

// 告警必须一步到达出问题的对象，所以按 ref_type 映射到各自的详情页
const REF_PATH: Record<string, string> = { platform: '/providers', proxy: '/models', api_key: '/keys', settings: '/settings' }

export function AlertsBell() {
  const { locale } = useAppContext()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [opened, setOpened] = useState(false)
  const alertsQuery = useQuery({ queryKey: ['alerts'], queryFn: listAlerts })
  const refresh = () => queryClient.invalidateQueries({ queryKey: ['alerts'] })

  const items = alertsQuery.data?.items ?? []
  const unread = alertsQuery.data?.unread ?? 0
  // 徽标上的可见数字必须出现在按钮的可访问名里，否则读屏念到的和看到的不是一回事
  const badge = unread > 0 ? (unread > 9 ? '9+' : String(unread)) : ''

  const markRead = async (item: AlertItem) => {
    try {
      await readAlert(item.dedupe_key)
    } catch {
      // 行可能刚被这次扫描标成已解决；下一轮刷新自然对上
    }
    void refresh()
  }

  const jumpTo = (item: AlertItem) => {
    void markRead(item)
    const base = item.ref_type ? REF_PATH[item.ref_type] : undefined
    // 有些告警（备份过期）指向的是页面而不是某一行，没有 ref_id 也要能跳
    if (base) navigate(item.ref_id ? `${base}/${item.ref_id}` : base)
    setOpened(false)
  }

  const markAll = async () => {
    try {
      await readAllAlerts()
    } catch {
      // 已读失败不该挡住面板，重开就是
    }
    void refresh()
  }

  return (
    <Popover
      opened={opened}
      onChange={setOpened}
      position="bottom-end"
      width={340}
      withArrow
      shadow="md"
      classNames={{ dropdown: 'console-alerts-dropdown' }}
    >
      <Popover.Target>
        <ActionIcon
          className="console-bell"
          variant="subtle"
          size="lg"
          aria-label={unread > 0 ? t(locale, 'alertsBellUnread', badge) : t(locale, 'alertsTitle')}
          title={unread > 0 ? t(locale, 'alertsUnread', badge) : t(locale, 'alertsEmpty')}
          // 鼠标点击由 Mantine 的浮层接管，键盘 Enter/Space 走这里：两条通路都要留着
          onClick={() => setOpened(v => !v)}
        >
          <span className="console-bell-wrap">
            <IconBell size={18} />
            {unread > 0 && <span className="console-bell-badge">{badge}</span>}
          </span>
        </ActionIcon>
      </Popover.Target>
      <Popover.Dropdown>
        <div className="console-alerts-head">
          <div>
            <div className="console-alerts-title">{t(locale, 'alertsTitle')}</div>
            {unread > 0 && <div className="console-alerts-sub">{t(locale, 'alertsUnread', String(unread))}</div>}
          </div>
          {unread > 0 && (
            <button type="button" className="console-alerts-action" onClick={() => void markAll()}>
              {t(locale, 'alertsMarkAllRead')}
            </button>
          )}
        </div>
        {alertsQuery.isError ? (
          <div className="console-note warn">
            {t(locale, 'alertsUnavailable')}
            <button type="button" className="console-alerts-action" style={{ marginLeft: 8 }} onClick={() => void refresh()}>
              {t(locale, 'retry')}
            </button>
          </div>
        ) : items.length === 0 ? (
          <div className="console-alerts-empty">{t(locale, 'alertsEmpty')}</div>
        ) : (
          <ScrollArea mah={320} type="auto">
            {items.map(item => (
              <div key={item.dedupe_key} className={`console-alert${item.severity === 'error' ? ' is-error' : ''}${item.read_at ? '' : ' is-unread'}`}>
                <span className={`console-dot ${item.severity === 'error' ? 'err' : 'warn'}`} />
                <div className="console-alert-body">
                  <div className="console-alert-text">{t(locale, item.code as TranslationKey, ...item.args)}</div>
                  <div className="console-alert-meta">
                    {formatLogTime(item.first_seen, locale)}
                    {item.ref_type && (
                      <button type="button" className="console-alerts-action" onClick={() => jumpTo(item)}>
                        {t(locale, 'alertsView')}
                      </button>
                    )}
                    {!item.read_at && (
                      <button type="button" className="console-alerts-action" onClick={() => void markRead(item)}>
                        {t(locale, 'alertsMarkRead')}
                      </button>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </ScrollArea>
        )}
        <div className="console-alerts-foot">{t(locale, 'alertsHint')}</div>
      </Popover.Dropdown>
    </Popover>
  )
}
