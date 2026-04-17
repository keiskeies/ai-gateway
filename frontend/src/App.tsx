import { useState } from 'react'
import { Layout, Menu, Typography, theme, Dropdown, Button, Space, Modal, Tooltip, Divider } from 'antd'
import {
  DashboardOutlined, CloudServerOutlined, RobotOutlined, ApiOutlined,
  SunOutlined, MoonOutlined, DesktopOutlined, GlobalOutlined,
  BookOutlined, SettingOutlined, GithubOutlined,
} from '@ant-design/icons'
import { Routes, Route, useNavigate, useLocation } from 'react-router-dom'
import Dashboard from './pages/Dashboard'
import Platforms from './pages/Platforms'
import Models from './pages/Models'
import Proxies from './pages/Proxies'
import Settings from './pages/Settings'
import { useAppContext } from './ThemeContext'
import { t, type Locale, type ThemeMode } from './i18n'

const { Sider, Content } = Layout
const { Title, Text, Paragraph } = Typography

export default function App() {
  const navigate = useNavigate()
  const location = useLocation()
  const { token } = theme.useToken()
  const { themeMode, setThemeMode, isDark, locale, setLocale } = useAppContext()

  const menuItems = [
    { key: '/', icon: <DashboardOutlined />, label: t(locale, 'dashboard') },
    { key: '/platforms', icon: <CloudServerOutlined />, label: t(locale, 'platforms') },
    { key: '/models', icon: <RobotOutlined />, label: t(locale, 'models') },
    { key: '/proxies', icon: <ApiOutlined />, label: t(locale, 'proxies') },
    { key: '/settings', icon: <SettingOutlined />, label: t(locale, 'settings') },
  ]

  const themeIcon = themeMode === 'dark' ? <MoonOutlined /> : themeMode === 'light' ? <SunOutlined /> : <DesktopOutlined />
  const themeLabel = themeMode === 'dark' ? t(locale, 'themeDark') : themeMode === 'light' ? t(locale, 'themeLight') : t(locale, 'themeSystem')

  const [docOpen, setDocOpen] = useState(false)
  const [collapsed, setCollapsed] = useState(false)

  return (
    <Layout style={{ height: '100vh', overflow: 'hidden' }}>
      {/* Sidebar */}
      <Sider
        width={220}
        collapsible
        collapsed={collapsed}
        onCollapse={setCollapsed}
        style={{
          background: isDark ? '#0a0a0a' : '#fff',
          borderRight: `1px solid ${token.colorBorderSecondary}`,
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
        }}
        trigger={null}
      >
        {/* Logo */}
        <div style={{
          padding: collapsed ? '20px 12px' : '20px 16px',
          display: 'flex',
          alignItems: 'center',
          justifyContent: collapsed ? 'center' : 'flex-start',
          gap: 10,
          borderBottom: `1px solid ${token.colorBorderSecondary}`,
        }}>
          <img src="./logo.png" alt="" style={{ width: 28, height: 28, flexShrink: 0 }} />
          {!collapsed && (
            <div style={{ overflow: 'hidden' }}>
              <div style={{ fontSize: 15, fontWeight: 700, color: token.colorPrimary, lineHeight: '20px', whiteSpace: 'nowrap' }}>AI Gateway</div>
              <div style={{ fontSize: 11, color: token.colorTextSecondary, lineHeight: '14px', whiteSpace: 'nowrap' }}>{t(locale, 'appSubtitle')}</div>
            </div>
          )}
        </div>

        {/* Menu */}
        <Menu
          mode="inline"
          selectedKeys={[location.pathname]}
          items={menuItems}
          onClick={({ key }) => navigate(key)}
          style={{
            border: 'none',
            flex: 1,
            background: 'transparent',
            marginTop: 8,
          }}
        />

        {/* Bottom actions */}
        <div style={{
          padding: '8px 12px',
          borderTop: `1px solid ${token.colorBorderSecondary}`,
          display: 'flex',
          flexDirection: collapsed ? 'column' : 'row',
          alignItems: 'center',
          gap: 4,
        }}>
          <Tooltip title={t(locale, 'documentation')} placement="right">
            <Button type="text" icon={<BookOutlined />} onClick={() => setDocOpen(true)} style={{ flex: 1 }} />
          </Tooltip>
          <Dropdown menu={{
            items: [
              { key: 'light', icon: <SunOutlined />, label: t(locale, 'themeLight'), disabled: themeMode === 'light' },
              { key: 'dark', icon: <MoonOutlined />, label: t(locale, 'themeDark'), disabled: themeMode === 'dark' },
              { key: 'system', icon: <DesktopOutlined />, label: t(locale, 'themeSystem'), disabled: themeMode === 'system' },
            ],
            onClick: ({ key }) => setThemeMode(key as ThemeMode),
          }}>
            <Button type="text" icon={themeIcon} style={{ flex: 1 }} />
          </Dropdown>
          <Dropdown menu={{
            items: [
              { key: 'zh', label: '中文', disabled: locale === 'zh' },
              { key: 'en', label: 'EN', disabled: locale === 'en' },
            ],
            onClick: ({ key }) => setLocale(key as Locale),
          }}>
            <Button type="text" icon={<GlobalOutlined />} style={{ flex: 1 }} />
          </Dropdown>
          <Tooltip title="GitHub" placement="right">
            <Button type="text" icon={<GithubOutlined />} onClick={() => window.open('https://github.com/keiskeies/ai-gateway', '_blank')} style={{ flex: 1 }} />
          </Tooltip>
        </div>
      </Sider>

      {/* Main content area */}
      <Layout style={{ overflow: 'hidden' }}>
        {/* Top bar */}
        <div style={{
          height: 48,
          padding: '0 24px',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          borderBottom: `1px solid ${token.colorBorderSecondary}`,
          background: isDark ? '#0a0a0a' : '#fff',
          flexShrink: 0,
        }}>
          <Title level={5} style={{ margin: 0, fontWeight: 600 }}>
            {menuItems.find(i => i.key === location.pathname)?.label || 'AI Gateway'}
          </Title>
          <Space size="small">
            <Text type="secondary" style={{ fontSize: 12 }}>AI Gateway v1.1.0</Text>
          </Space>
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
            <Route path="/models" element={<Models />} />
            <Route path="/proxies" element={<Proxies />} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </Content>
      </Layout>

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
        <Text strong>AI Gateway</Text> 是一个跨平台 AI 接口聚合与负载均衡工具，支持 OpenAI、Anthropic、Ollama 等多种 AI 平台的统一接入。下面将从零开始，一步步教你完成配置和使用。
      </Paragraph>

      <Divider />

      <Title level={5}>🔗 第一步：添加 AI 平台</Title>
      <Paragraph>平台是你 AI 模型的来源，比如 OpenAI、Anthropic、NVIDIA 等。每个平台需要配置 API 地址和 API Key。</Paragraph>
      <ol>
        <li>点击左侧菜单进入「<Text strong>平台</Text>」页面</li>
        <li>点击右上角「<Text strong>添加平台</Text>」按钮</li>
        <li>在「快速选择预设」下拉框中选择一个预设平台（如 OpenAI），系统会自动填充 API 地址和平台类型</li>
        <li>在「API Key」输入框中填入你的 API Key（如 <Text code>sk-xxxxxxxx</Text>）</li>
        <li>如果你使用的平台不在预设列表中，可以手动填写平台名称、API 地址和平台类型</li>
        <li>点击「确认」保存平台配置</li>
      </ol>
      <Paragraph type="secondary" style={{ fontSize: 12 }}>提示：API Key 仅存储在本地，不会上传到任何服务器。</Paragraph>

      <Title level={5}>🤖 第二步：添加模型</Title>
      <Paragraph>每个平台上可能有多个 AI 模型，你需要把要使用的模型添加进来。</Paragraph>
      <ol>
        <li>点击左侧菜单进入「<Text strong>模型</Text>」页面</li>
        <li>点击右上角「<Text strong>添加模型</Text>」按钮</li>
        <li>在「所属平台」下拉框中选择刚才添加的平台</li>
        <li>选择平台后，系统会自动列出该平台的预设模型，你可以直接选择</li>
        <li>如果预设列表中没有你需要的模型，切换到「自定义输入」标签，手动输入模型 ID（如 <Text code>gpt-4o-mini</Text>）</li>
        <li>填写显示名称（便于识别），可选填写最大输出 Token 和上下文窗口大小</li>
        <li>点击「确认」保存模型</li>
      </ol>
      <Paragraph type="secondary" style={{ fontSize: 12 }}>提示：添加模型后，可以点击操作列中的 🔌 图标按钮测试模型的连通性，确认 API Key 和地址配置正确。</Paragraph>

      <Title level={5}>🔌 第三步：创建聚合接口</Title>
      <Paragraph>聚合接口是客户端访问的统一入口。你可以把多个后端模型聚合成一个虚拟大模型，实现负载均衡和高可用。</Paragraph>
      <ol>
        <li>点击左侧菜单进入「<Text strong>聚合接口</Text>」页面</li>
        <li>点击右上角「<Text strong>新建接口</Text>」按钮</li>
        <li>填写接口名称（如 <Text code>my-gpt4</Text>）</li>
        <li>设置监听端口（如 <Text code>1998</Text>），这是聚合接口对外提供服务的端口</li>
        <li>选择支持的协议：
          <ul>
            <li><Text strong>OpenAI 兼容</Text>：支持 <Text code>/v1/chat/completions</Text>、<Text code>/v1/models</Text> 等接口</li>
            <li><Text strong>Anthropic 兼容</Text>：支持 <Text code>/v1/messages</Text> 接口</li>
          </ul>
        </li>
        <li>（可选）设置访问 Token：如果你希望对接口做访问控制，可以填写一个 Token，或者点击「自动生成」按钮生成一个 <Text code>sk-ag-xxx</Text> 格式的 Token</li>
        <li>点击「确认」创建接口</li>
      </ol>

      <Title level={5}>⚙️ 第四步：配置虚拟大模型</Title>
      <Paragraph>虚拟大模型是客户端请求时使用的模型名，它背后可以挂载一个或多个后端模型，实现负载均衡。</Paragraph>
      <ol>
        <li>在聚合接口列表中，点击操作列的 ⚙ 设置按钮，打开接口配置面板</li>
        <li>在「虚拟大模型」区域，点击「添加」按钮</li>
        <li>填写虚拟模型名（如 <Text code>gpt-4</Text>），这是客户端调用时使用的模型名称</li>
        <li>选择负载均衡策略：
          <ul>
            <li><Text strong>轮询</Text>：请求依次分配到各后端，适合后端性能相近的场景</li>
            <li><Text strong>加权随机</Text>：按权重随机分配，适合后端性能差异较大的场景</li>
            <li><Text strong>最少连接</Text>：优先分配给当前连接数最少的后端</li>
            <li><Text strong>优先级</Text>：主备模式，优先使用高优先级后端，主节点故障时自动切换到备节点</li>
            <li><Text strong>延迟优先</Text>：自动选择响应最快的后端</li>
          </ul>
        </li>
        <li>添加后端大模型：选择平台和模型，设置权重和优先级</li>
        <li>可以添加多个后端大模型，它们将按所选策略进行负载均衡</li>
        <li>点击「确认」保存配置</li>
      </ol>

      <Title level={5}>🚀 第五步：启动接口并调用</Title>
      <Paragraph>配置完成后，启动接口即可开始使用。</Paragraph>
      <ol>
        <li>在聚合接口列表中，点击操作列的 ▶ 启动按钮</li>
        <li>接口状态变为「运行中」后，即可通过 API 调用</li>
        <li>在虚拟大模型卡片中，点击「使用方式」按钮可以查看各语言的调用代码示例</li>
      </ol>

      <Title level={5}>📡 API 调用方式</Title>
      <Paragraph>假设聚合接口的端口为 <Text code>1998</Text>，虚拟模型名为 <Text code>gpt-4</Text>：</Paragraph>

      <Text strong>OpenAI 兼容接口：</Text>
      <div style={codeStyle}>{`POST http://localhost:1998/v1/chat/completions
Content-Type: application/json
${'Authorization: Bearer <your-token>' /* for readability */}

{
  "model": "gpt-4",
  "messages": [{"role": "user", "content": "Hello"}],
  "max_tokens": 100
}`}</div>

      <Text strong>Anthropic 兼容接口：</Text>
      <div style={codeStyle}>{`POST http://localhost:1998/v1/messages
Content-Type: application/json
anthropic-version: 2023-06-01
x-api-key: <your-token>

{
  "model": "gpt-4",
  "max_tokens": 100,
  "messages": [{"role": "user", "content": "Hello"}]
}`}</div>

      <Text strong>查看可用模型列表：</Text>
      <div style={codeStyle}>GET http://localhost:1998/v1/models</div>

      <Paragraph type="secondary" style={{ fontSize: 12 }}>提示：如果聚合接口设置了访问 Token，请在请求头中携带 <Text code>Authorization: Bearer &lt;token&gt;</Text>（OpenAI 协议）或 <Text code>x-api-key: &lt;token&gt;</Text>（Anthropic 协议）。</Paragraph>

      <Title level={5}>💡 其他功能</Title>
      <ul>
        <li><Text strong>连通性测试</Text>：在模型列表中，点击 🔌 按钮可测试模型是否能正常连接，查看延迟和状态信息</li>
        <li><Text strong>编辑接口基本信息</Text>：在接口配置面板中，点击「编辑」按钮可修改接口名称、端口和支持协议</li>
        <li><Text strong>修改设置</Text>：在「设置」页面可修改管理端口、监听地址、日志级别、重试策略等（修改端口和地址后需重启应用）</li>
      </ul>
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
        <Text strong>AI Gateway</Text> is a cross-platform AI API aggregation and load balancing tool, supporting OpenAI, Anthropic, Ollama and more. This guide will walk you through the setup process step by step.
      </Paragraph>

      <Divider />

      <Title level={5}>🔗 Step 1: Add an AI Platform</Title>
      <Paragraph>A platform is the source of your AI models, such as OpenAI, Anthropic, NVIDIA, etc. Each platform requires an API URL and API Key.</Paragraph>
      <ol>
        <li>Navigate to the <Text strong>"Platforms"</Text> page from the sidebar</li>
        <li>Click the <Text strong>"Add Platform"</Text> button in the top right</li>
        <li>Select a preset platform from the "Quick preset" dropdown (e.g., OpenAI). The API URL and platform type will be auto-filled</li>
        <li>Enter your API Key in the "API Key" field (e.g., <Text code>sk-xxxxxxxx</Text>)</li>
        <li>If your platform is not in the preset list, manually fill in the platform name, API URL, and platform type</li>
        <li>Click "OK" to save the platform configuration</li>
      </ol>
      <Paragraph type="secondary" style={{ fontSize: 12 }}>Tip: Your API Key is stored locally and never uploaded to any server.</Paragraph>

      <Title level={5}>🤖 Step 2: Add Models</Title>
      <Paragraph>Each platform may have multiple AI models. You need to add the models you want to use.</Paragraph>
      <ol>
        <li>Navigate to the <Text strong>"Models"</Text> page from the sidebar</li>
        <li>Click the <Text strong>"Add Model"</Text> button in the top right</li>
        <li>Select the platform you just added from the "Platform" dropdown</li>
        <li>After selecting a platform, preset models will be listed automatically. You can select one directly</li>
        <li>If your model is not in the preset list, switch to the "Custom" tab and type the model ID manually (e.g., <Text code>gpt-4o-mini</Text>)</li>
        <li>Fill in the display name (for easy identification), and optionally set max output tokens and context window size</li>
        <li>Click "OK" to save the model</li>
      </ol>
      <Paragraph type="secondary" style={{ fontSize: 12 }}>Tip: After adding a model, click the 🔌 button in the action column to test its connectivity and verify your API Key and URL are correct.</Paragraph>

      <Title level={5}>🔌 Step 3: Create an Aggregated API</Title>
      <Paragraph>An aggregated API is the unified entry point for clients. You can combine multiple backend models into one virtual model with load balancing and high availability.</Paragraph>
      <ol>
        <li>Navigate to the <Text strong>"Aggregated APIs"</Text> page from the sidebar</li>
        <li>Click the <Text strong>"New API"</Text> button in the top right</li>
        <li>Fill in the API name (e.g., <Text code>my-gpt4</Text>)</li>
        <li>Set the listen port (e.g., <Text code>1998</Text>). This is the port the aggregated API will serve on</li>
        <li>Select the supported protocols:
          <ul>
            <li><Text strong>OpenAI Compatible</Text>: Supports <Text code>/v1/chat/completions</Text>, <Text code>/v1/models</Text>, etc.</li>
            <li><Text strong>Anthropic Compatible</Text>: Supports <Text code>/v1/messages</Text> endpoint</li>
          </ul>
        </li>
        <li>(Optional) Set an Auth Token: If you want access control, fill in a token or click "Auto Generate" to create one in <Text code>sk-ag-xxx</Text> format</li>
        <li>Click "OK" to create the API</li>
      </ol>

      <Title level={5}>⚙️ Step 4: Configure Virtual Models</Title>
      <Paragraph>A virtual model is the model name clients use when making requests. It can have one or more backend models behind it for load balancing.</Paragraph>
      <ol>
        <li>In the aggregated API list, click the ⚙ Settings button to open the configuration panel</li>
        <li>In the "Virtual Models" section, click the "Add" button</li>
        <li>Fill in the virtual model name (e.g., <Text code>gpt-4</Text>). This is the model name clients will use in their requests</li>
        <li>Select a load balancing strategy:
          <ul>
            <li><Text strong>Round Robin</Text>: Requests are distributed sequentially. Best for similar-performance backends</li>
            <li><Text strong>Weighted Random</Text>: Random distribution by weight. Best for mixed-performance backends</li>
            <li><Text strong>Least Connections</Text>: Prefers the backend with fewest active connections</li>
            <li><Text strong>Priority</Text>: Failover mode. Uses highest-priority backend first, switches on failure</li>
            <li><Text strong>Latency Based</Text>: Automatically selects the fastest-responding backend</li>
          </ul>
        </li>
        <li>Add backend models: select a platform and model, then set weight and priority</li>
        <li>You can add multiple backend models for load balancing</li>
        <li>Click "OK" to save the configuration</li>
      </ol>

      <Title level={5}>🚀 Step 5: Start the API and Make Requests</Title>
      <Paragraph>Once configured, start the API to begin using it.</Paragraph>
      <ol>
        <li>In the aggregated API list, click the ▶ Start button</li>
        <li>When the status changes to "Running", the API is ready to accept requests</li>
        <li>Click the "Usage" button on a virtual model card to see code examples in various languages</li>
      </ol>

      <Title level={5}>📡 API Endpoints</Title>
      <Paragraph>Assuming the aggregated API port is <Text code>1998</Text> and the virtual model name is <Text code>gpt-4</Text>:</Paragraph>

      <Text strong>OpenAI Compatible:</Text>
      <div style={codeStyle}>{`POST http://localhost:1998/v1/chat/completions
Content-Type: application/json
Authorization: Bearer <your-token>

{
  "model": "gpt-4",
  "messages": [{"role": "user", "content": "Hello"}],
  "max_tokens": 100
}`}</div>

      <Text strong>Anthropic Compatible:</Text>
      <div style={codeStyle}>{`POST http://localhost:1998/v1/messages
Content-Type: application/json
anthropic-version: 2023-06-01
x-api-key: <your-token>

{
  "model": "gpt-4",
  "max_tokens": 100,
  "messages": [{"role": "user", "content": "Hello"}]
}`}</div>

      <Text strong>List available models:</Text>
      <div style={codeStyle}>GET http://localhost:1998/v1/models</div>

      <Paragraph type="secondary" style={{ fontSize: 12 }}>Tip: If the aggregated API has an auth token set, include <Text code>Authorization: Bearer &lt;token&gt;</Text> (OpenAI protocol) or <Text code>x-api-key: &lt;token&gt;</Text> (Anthropic protocol) in your request headers.</Paragraph>

      <Title level={5}>💡 Other Features</Title>
      <ul>
        <li><Text strong>Connectivity Test</Text>: Click the 🔌 button in the model list to test if a model can connect properly, and view latency and status info</li>
        <li><Text strong>Edit API Info</Text>: In the API configuration panel, click "Edit" to modify the API name, port, and supported protocols</li>
        <li><Text strong>Settings</Text>: On the "Settings" page, you can modify the admin port, listen host, log level, retry policies, etc. (Port and host changes require an app restart)</li>
      </ul>
    </div>
  )
}
