import { useState, useEffect } from 'react'
import { ConfigProvider, Layout, Typography, Dropdown, Button, Tooltip, Modal, Divider } from 'antd'
import {
  DashboardOutlined, CloudServerOutlined, ApiOutlined,
  SunOutlined, MoonOutlined, DesktopOutlined, GlobalOutlined,
  BookOutlined, SettingOutlined, GithubOutlined, KeyOutlined,
} from '@ant-design/icons'
import { Routes, Route, useNavigate, useLocation } from 'react-router-dom'
import Dashboard from './pages/Dashboard'
import Platforms from './pages/Platforms'
import Proxies from './pages/Proxies'
import ApiKeys from './pages/ApiKeys'
import Settings from './pages/Settings'
import { useAppContext } from './ThemeContext'
import { t, type Locale, type ThemeMode, type TranslationKey } from './i18n'
import { lightTheme, darkTheme } from './theme'
import './styles.css'

const { Content } = Layout
const { Text } = Typography

const TAB_ITEMS = [
  { key: '/', icon: <DashboardOutlined />, label: 'dashboard', color: '#2563eb' },
  { key: '/platforms', icon: <CloudServerOutlined />, label: 'platforms', color: '#7c3aed' },
  { key: '/proxies', icon: <ApiOutlined />, label: 'proxies', color: '#db2777' },
  { key: '/api-keys', icon: <KeyOutlined />, label: 'apiKeys', color: '#d97706' },
  { key: '/settings', icon: <SettingOutlined />, label: 'settings', color: '#64748b' },
]

export default function App() {
  const navigate = useNavigate()
  const location = useLocation()
  const { themeMode, setThemeMode, isDark, locale, setLocale } = useAppContext()

  const themeIcon = themeMode === 'dark' ? <MoonOutlined /> : themeMode === 'light' ? <SunOutlined /> : <DesktopOutlined />
  const [docOpen, setDocOpen] = useState(false)

  const activeIndex = TAB_ITEMS.findIndex(item => item.key === location.pathname)

  return (
    <ConfigProvider theme={isDark ? darkTheme : lightTheme}>
      <Layout>
        <div className="app-header">
          <div className="app-header-inner">
            <div className="app-header-left">
              <img src="./logo.png" alt="" className="app-logo" />
              <div className="app-brand">
                <div className="app-title">AI Gateway</div>
                <div className="app-subtitle">v1.2.1</div>
              </div>
            </div>

            <nav className="app-nav">
              {TAB_ITEMS.map(item => {
                const isActive = location.pathname === item.key
                return (
                  <button
                    key={item.key}
                    className={`app-nav-item ${isActive ? 'active' : ''}`}
                    onClick={() => navigate(item.key)}
                    style={{
                      '--nav-color': item.color,
                      color: isActive ? item.color : undefined,
                    } as React.CSSProperties}
                  >
                    <span className="app-nav-icon" style={{ color: isActive ? item.color : item.color + '99' }}>{item.icon}</span>
                    <span className="app-nav-label">{t(locale, item.label as TranslationKey)}</span>
                  </button>
                )
              })}
            </nav>

            <div className="app-header-right">
              <Tooltip title={t(locale, 'documentation')} placement="bottom">
                <button className="app-action-btn" onClick={() => setDocOpen(true)}>
                  <BookOutlined />
                </button>
              </Tooltip>
              <Dropdown menu={{
                items: [
                  { key: 'light', icon: <SunOutlined />, label: t(locale, 'themeLight'), disabled: themeMode === 'light' },
                  { key: 'dark', icon: <MoonOutlined />, label: t(locale, 'themeDark'), disabled: themeMode === 'dark' },
                  { key: 'system', icon: <DesktopOutlined />, label: t(locale, 'themeSystem'), disabled: themeMode === 'system' },
                ],
                onClick: ({ key }) => setThemeMode(key as ThemeMode),
              }}>
                <button className="app-action-btn">
                  {themeIcon}
                </button>
              </Dropdown>
              <Dropdown menu={{
                items: [
                  { key: 'zh', label: '中文', disabled: locale === 'zh' },
                  { key: 'en', label: 'EN', disabled: locale === 'en' },
                ],
                onClick: ({ key }) => setLocale(key as Locale),
              }}>
                <button className="app-action-btn">
                  <GlobalOutlined />
                </button>
              </Dropdown>
            <Tooltip title="GitHub" placement="bottom">
              <button className="app-action-btn" onClick={() => window.open('https://github.com/keiskeies/ai-gateway', '_blank')}>
                <GithubOutlined />
              </button>
            </Tooltip>
          </div>
          </div>
        </div>

        <Content className="app-content">
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/platforms" element={<Platforms />} />
            <Route path="/proxies" element={<Proxies />} />
            <Route path="/api-keys" element={<ApiKeys />} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </Content>

        <Modal
          title={t(locale, 'documentation')}
          open={docOpen}
          onCancel={() => setDocOpen(false)}
          footer={null}
          width={720}
        >
          {locale === 'zh' ? <DocZh /> : <DocEn />}
        </Modal>
      </Layout>
    </ConfigProvider>
  )
}

