import { useState, useEffect } from 'react'
import { Layout, Typography, theme, Dropdown, Button, Space, Modal, Tooltip, Divider, message } from 'antd'
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
import { t, type Locale, type ThemeMode } from './i18n'

const { Content } = Layout
const { Title, Text, Paragraph } = Typography

const TAB_ITEMS = [
  { key: '/', icon: <DashboardOutlined style={{ fontSize: 18 }} />, color: '#1677ff', label: 'dashboard' },
  { key: '/platforms', icon: <CloudServerOutlined style={{ fontSize: 18 }} />, color: '#722ed1', label: 'platforms' },
  { key: '/proxies', icon: <ApiOutlined style={{ fontSize: 18 }} />, color: '#eb2f96', label: 'proxies' },
  { key: '/api-keys', icon: <KeyOutlined style={{ fontSize: 18 }} />, color: '#faad14', label: 'apiKeys' },
  { key: '/settings', icon: <SettingOutlined style={{ fontSize: 18 }} />, color: '#8c8c8c', label: 'settings' },
]

export default function App() {
  const navigate = useNavigate()
  const location = useLocation()
  const { token } = theme.useToken()
  const { themeMode, setThemeMode, isDark, locale, setLocale } = useAppContext()

  // Move message notifications to bottom-left to avoid blocking top navigation
  useEffect(() => {
    const style = document.createElement('style')
    style.id = 'message-left'
    style.textContent = `
      .ant-message {
        top: auto !important;
        bottom: 24px !important;
        left: 24px !important;
        transform: none !important;
      }
    `
    document.head.appendChild(style)
    return () => { document.getElementById('message-left')?.remove() }
  }, [])

  const themeIcon = themeMode === 'dark' ? <MoonOutlined /> : themeMode === 'light' ? <SunOutlined /> : <DesktopOutlined />
  const [docOpen, setDocOpen] = useState(false)

  const activeTab = TAB_ITEMS.find(t => t.key === location.pathname)
  const activeColor = activeTab?.color || '#1677ff'

  return (
    <Layout style={{ height: '100vh', overflow: 'hidden' }}>
      {/* Top Header Bar */}
      <div style={{
        height: 52,
        padding: '0 20px',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        borderBottom: `1px solid ${token.colorBorderSecondary}`,
        background: isDark ? '#0a0a0a' : '#fff',
        flexShrink: 0,
      }}>
        {/* Left: Logo + Name */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, minWidth: 200 }}>
          <img src="./logo.png" alt="" style={{ width: 26, height: 26, flexShrink: 0 }} />
          <div style={{ overflow: 'hidden' }}>
            <div style={{ fontSize: 14, fontWeight: 700, color: token.colorPrimary, lineHeight: '18px', whiteSpace: 'nowrap' }}>AI Gateway</div>
            <div style={{ fontSize: 10, color: token.colorTextSecondary, lineHeight: '13px', whiteSpace: 'nowrap' }}>{t(locale, 'appSubtitle')} v1.1.1</div>
          </div>
        </div>

        {/* Center: Tab Navigation — icon-only slider */}
        <div style={{
          display: 'flex',
          alignItems: 'center',
          gap: 0,
          background: isDark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.04)',
          borderRadius: 10,
          padding: 3,
          position: 'relative',
        }}>
          {/* Slider background */}
          <div style={{
            position: 'absolute',
            height: 'calc(100% - 6px)',
            width: `calc(${100 / TAB_ITEMS.length}% - 3px)`,
            left: `calc(${TAB_ITEMS.findIndex(item => item.key === location.pathname) * (100 / TAB_ITEMS.length)}% + 3px)`,
            top: 3,
            background: isDark ? 'rgba(255,255,255,0.1)' : '#fff',
            borderRadius: 8,
            boxShadow: isDark ? '0 1px 4px rgba(0,0,0,0.3)' : '0 1px 4px rgba(0,0,0,0.08)',
            transition: 'left 0.25s cubic-bezier(0.4, 0, 0.2, 1)',
            zIndex: 0,
          }} />
          {TAB_ITEMS.map(item => {
            const isActive = location.pathname === item.key
            return (
              <div
                key={item.key}
                onClick={() => navigate(item.key)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  width: 42,
                  height: 34,
                  borderRadius: 8,
                  cursor: 'pointer',
                  transition: 'color 0.2s',
                  position: 'relative',
                  zIndex: 1,
                  color: isActive ? item.color : (isDark ? `${item.color}66` : `${item.color}88`),
                }}
              >
                {item.icon}
              </div>
            )
          })}
        </div>

        {/* Right: Actions */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 2, minWidth: 200, justifyContent: 'flex-end' }}>
          <Tooltip title={t(locale, 'documentation')} placement="bottom">
            <Button type="text" size="small" icon={<BookOutlined style={{ color: token.colorTextSecondary }} />} onClick={() => setDocOpen(true)} />
          </Tooltip>
          <Dropdown menu={{
            items: [
              { key: 'light', icon: <SunOutlined />, label: t(locale, 'themeLight'), disabled: themeMode === 'light' },
              { key: 'dark', icon: <MoonOutlined />, label: t(locale, 'themeDark'), disabled: themeMode === 'dark' },
              { key: 'system', icon: <DesktopOutlined />, label: t(locale, 'themeSystem'), disabled: themeMode === 'system' },
            ],
            onClick: ({ key }) => setThemeMode(key as ThemeMode),
          }}>
            <Button type="text" size="small" icon={<span style={{ color: token.colorTextSecondary }}>{themeIcon}</span>} />
          </Dropdown>
          <Dropdown menu={{
            items: [
              { key: 'zh', label: '中文', disabled: locale === 'zh' },
              { key: 'en', label: 'EN', disabled: locale === 'en' },
            ],
            onClick: ({ key }) => setLocale(key as Locale),
          }}>
            <Button type="text" size="small" icon={<GlobalOutlined style={{ color: token.colorTextSecondary }} />} />
          </Dropdown>
          <Tooltip title="GitHub" placement="bottom">
            <Button type="text" size="small" icon={<GithubOutlined style={{ color: token.colorTextSecondary }} />} onClick={() => window.open('https://github.com/keiskeies/ai-gateway', '_blank')} />
          </Tooltip>
        </div>
      </div>

      {/* Scrollable content */}
      <Content style={{
        overflow: 'auto',
        padding: 24,
        background: isDark ? '#111' : '#f5f5f5',
        flex: 1,
      }}>
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/platforms" element={<Platforms />} />
          <Route path="/proxies" element={<Proxies />} />
          <Route path="/api-keys" element={<ApiKeys />} />
          <Route path="/settings" element={<Settings />} />
        </Routes>
      </Content>

      {/* Documentation Modal */}
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
  )
}

