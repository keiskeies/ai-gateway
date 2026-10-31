import { notifications, type NotificationData } from '@mantine/notifications'
import { t, type Locale } from '../i18n'

/**
 * 应用内统一的提示出口。
 *
 * Mantine 的 toast 关闭按钮只有一个 × 图标、没有可访问名，axe 按 `button-name` 判违规；
 * 页面上每条提示都要走这里，不然无障碍只能靠"审计时恰好没有 toast 在屏"糊过去。
 */
export function notify(locale: Locale, options: NotificationData): string {
  return notifications.show({
    closeButtonProps: { 'aria-label': t(locale, 'close') },
    ...options,
  })
}
