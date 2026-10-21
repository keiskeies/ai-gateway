import type { ComponentType } from 'react'
import {
  IconActivity,
  IconCloudShare,
  IconDeviceDesktop,
  IconKey,
  IconLayoutDashboard,
  IconSettings,
  IconStack2,
  IconTerminal2,
} from '@tabler/icons-react'
import type { TranslationKey } from '../i18n'

export type NavIcon = ComponentType<{ size?: number; stroke?: number }>

export interface NavItem {
  path: string
  label: TranslationKey
  icon: NavIcon
}

export const NAV_GROUPS: { label: TranslationKey; items: NavItem[] }[] = [
  {
    label: 'navRun',
    items: [
      { path: '/', label: 'overview', icon: IconLayoutDashboard },
      { path: '/traffic', label: 'traffic', icon: IconActivity },
      { path: '/wall', label: 'wall', icon: IconDeviceDesktop },
    ],
  },
  {
    label: 'navConfigure',
    items: [
      { path: '/models', label: 'proxies', icon: IconStack2 },
      { path: '/providers', label: 'platforms', icon: IconCloudShare },
      { path: '/keys', label: 'keys', icon: IconKey },
    ],
  },
  {
    label: 'navTools',
    items: [
      { path: '/playground', label: 'playground', icon: IconTerminal2 },
    ],
  },
  {
    label: 'navSystem',
    items: [
      { path: '/settings', label: 'settings', icon: IconSettings },
    ],
  },
]

export function isNavItemActive(pathname: string, path: string): boolean {
  if (path === '/') return pathname === '/'
  return pathname === path || pathname.startsWith(`${path}/`)
}