function DocZh() {
  const { token } = theme.useToken()
  const codeStyle: React.CSSProperties = {
    background: token.colorBgContainer,
    border: `1px solid ${token.colorBorderSecondary}`,
    borderRadius: 6,
    padding: '8px 12px',
    fontSize: 13,
    fontFamily: 'monospace',
    overflow: 'auto',
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-all',
    lineHeight: 1.6,
    marginTop: 8,
    marginBottom: 12,
  }
  return (
    <div style={{ lineHeight: 1.8 }}>
      <Title level={4}>📖 AI Gateway 使用帮助</Title>
      <Paragraph>
        <Text strong>AI Gateway</Text> 是一个跨平台 AI 接口聚合与负载均衡工具，支持 OpenAI、Anthropic、Ollama 等多种 AI 平台的统一接入。
      </Paragraph>
      <Divider />
      <Title level={5}>🔗 第一步：添加 AI 平台</Title>
      <Paragraph>平台是你 AI 模型的来源，比如 OpenAI、Anthropic、NVIDIA 等。每个平台需要配置 API 地址和 API Key。</Paragraph>
      <ol>
        <li>进入「<Text strong>平台</Text>」页面，点击「<Text strong>添加平台</Text>」</li>
        <li>选择预设平台或手动填写，配置 API Key 后保存</li>
      </ol>
      <Title level={5}>🤖 第二步：添加模型</Title>
      <Paragraph>每个平台上可能有多个 AI 模型，你需要把要使用的模型添加进来。</Paragraph>
      <ol>
        <li>进入「<Text strong>模型</Text>」页面，点击「<Text strong>添加模型</Text>」</li>
        <li>选择平台，选择预设模型或自定义输入模型 ID</li>
      </ol>
      <Title level={5}>🔌 第三步：创建虚拟大模型</Title>
      <Paragraph>虚拟大模型的名称即为对外暴露的模型 ID，后端可以挂载多个平台大模型实现负载均衡。默认同时支持 OpenAI 和 Anthropic 协议。</Paragraph>
      <ol>
        <li>进入「<Text strong>虚拟大模型</Text>」页面，点击「新建虚拟大模型」</li>
        <li>填写名称（即对外模型 ID，如 <Text code>qc480</Text>），添加后端大模型</li>
      </ol>
      <Title level={5}>🔑 第四步：创建 API Key</Title>
      <Paragraph>在「API Key」页面创建 API Key，用于 API 访问认证。API Key 全局通用，无需绑定特定虚拟大模型。</Paragraph>
      <ol>
        <li>进入「<Text strong>API Key</Text>」页面，点击「新建 API Key」</li>
        <li>填写名称，密钥自动生成，请立即复制保存</li>
      </ol>
      <Title level={5}>📡 API 调用方式</Title>
      <Paragraph>假设管理端口为 <Text code>1994</Text>，虚拟大模型名称为 <Text code>qc480</Text>：</Paragraph>
      <Text strong>OpenAI 兼容接口：</Text>
      <div style={codeStyle}>{`POST http://localhost:1994/v1/chat/completions
Content-Type: application/json
Authorization: Bearer <your-api-key>

{
  "model": "qc480",
  "messages": [{"role": "user", "content": "Hello"}],
  "max_tokens": 100
}`}</div>
    </div>
  )
}

