import React from 'react'
import ReactDOM from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { ConfigProvider, theme as antTheme } from 'antd'
import zhCN from 'antd/locale/zh_CN'
import enUS from 'antd/locale/en_US'
import { AppProvider, useAppContext } from './ThemeContext'
import App from './App'
import { initConfig } from './api'

// Initialize API config (detect Tauri backend port)
initConfig()

function ThemedApp() {
  const { isDark, locale } = useAppContext()
  return (
    <ConfigProvider
      locale={locale === 'zh' ? zhCN : enUS}
      theme={{
        algorithm: isDark ? antTheme.darkAlgorithm : antTheme.defaultAlgorithm,
        token: { colorPrimary: '#1677ff', borderRadius: 6 },
        ...(isDark ? {
          components: {
            Layout: { colorBgBody: '#141414', colorBgLayout: '#000', colorBgContainer: '#1f1f1f', colorBgElevated: '#262626' },
            Menu: { colorBgContainer: '#1f1f1f', colorItemBgSelected: '#111d2c' },
          },
        } : {}),
      }}
    >
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </ConfigProvider>
  )
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AppProvider>
      <ThemedApp />
    </AppProvider>
  </React.StrictMode>,
)
