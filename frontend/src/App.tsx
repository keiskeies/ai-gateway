import { useState } from 'react'
import { Layout, Menu, Typography, theme, Dropdown, Button, Space, Modal } from 'antd'
import {
  DashboardOutlined, CloudServerOutlined, RobotOutlined, ApiOutlined,
  SunOutlined, MoonOutlined, DesktopOutlined, GlobalOutlined,
  BookOutlined,
} from '@ant-design/icons'
import { Routes, Route, useNavigate, useLocation } from 'react-router-dom'
import Dashboard from './pages/Dashboard'
import Platforms from './pages/Platforms'
import Models from './pages/Models'
import Proxies from './pages/Proxies'
import { useAppContext } from './ThemeContext'
import { t, type Locale, type ThemeMode } from './i18n'

const { Header, Sider, Content, Footer } = Layout
const { Title, Paragraph, Text } = Typography

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
  ]

  const themeIcon = themeMode === 'dark' ? <MoonOutlined /> : themeMode === 'light' ? <SunOutlined /> : <DesktopOutlined />
  const themeLabel = themeMode === 'dark' ? t(locale, 'themeDark') : themeMode === 'light' ? t(locale, 'themeLight') : t(locale, 'themeSystem')

  const [docOpen, setDocOpen] = useState(false)

  return (
    <Layout style={{ minHeight: '100vh', margin: 0, padding: 0 }}>
      <Sider width={220} style={{ background: token.colorBgContainer, display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: '20px 16px', textAlign: 'center' }}>
          <Title level={4} style={{ margin: 0, color: token.colorPrimary }}>
            <ApiOutlined /> AI Gateway
          </Title>
          <div style={{ fontSize: 12, color: token.colorTextSecondary, marginTop: 4 }}>
            {t(locale, 'appSubtitle')}
          </div>
        </div>
        <Menu
          mode="inline"
          selectedKeys={[location.pathname]}
          items={menuItems}
          onClick={({ key }) => navigate(key)}
          style={{ border: 'none', flex: 1 }}
        />
        <div style={{ padding: '12px 16px', borderTop: `1px solid ${token.colorBorderSecondary}` }}>
          <Button type="link" icon={<BookOutlined />} block style={{ textAlign: 'left' }} onClick={() => setDocOpen(true)}>
            {t(locale, 'documentation')}
          </Button>
        </div>
      </Sider>
      <Layout>
        <Header style={{ background: token.colorBgContainer, padding: '0 24px', display: 'flex', alignItems: 'center', justifyContent: 'space-between', borderBottom: `1px solid ${token.colorBorderSecondary}` }}>
          <Title level={5} style={{ margin: 0 }}>
            {menuItems.find(i => i.key === location.pathname)?.label || 'AI Gateway'}
          </Title>
          <Space>
            <Dropdown menu={{
              items: [
                { key: 'light', icon: <SunOutlined />, label: t(locale, 'themeLight'), disabled: themeMode === 'light' },
                { key: 'dark', icon: <MoonOutlined />, label: t(locale, 'themeDark'), disabled: themeMode === 'dark' },
                { key: 'system', icon: <DesktopOutlined />, label: t(locale, 'themeSystem'), disabled: themeMode === 'system' },
              ],
              onClick: ({ key }) => setThemeMode(key as ThemeMode),
            }}>
              <Button icon={themeIcon} size="small">{themeLabel}</Button>
            </Dropdown>
            <Dropdown menu={{
              items: [
                { key: 'zh', label: t(locale, 'langZh'), disabled: locale === 'zh' },
                { key: 'en', label: t(locale, 'langEn'), disabled: locale === 'en' },
              ],
              onClick: ({ key }) => setLocale(key as Locale),
            }}>
              <Button icon={<GlobalOutlined />} size="small">{locale === 'zh' ? '中文' : 'EN'}</Button>
            </Dropdown>
          </Space>
        </Header>
        <Content style={{ margin: 0, padding: 20, background: token.colorBgLayout, overflow: 'auto' }}>
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/platforms" element={<Platforms />} />
            <Route path="/models" element={<Models />} />
            <Route path="/proxies" element={<Proxies />} />
          </Routes>
        </Content>
      </Layout>

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
  return (
    <div style={{ lineHeight: 1.8 }}>
      <Title level={4}>AI Gateway 使用文档</Title>
      <Paragraph>
        <Text strong>AI Gateway</Text> 是一个跨平台 AI 接口聚合与负载均衡工具，支持 OpenAI、Anthropic、Ollama 等多种 AI 平台的统一接入。
      </Paragraph>
      <Title level={5}>1. 添加平台</Title>
      <Paragraph>进入「平台管理」页面，点击「添加平台」，选择预设平台或自定义配置。填写 API Key 后保存。</Paragraph>
      <Title level={5}>2. 添加模型</Title>
      <Paragraph>进入「模型管理」页面，点击「添加模型」，选择所属平台后，从预设模型中选择或手动输入模型ID。</Paragraph>
      <Title level={5}>3. 创建代理</Title>
      <Paragraph>进入「代理管理」页面，点击「新建代理」，设置代理名称和监听端口，选择支持的协议。</Paragraph>
      <Title level={5}>4. 配置路由</Title>
      <Paragraph>在代理详情中，点击「添加路由」，设置虚拟模型名（客户端请求时使用），选择负载均衡策略，添加一个或多个后端模型。</Paragraph>
      <Title level={5}>5. 启动代理</Title>
      <Paragraph>点击代理列表中的「启动」按钮，代理开始运行后即可通过对应端点访问。</Paragraph>
      <Title level={5}>6. 调用方式</Title>
      <Paragraph>
        <Text strong>OpenAI 兼容：</Text><Text code>POST http://localhost:&#123;端口&#125;/v1/chat/completions</Text><br/>
        <Text strong>Anthropic 兼容：</Text><Text code>POST http://localhost:&#123;端口&#125;/v1/messages</Text><br/>
        <Text strong>模型列表：</Text><Text code>GET http://localhost:&#123;端口&#125;/v1/models</Text>
      </Paragraph>
      <Title level={5}>7. 负载均衡策略</Title>
      <ul>
        <li><Text strong>轮询</Text>：依次分配请求到各后端</li>
        <li><Text strong>加权随机</Text>：按权重随机分配</li>
        <li><Text strong>最少连接</Text>：优先分配给连接数最少的后端</li>
        <li><Text strong>优先级</Text>：主备模式，优先使用高优先级后端</li>
        <li><Text strong>延迟优先</Text>：优先使用响应最快的后端</li>
      </ul>
    </div>
  )
}

