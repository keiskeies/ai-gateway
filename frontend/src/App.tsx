import { lazy, Suspense, useEffect, useState } from 'react'
import { Button, Loader, Stack, Text } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { Navigate, Route, Routes } from 'react-router-dom'
import Login from './pages/Login'
import { ConsoleShell } from './app/ConsoleShell'
import { LiveProvider } from './app/LiveContext'
import { getAuthStatus, type AuthStatus } from './api'
import { useAppContext } from './ThemeContext'
import { t } from './i18n'

const Traffic = lazy(() => import('./pages/Traffic'))
const Models = lazy(() => import('./pages/Models'))
const ModelDetail = lazy(() => import('./pages/ModelDetail'))
const Playground = lazy(() => import('./pages/Playground'))
const Wall = lazy(() => import('./pages/Wall'))
const Platforms = lazy(() => import('./pages/Platforms'))
const ProviderDetail = lazy(() => import('./pages/ProviderDetail'))
const ApiKeys = lazy(() => import('./pages/ApiKeys'))
const KeyDetail = lazy(() => import('./pages/KeyDetail'))
const Settings = lazy(() => import('./pages/Settings'))
const Setup = lazy(() => import('./pages/Setup'))

export default function App() {
  const { locale } = useAppContext()
  const [auth, setAuth] = useState<AuthStatus | null>(null)
  const [authFailed, setAuthFailed] = useState(false)

  const loadAuth = () => {
    setAuthFailed(false)
    getAuthStatus()
      .then(data => setAuth(data))
      .catch(() => setAuthFailed(true))
  }

  useEffect(() => {
    loadAuth()
  }, [])

  useEffect(() => {
    const onUnauthorized = () => {
      notifications.clean()
      setAuth(prev => (prev && prev.authenticated
        ? { ...prev, authenticated: false, username: null }
        : prev))
    }
    window.addEventListener('aigw:unauthorized', onUnauthorized)
    return () => window.removeEventListener('aigw:unauthorized', onUnauthorized)
  }, [])

  if (!auth) {
    return (
      <div className="auth-page">
        {authFailed ? (
          <Stack align="center" gap="sm">
            <Text c="dimmed">{t(locale, 'authLoadFailed')}</Text>
            <Button variant="default" onClick={loadAuth}>{t(locale, 'retry')}</Button>
          </Stack>
        ) : (
          <Loader />
        )}
      </div>
    )
  }

  if (!auth.initialized || !auth.authenticated) {
    return (
      <Login
        initialized={auth.initialized}
        onAuthenticated={username => setAuth({ ...auth, initialized: true, authenticated: true, username })}
      />
    )
  }

  return (
    <Routes>
      <Route
        path="/wall"
        element={(
          <Suspense fallback={<div className="console-route-fallback"><Loader /></div>}>
            <Wall />
          </Suspense>
        )}
      />
      <Route
        path="/setup"
        element={(
          <Suspense fallback={<div className="console-route-fallback"><Loader /></div>}>
            <Setup />
          </Suspense>
        )}
      />
      <Route
        element={(
          <LiveProvider>
            <ConsoleShell
              username={auth.username || ''}
              adminPort={auth.admin_port}
              onLoggedOut={() => setAuth({ ...auth, authenticated: false, username: null })}
            />
          </LiveProvider>
        )}
      >
        <Route path="/" element={<Navigate to="/traffic" replace />} />
        <Route path="/traffic" element={<Traffic />} />
        <Route path="/models" element={<Models />} />
        <Route path="/models/:id" element={<ModelDetail />} />
        <Route path="/providers" element={<Platforms />} />
        <Route path="/providers/:id" element={<ProviderDetail />} />
        <Route path="/keys" element={<ApiKeys />} />
        <Route path="/keys/:id" element={<KeyDetail />} />
        <Route path="/playground" element={<Playground />} />
        <Route path="/settings" element={<Settings />} />
        <Route path="/platforms" element={<Navigate to="/providers" replace />} />
        <Route path="/proxies" element={<Navigate to="/models" replace />} />
        <Route path="/api-keys" element={<Navigate to="/keys" replace />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  )
}
