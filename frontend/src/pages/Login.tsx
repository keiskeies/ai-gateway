import { useState } from 'react'
import { ActionIcon, Alert, Button, PasswordInput, Stack, Text, TextInput, Title } from '@mantine/core'
import {
  IconAlertCircle, IconLanguage, IconLock, IconMoon, IconShieldLock, IconSun, IconUser,
} from '@tabler/icons-react'
import { login, setupAdmin } from '../api'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'

interface LoginProps {
  initialized: boolean
  onAuthenticated: (username: string) => void
}

export default function Login({ initialized, onAuthenticated }: LoginProps) {
  const { locale, setLocale, themeMode, setThemeMode } = useAppContext()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)

  const dark = themeMode === 'dark'
  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    setError('')

    const name = username.trim()
    if (name.length < 3) {
      setError(t(locale, 'usernameTooShort'))
      return
    }
    if (password.length < 8) {
      setError(t(locale, 'passwordTooShort'))
      return
    }
    if (!initialized && password !== confirmPassword) {
      setError(t(locale, 'passwordMismatch'))
      return
    }

    setLoading(true)
    try {
      const data = initialized
        ? await login(name, password)
        : await setupAdmin(name, password)
      onAuthenticated(data.username)
    } catch (err: any) {
      const status = err?.response?.status
      const message = err?.response?.data?.error?.message
      if (status === 401) setError(t(locale, 'invalidCredentials'))
      else if (status === 409) setError(t(locale, 'alreadyInitialized'))
      else if (status === 429) setError(t(locale, 'tooManyAttempts'))
      else setError(message || t(locale, 'loginFailed'))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="auth-page">
      <div className="auth-toolbar">
        <ActionIcon
          variant="subtle"
          size="lg"
          onClick={() => setThemeMode(dark ? 'light' : 'dark')}
          title={dark ? t(locale, 'themeLight') : t(locale, 'themeDark')}
        >
          {dark ? <IconSun size={18} /> : <IconMoon size={18} />}
        </ActionIcon>
        <ActionIcon
          variant="subtle"
          size="lg"
          onClick={() => setLocale(locale === 'zh' ? 'en' : 'zh')}
          title={t(locale, 'langEn')}
        >
          <IconLanguage size={18} />
        </ActionIcon>
      </div>

      <div className="auth-card">
        <div className="auth-brand">
          <img src="./logo.png" alt="" className="auth-logo" />
          <div>
            <div className="auth-brand-name">AI Gateway</div>
            <div className="auth-brand-sub">{t(locale, 'appSubtitle')}</div>
          </div>
        </div>

        <div className="auth-heading">
          <span className="auth-shield"><IconShieldLock size={20} stroke={1.7} /></span>
          <div>
            <Title order={3}>{initialized ? t(locale, 'loginTitle') : t(locale, 'setupTitle')}</Title>
            <Text size="sm" c="dimmed">
              {initialized ? t(locale, 'loginSubtitle') : t(locale, 'setupSubtitle')}
            </Text>
          </div>
        </div>

        <form onSubmit={submit}>
          <Stack gap="sm">
            <TextInput
              label={t(locale, 'username')}
              leftSection={<IconUser size={15} />}
              value={username}
              onChange={event => setUsername(event.currentTarget.value)}
              autoComplete="username"
              autoFocus
            />
            <PasswordInput
              label={t(locale, 'password')}
              description={initialized ? undefined : t(locale, 'passwordMinHint')}
              leftSection={<IconLock size={15} />}
              value={password}
              onChange={event => setPassword(event.currentTarget.value)}
              autoComplete={initialized ? 'current-password' : 'new-password'}
            />
            {!initialized && (
              <PasswordInput
                label={t(locale, 'confirmPassword')}
                leftSection={<IconLock size={15} />}
                value={confirmPassword}
                onChange={event => setConfirmPassword(event.currentTarget.value)}
                autoComplete="new-password"
              />
            )}
            {error && (
              <Alert color="red" icon={<IconAlertCircle size={16} />} py={8}>
                {error}
              </Alert>
            )}
            <Button
              type="submit"
              size="md"
              fullWidth
              loading={loading}
              leftSection={<IconLock size={16} />}
              mt={4}
            >
              {initialized ? t(locale, 'login') : t(locale, 'setupAccount')}
            </Button>
          </Stack>
        </form>

        {!initialized && (
          <div className="auth-hint">{t(locale, 'setupHint')}</div>
        )}
      </div>

      <div className="auth-foot">{t(locale, 'authFooter')}</div>
    </div>
  )
}
