import ReactDOM from 'react-dom/client'
import { MantineProvider, createTheme } from '@mantine/core'
import { Notifications } from '@mantine/notifications'
import { HashRouter } from 'react-router-dom'
import { AppProvider, useAppContext } from './ThemeContext'
import App from './App'
import { initConfig } from './api'
import '@mantine/core/styles.css'
import '@mantine/notifications/styles.css'
import '@mantine/charts/styles.css'
import './main.css'

initConfig()

const theme = createTheme({ primaryColor: 'blue', defaultRadius: 'md' })

function ThemedApp() {
  const { isDark } = useAppContext()
  return (
    <MantineProvider theme={theme} forceColorScheme={isDark ? 'dark' : 'light'}>
      <Notifications position="top-right" />
      <HashRouter>
        <App />
      </HashRouter>
    </MantineProvider>
  )
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <AppProvider>
    <ThemedApp />
  </AppProvider>,
)
