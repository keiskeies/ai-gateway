import React, { createContext, useContext, useState, useEffect, useMemo } from 'react'
import type { ThemeMode, Locale } from './i18n'

interface AppContextType {
  themeMode: ThemeMode
  setThemeMode: (m: ThemeMode) => void
  isDark: boolean
  locale: Locale
  setLocale: (l: Locale) => void
}

const AppContext = createContext<AppContextType>({
  themeMode: 'system',
  setThemeMode: () => {},
  isDark: false,
  locale: 'zh',
  setLocale: () => {},
})

export const useAppContext = () => useContext(AppContext)

function getSystemDark(): boolean {
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

export function AppProvider({ children }: { children: React.ReactNode }) {
  const [themeMode, setThemeModeState] = useState<ThemeMode>(() => {
    return (localStorage.getItem('theme') as ThemeMode) || 'dark'
  })
  const [locale, setLocaleState] = useState<Locale>(() => {
    const stored = localStorage.getItem('locale')
    return (stored === 'zh' || stored === 'en') ? stored : 'zh'
  })
  const [systemDark, setSystemDark] = useState(getSystemDark)

  useEffect(() => {
    const mql = window.matchMedia('(prefers-color-scheme: dark)')
    const handler = (e: MediaQueryListEvent) => setSystemDark(e.matches)
    mql.addEventListener('change', handler)
    return () => mql.removeEventListener('change', handler)
  }, [])

  const isDark = useMemo(() => {
    if (themeMode === 'dark') return true
    if (themeMode === 'light') return false
    return systemDark
  }, [themeMode, systemDark])

  useEffect(() => {
    const expected = isDark ? 'dark-mode' : 'light-mode'
    const bg = isDark ? '#0a0a0a' : '#f1f5f9'
    document.documentElement.className = expected
    document.documentElement.style.background = bg
    document.body.style.background = bg
  }, [isDark])

  const setThemeMode = (m: ThemeMode) => {
    setThemeModeState(m)
    localStorage.setItem('theme', m)
  }

  const setLocale = (l: Locale) => {
    setLocaleState(l)
    localStorage.setItem('locale', l)
  }

  return (
    <AppContext.Provider value={{ themeMode, setThemeMode, isDark, locale, setLocale }}>
      {children}
    </AppContext.Provider>
  )
}
