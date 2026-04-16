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
    return (localStorage.getItem('theme') as ThemeMode) || 'system'
  })
  const [locale, setLocaleState] = useState<Locale>(() => {
    return (localStorage.getItem('locale') as Locale) || 'zh'
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