function DocEn() {
  return (
    <div style={{ lineHeight: 1.8 }}>
      <Title level={4}>AI Gateway Documentation</Title>
      <Paragraph>
        <Text strong>AI Gateway</Text> is a cross-platform AI API aggregation and load balancing tool, supporting OpenAI, Anthropic, Ollama and more.
      </Paragraph>
      <Title level={5}>1. Add Platform</Title>
      <Paragraph>Go to "Platforms" page, click "Add Platform", select a preset or custom configuration. Fill in your API Key and save.</Paragraph>
      <Title level={5}>2. Add Model</Title>
      <Paragraph>Go to "Models" page, click "Add Model", select a platform, then choose from preset models or type a custom model ID.</Paragraph>
      <Title level={5}>3. Create Proxy</Title>
      <Paragraph>Go to "Proxies" page, click "New Proxy", set name and port, select protocols.</Paragraph>
      <Title level={5}>4. Configure Routes</Title>
      <Paragraph>In proxy details, click "Add Route", set virtual model name (used by client), select LB strategy, add one or more backend models.</Paragraph>
      <Title level={5}>5. Start Proxy</Title>
      <Paragraph>Click "Start" in the proxy list. Once running, access via the endpoint.</Paragraph>
      <Title level={5}>6. API Endpoints</Title>
      <Paragraph>
        <Text strong>OpenAI Compatible:</Text> <Text code>POST http://localhost:&#123;port&#125;/v1/chat/completions</Text><br/>
        <Text strong>Anthropic Compatible:</Text> <Text code>POST http://localhost:&#123;port&#125;/v1/messages</Text><br/>
        <Text strong>Model List:</Text> <Text code>GET http://localhost:&#123;port&#125;/v1/models</Text>
      </Paragraph>
      <Title level={5}>7. Load Balancing Strategies</Title>
      <ul>
        <li><Text strong>Round Robin</Text>: Distribute requests sequentially</li>
        <li><Text strong>Weighted Random</Text>: Random distribution by weight</li>
        <li><Text strong>Least Connections</Text>: Prefer backend with fewest connections</li>
        <li><Text strong>Priority</Text>: Failover mode, prefer higher priority backends</li>
        <li><Text strong>Latency Based</Text>: Prefer fastest responding backend</li>
      </ul>
    </div>
  )
}