function DocEn() {
  const { token } = theme.useToken()
  const codeStyle: React.CSSProperties = {
    background: token.colorBgContainer,
    border: `1px solid ${token.colorBorderSecondary}`,
    borderRadius: 6,
    padding: '8px 12px',
    fontSize: 13,
    fontFamily: 'monospace',
    overflow: 'auto',
    whiteSpace: 'pre-wrap',
    wordBreak: 'break-all',
    lineHeight: 1.6,
    marginTop: 8,
    marginBottom: 12,
  }
  return (
    <div style={{ lineHeight: 1.8 }}>
      <Title level={4}>📖 AI Gateway Help</Title>
      <Paragraph>
        <Text strong>AI Gateway</Text> is a cross-platform AI API aggregation and load balancing tool.
      </Paragraph>
      <Divider />
      <Title level={5}>🔗 Step 1: Add an AI Platform</Title>
      <Paragraph>Navigate to "Platforms" page, add a platform with API URL and API Key.</Paragraph>
      <Title level={5}>🤖 Step 2: Add Models</Title>
      <Paragraph>Navigate to "Models" page, add models from your platforms.</Paragraph>
      <Title level={5}>🔌 Step 3: Create Virtual Models</Title>
      <Paragraph>The virtual model name is the model ID exposed to clients. You can attach multiple backend models for load balancing. Both OpenAI and Anthropic protocols are supported by default.</Paragraph>
      <Title level={5}>🔑 Step 4: Create API Keys</Title>
      <Paragraph>Navigate to "API Keys" page to create API keys for access authentication. Keys are globally valid — no need to bind to specific virtual models.</Paragraph>
      <Title level={5}>📡 API Endpoints</Title>
      <Paragraph>Assuming admin port is <Text code>1994</Text>, virtual model name is <Text code>qc480</Text>:</Paragraph>
      <Text strong>OpenAI Compatible:</Text>
      <div style={codeStyle}>{`POST http://localhost:1994/v1/chat/completions
Content-Type: application/json
Authorization: Bearer <your-api-key>

{
  "model": "qc480",
  "messages": [{"role": "user", "content": "Hello"}],
  "max_tokens": 100
}`}</div>
    </div>
  )
}
