import { Suspense, useEffect, useState } from 'react'
import {
  ActionIcon,
  AppShell,
  Burger,
  Group,
  Menu,
  Tooltip,
} from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import {
  IconBook,
  IconBrandGithub,
  IconDeviceDesktop,
  IconLanguage,
  IconLogout,
  IconMoon,
  IconSettings,
  IconSun,
  IconUserCircle,
} from '@tabler/icons-react'
import { Outlet, useLocation, useNavigate } from 'react-router-dom'
import { logout } from '../api'
import { t } from '../i18n'
import { useAppContext } from '../ThemeContext'
import { CommandPalette } from './CommandPalette'
import { HelpModal } from './HelpModal'
import { NAV_GROUPS, isNavItemActive } from './nav'
import { TIME_RANGES, useAppState } from './AppState'
import { useLive } from './LiveContext'
import { PageSkeleton } from '../components/ui/States'
import { AlertsBell } from './AlertsBell'

const APP_VERSION = `v${__APP_VERSION__}`

export function ConsoleShell({
  username,
  adminPort,
  onLoggedOut,
}: {
  username: string
  adminPort: number
  onLoggedOut: () => void
}) {
  const navigate = useNavigate()
  const location = useLocation()
  const { themeMode, setThemeMode, locale, setLocale } = useAppContext()
  const { range, setRange, live, setLive } = useAppState()
  const { status } = useLive()
  const [mobileOpened, { toggle: toggleMobile }] = useDisclosure()
  const [desktopOpened, { toggle: toggleDesktop }] = useDisclosure(true)
  const [docOpen, { open: openDoc, close: closeDoc }] = useDisclosure(false)
  const [paletteOpen, setPaletteOpen] = useState(false)

  const activeGroup = NAV_GROUPS.find(group =>
    group.items.some(item => isNavItemActive(location.pathname, item.path)),
  ) || NAV_GROUPS[0]
  const activeItem = activeGroup.items.find(item =>
    isNavItemActive(location.pathname, item.path),
  ) || activeGroup.items[0]

  const showDataControls = location.pathname === '/'
    || location.pathname.startsWith('/traffic')
    || location.pathname.startsWith('/models')
    || location.pathname.startsWith('/providers/')
    || location.pathname === '/keys'
    || location.pathname.startsWith('/keys/')

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setPaletteOpen(open => !open)
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [])

  useEffect(() => {
    window.scrollTo(0, 0)
    document.querySelector('.console-main')?.scrollTo(0, 0)
  }, [location.pathname])

  const handleLogout = async () => {
    try {
      await logout()
    } catch {
      // 会话可能已过期，直接回到登录页
    }
    onLoggedOut()
  }

  return (
    <AppShell
      header={{ height: 48 }}
      navbar={{ width: 232, breakpoint: 'sm', collapsed: { mobile: !mobileOpened, desktop: !desktopOpened } }}
      padding={0}
      className="console-shell"
    >
      <AppShell.Header className="console-topbar">
        <div className="console-topbar-inner">
          <div className="console-topbar-left">
            <Burger opened={mobileOpened} onClick={toggleMobile} hiddenFrom="sm" size="sm" aria-label={t(locale, 'navToggle')} />
            <Burger opened={desktopOpened} onClick={toggleDesktop} visibleFrom="sm" size="sm" aria-label={t(locale, 'navToggle')} />
            <div className="console-crumbs">
              <span>{t(locale, activeGroup.label)}</span>
              <span>/</span>
              <strong>{t(locale, activeItem.label)}</strong>
            </div>
          </div>
          <div className="console-topbar-right">
            {showDataControls && (
              <>
                <Group gap={2} className="console-range" aria-label={t(locale, 'timeRange')}>
                  {TIME_RANGES.map(option => (
                    <button
                      key={option.key}
                      type="button"
                      className={range === option.key ? 'active' : ''}
                      title={t(locale, option.labelKey)}
                      onClick={() => setRange(option.key)}
                    >
                      {option.key.toUpperCase()}
                    </button>
                  ))}
                </Group>
                <button
                  type="button"
                  className={`console-live${live ? '' : ' paused'}`}
                  aria-pressed={live}
                  title={live
                    ? t(locale, status === 'live' ? 'liveStreamHint' : status === 'connecting' ? 'liveConnectingHint' : 'livePollingHint')
                    : t(locale, 'livePausedHint')}
                  onClick={() => setLive(!live)}
                >
                  {/* 三个态要分得开：连上了、正在重连、以及重连期间退回轮询 */}
                  <span className={`console-dot ${live ? (status === 'live' ? 'ok' : 'warn') : 'idle'}`} />
                  {live ? t(locale, status === 'live' ? 'live' : status === 'connecting' ? 'liveConnecting' : 'livePolling') : t(locale, 'paused')}
                </button>
              </>
            )}
            <Tooltip label={`${t(locale, 'commandPalette')} (⌘K)`} withArrow>
              <ActionIcon variant="subtle" size="lg" onClick={() => setPaletteOpen(true)}>
                <span className="console-cmd">⌘K</span>
              </ActionIcon>
            </Tooltip>
            <AlertsBell />
            <Menu shadow="md" width={160}>
              <Menu.Target>
                <ActionIcon variant="subtle" size="lg" aria-label={t(locale, 'themeModeMenu')}>
                  {themeMode === 'dark' ? <IconMoon size={18} /> : themeMode === 'light' ? <IconSun size={18} /> : <IconDeviceDesktop size={18} />}
                </ActionIcon>
              </Menu.Target>
              <Menu.Dropdown>
                <Menu.Item leftSection={<IconSun size={14} />} disabled={themeMode === 'light'} onClick={() => setThemeMode('light')}>{t(locale, 'themeLight')}</Menu.Item>
                <Menu.Item leftSection={<IconMoon size={14} />} disabled={themeMode === 'dark'} onClick={() => setThemeMode('dark')}>{t(locale, 'themeDark')}</Menu.Item>
                <Menu.Item leftSection={<IconDeviceDesktop size={14} />} disabled={themeMode === 'system'} onClick={() => setThemeMode('system')}>{t(locale, 'themeSystem')}</Menu.Item>
              </Menu.Dropdown>
            </Menu>
            <Menu shadow="md" width={120}>
              <Menu.Target>
                <ActionIcon className="console-lang" variant="subtle" size="lg" aria-label={t(locale, 'languageMenu')}>
                  <IconLanguage size={18} />
                </ActionIcon>
              </Menu.Target>
              <Menu.Dropdown>
                <Menu.Item disabled={locale === 'zh'} onClick={() => setLocale('zh')}>中文</Menu.Item>
                <Menu.Item disabled={locale === 'en'} onClick={() => setLocale('en')}>EN</Menu.Item>
              </Menu.Dropdown>
            </Menu>
            <Menu shadow="md" width={200}>
              <Menu.Target>
                <ActionIcon variant="subtle" size="lg" title={username} aria-label={t(locale, 'accountMenu')}>
                  <IconUserCircle size={18} />
                </ActionIcon>
              </Menu.Target>
              <Menu.Dropdown>
                <Menu.Label>{username}</Menu.Label>
                <Menu.Item leftSection={<IconBook size={14} />} onClick={openDoc}>{t(locale, 'documentation')}</Menu.Item>
                <Menu.Item
                  leftSection={<IconBrandGithub size={14} />}
                  onClick={() => window.open('https://github.com/keiskeies/ai-gateway', '_blank', 'noopener,noreferrer')}
                >
                  GitHub
                </Menu.Item>
                <Menu.Item leftSection={<IconSettings size={14} />} onClick={() => navigate('/settings')}>
                  {t(locale, 'accountSettings')}
                </Menu.Item>
                <Menu.Item color="red" leftSection={<IconLogout size={14} />} onClick={handleLogout}>
                  {t(locale, 'logout')}
                </Menu.Item>
              </Menu.Dropdown>
            </Menu>
          </div>
        </div>
      </AppShell.Header>

      <AppShell.Navbar className="console-navbar">
        <div className="console-brand">
          <img src="./logo.png" alt="" className="console-brand-logo" />
          <div className="console-brand-text">
            <div className="console-brand-name">AI Gateway</div>
            <div className="console-brand-sub">{t(locale, 'brandSub')}</div>
          </div>
        </div>
        <nav className="console-nav" aria-label={t(locale, 'navMain')}>
          {NAV_GROUPS.map(group => (
            <div className="console-nav-group" key={group.label}>
              <div className="console-nav-label">{t(locale, group.label)}</div>
              {group.items.map(item => {
                const Icon = item.icon
                const active = isNavItemActive(location.pathname, item.path)
                return (
                  <button
                    key={item.path}
                    type="button"
                    className={`console-nav-item${active ? ' active' : ''}`}
                    onClick={() => {
                      navigate(item.path)
                      if (mobileOpened) toggleMobile()
                    }}
                  >
                    <Icon size={15} stroke={1.7} />
                    <span>{t(locale, item.label)}</span>
                  </button>
                )
              })}
            </div>
          ))}
        </nav>
        <div className="console-nav-foot">
          <div className="console-instance"><span className="console-dot ok" /> {t(locale, 'localInstance')}</div>
          <div>127.0.0.1:{adminPort}</div>
          <div>{APP_VERSION} · {t(locale, 'localSuffix')}</div>
        </div>
      </AppShell.Navbar>

      <AppShell.Main className="console-main">
        <Suspense fallback={<PageSkeleton />}>
          <Outlet />
        </Suspense>
      </AppShell.Main>

      <HelpModal opened={docOpen} onClose={closeDoc} />
      <CommandPalette opened={paletteOpen} onClose={() => setPaletteOpen(false)} />
    </AppShell>
  )
}