function DocZh() {
  return (
    <div style={{ lineHeight: 1.8 }}>
      <Typography.Title level={4}>AI Gateway 使用帮助</Typography.Title>
      <Typography.Paragraph>
        <Text strong>AI Gateway</Text> 是一个跨平台 AI 接口聚合与负载均衡工具，支持 OpenAI、Anthropic、Ollama 等多种 AI 平台的统一接入。
      </Typography.Paragraph>
      <Divider />
      <Typography.Title level={5}>第一步：添加 AI 平台</Typography.Title>
      <Typography.Paragraph>平台是你 AI 模型的来源，比如 OpenAI、Anthropic、NVIDIA 等。每个平台需要配置 API 地址和 API Key。</Typography.Paragraph>
      <ol>
        <li>进入「<Text strong>平台</Text>」页面，点击「<Text strong>添加平台</Text>」</li>
        <li>选择预设平台或手动填写，配置 API Key 后保存</li>
      </ol>
      <Typography.Title level={5}>第二步：添加模型</Typography.Title>
      <Typography.Paragraph>每个平台上可能有多个 AI 模型，你需要把要使用的模型添加进来。</Typography.Paragraph>
      <ol>
        <li>进入「<Text strong>模型</Text>」页面，点击「<Text strong>添加模型</Text>」</li>
        <li>选择平台，选择预设模型或自定义输入模型 ID</li>
      </ol>
      <Typography.Title level={5}>第三步：创建虚拟大模型</Typography.Title>
      <Typography.Paragraph>虚拟大模型的名称即为对外暴露的模型 ID，后端可以挂载多个平台大模型实现负载均衡。默认同时支持 OpenAI 和 Anthropic 协议。</Typography.Paragraph>
      <ol>
        <li>进入「<Text strong>虚拟大模型</Text>」页面，点击「新建虚拟大模型」</li>
        <li>填写名称（即对外模型 ID，如 <Text code>qc480</Text>），添加后端大模型</li>
      </ol>
      <Typography.Title level={5}>第四步：创建 API Key</Typography.Title>
      <Typography.Paragraph>在「API Key」页面创建 API Key，用于 API 访问认证。API Key 全局通用，无需绑定特定虚拟大模型。</Typography.Paragraph>
      <ol>
        <li>进入「<Text strong>API Key</Text>」页面，点击「新建 API Key」</li>
        <li>填写名称，密钥自动生成，请立即复制保存</li>
      </ol>
      <Typography.Title level={5}>API 调用方式</Typography.Title>
      <Typography.Paragraph>假设管理端口为 <Text code>1994</Text>，虚拟大模型名称为 <Text code>qc480</Text>：</Typography.Paragraph>
      <Text strong>OpenAI 兼容接口：</Text>
      <pre className="code-block">{`POST http://localhost:1994/v1/chat/completions
Content-Type: application/json
Authorization: Bearer <your-api-key>

{
  "model": "qc480",
  "messages": [{"role": "user", "content": "Hello"}],
  "max_tokens": 100
}`}</pre>
    </div>
  )
}

function DocEn() {
  return (
    <div style={{ lineHeight: 1.8 }}>
      <Typography.Title level={4}>AI Gateway Help</Typography.Title>
      <Typography.Paragraph>
        <Text strong>AI Gateway</Text> is a cross-platform AI API aggregation and load balancing tool.
      </Typography.Paragraph>
      <Divider />
      <Typography.Title level={5}>Step 1: Add an AI Platform</Typography.Title>
      <Typography.Paragraph>Navigate to "Platforms" page, add a platform with API URL and API Key.</Typography.Paragraph>
      <Typography.Title level={5}>Step 2: Add Models</Typography.Title>
      <Typography.Paragraph>Navigate to "Models" page, add models from your platforms.</Typography.Paragraph>
      <Typography.Title level={5}>Step 3: Create Virtual Models</Typography.Title>
      <Typography.Paragraph>The virtual model name is the model ID exposed to clients. You can attach multiple backend models for load balancing. Both OpenAI and Anthropic protocols are supported by default.</Typography.Paragraph>
      <Typography.Title level={5}>Step 4: Create API Keys</Typography.Title>
      <Typography.Paragraph>Navigate to "API Keys" page to create API keys for access authentication. Keys are globally valid — no need to bind to specific virtual models.</Typography.Paragraph>
      <Typography.Title level={5}>API Endpoints</Typography.Title>
      <Typography.Paragraph>Assuming admin port is <Text code>1994</Text>, virtual model name is <Text code>qc480</Text>:</Typography.Paragraph>
      <Text strong>OpenAI Compatible:</Text>
      <pre className="code-block">{`POST http://localhost:1994/v1/chat/completions
Content-Type: application/json
Authorization: Bearer <your-api-key>

{
  "model": "qc480",
  "messages": [{"role": "user", "content": "Hello"}],
  "max_tokens": 100
}`}</pre>
    </div>
  )
}
