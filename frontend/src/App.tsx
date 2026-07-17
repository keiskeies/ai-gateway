import { useState } from 'react'
import { AppShell, Group, Burger, ActionIcon, Menu, Modal, Text, Code, Title, Divider, Button, Anchor } from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import {
  IconDashboard, IconCloudShare, IconApi, IconKey,
  IconSun, IconMoon, IconDeviceDesktop, IconBook,
  IconSettings, IconBrandGithub, IconLanguage,
} from '@tabler/icons-react'
import { Routes, Route, useNavigate, useLocation } from 'react-router-dom'
import Dashboard from './pages/Dashboard'
import Platforms from './pages/Platforms'
import Proxies from './pages/Proxies'
import ApiKeys from './pages/ApiKeys'
import Settings from './pages/Settings'
import { useAppContext } from './ThemeContext'
import { t, type Locale, type ThemeMode, type TranslationKey } from './i18n'

const NAV_ITEMS = [
  { key: '/', icon: IconDashboard, label: 'dashboard' as TranslationKey, color: '#2563eb' },
  { key: '/platforms', icon: IconCloudShare, label: 'platforms' as TranslationKey, color: '#7c3aed' },
  { key: '/proxies', icon: IconApi, label: 'proxies' as TranslationKey, color: '#db2777' },
  { key: '/api-keys', icon: IconKey, label: 'apiKeys' as TranslationKey, color: '#d97706' },
  { key: '/settings', icon: IconSettings, label: 'settings' as TranslationKey, color: '#64748b' },
]

export default function App() {
  const navigate = useNavigate()
  const location = useLocation()
  const { themeMode, setThemeMode, locale, setLocale } = useAppContext()
  const [mobileOpened, { toggle: toggleMobile }] = useDisclosure()
  const [desktopOpened, { toggle: toggleDesktop }] = useDisclosure()
  const [docOpen, { open: openDoc, close: closeDoc }] = useDisclosure(false)

  return (
    <AppShell
      header={{ height: 52 }}
      navbar={{ width: 200, breakpoint: 'sm', collapsed: { mobile: !mobileOpened, desktop: !desktopOpened } }}
      padding="md"
    >
      <AppShell.Header>
        <Group h="100%" px="md" justify="space-between">
          <Group>
            <Burger opened={mobileOpened} onClick={toggleMobile} hiddenFrom="sm" size="sm" />
            <Burger opened={desktopOpened} onClick={toggleDesktop} visibleFrom="sm" size="sm" />
            <Group gap={6} onClick={() => navigate('/')} style={{ cursor: 'pointer' }}>
              <img src="./logo.png" alt="" style={{ height: 28 }} />
              <Text fw={700} size="lg">AI Gateway</Text>
              <Text size="xs" c="dimmed">v1.2.4</Text>
            </Group>
          </Group>
          <Group gap={4}>
            <ActionIcon variant="subtle" size="lg" onClick={openDoc} title={t(locale, 'documentation')}>
              <IconBook size={18} />
            </ActionIcon>
            <Menu shadow="md" width={160}>
              <Menu.Target>
                <ActionIcon variant="subtle" size="lg">
                  {themeMode === 'dark' ? <IconMoon size={18} /> : themeMode === 'light' ? <IconSun size={18} /> : <IconDeviceDesktop size={18} />}
                </ActionIcon>
              </Menu.Target>
              <Menu.Dropdown>
                <Menu.Item leftSection={<IconSun size={14} />} disabled={themeMode === 'light'} onClick={() => setThemeMode('light')}>{t(locale, 'themeLight')}</Menu.Item>
                <Menu.Item leftSection={<IconMoon size={14} />} disabled={themeMode === 'dark'} onClick={() => setThemeMode('dark')}>{t(locale, 'themeDark')}</Menu.Item>
                <Menu.Item leftSection={<IconDeviceDesktop size={14} />} disabled={themeMode === 'system'} onClick={() => setThemeMode('system')}>{t(locale, 'themeSystem')}</Menu.Item>
              </Menu.Dropdown>
            </Menu>
            <Menu shadow="md" width={120}>
              <Menu.Target>
                <ActionIcon variant="subtle" size="lg">
                  <IconLanguage size={18} />
                </ActionIcon>
              </Menu.Target>
              <Menu.Dropdown>
                <Menu.Item disabled={locale === 'zh'} onClick={() => setLocale('zh')}>中文</Menu.Item>
                <Menu.Item disabled={locale === 'en'} onClick={() => setLocale('en')}>EN</Menu.Item>
              </Menu.Dropdown>
            </Menu>
            <Anchor href="https://github.com/keiskeies/ai-gateway" target="_blank">
              <ActionIcon variant="subtle" size="lg">
                <IconBrandGithub size={18} />
              </ActionIcon>
            </Anchor>
          </Group>
        </Group>
      </AppShell.Header>

      <AppShell.Navbar p="xs">
        {NAV_ITEMS.map(item => {
          const active = location.pathname === item.key
          return (
            <Button
              key={item.key}
              variant={active ? 'light' : 'subtle'}
              color={item.color}
              fullWidth
              justify="flex-start"
              leftSection={<item.icon size={18} />}
              onClick={() => { navigate(item.key); toggleMobile() }}
              style={{ fontWeight: active ? 600 : 400 }}
              mb={4}
            >
              {t(locale, item.label)}
            </Button>
          )
        })}
      </AppShell.Navbar>

      <AppShell.Main>
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/platforms" element={<Platforms />} />
          <Route path="/proxies" element={<Proxies />} />
          <Route path="/api-keys" element={<ApiKeys />} />
          <Route path="/settings" element={<Settings />} />
        </Routes>
      </AppShell.Main>

      <Modal opened={docOpen} onClose={closeDoc} title={t(locale, 'documentation')} size="lg">
        {locale === 'zh' ? <DocZh /> : <DocEn />}
      </Modal>
    </AppShell>
  )
}

function DocZh() {
  return (
    <div style={{ lineHeight: 1.8 }}>
      <Title order={4}>AI Gateway 使用帮助</Title>
      <Text mt="sm"><Text span fw={600}>AI Gateway</Text> 是一个跨平台 AI 接口聚合与负载均衡工具，支持 OpenAI、Anthropic、Ollama 等多种 AI 平台的统一接入。</Text>
      <Divider my="md" />
      <Title order={5}>第一步：添加 AI 平台</Title>
      <Text>进入「平台」页面，点击「添加平台」，选择预设平台或手动填写，配置 API Key 后保存。</Text>
      <Title order={5} mt="md">第二步：创建虚拟大模型</Title>
      <Text>虚拟大模型的名称即为对外暴露的模型 ID，后端可以挂载多个平台大模型实现负载均衡。默认同时支持 OpenAI 和 Anthropic 协议。</Text>
      <Title order={5} mt="md">第三步：创建 API Key</Title>
      <Text>在「API Key」页面创建 API Key，用于 API 访问认证。API Key 全局通用，无需绑定特定虚拟大模型。</Text>
      <Title order={5} mt="md">API 调用方式</Title>
      <Text>假设管理端口为 <Code>1994</Code>，虚拟大模型名称为 <Code>qc480</Code>：</Text>
      <Text fw={600} mt="xs">OpenAI 兼容接口：</Text>
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
      <Title order={4}>AI Gateway Help</Title>
      <Text mt="sm"><Text span fw={600}>AI Gateway</Text> is a cross-platform AI API aggregation and load balancing tool.</Text>
      <Divider my="md" />
      <Title order={5}>Step 1: Add an AI Platform</Title>
      <Text>Navigate to "Platforms" page, add a platform with API URL and API Key.</Text>
      <Title order={5} mt="md">Step 2: Create Virtual Models</Title>
      <Text>The virtual model name is the model ID exposed to clients. Attach multiple backend models for load balancing.</Text>
      <Title order={5} mt="md">Step 3: Create API Keys</Title>
      <Text>Navigate to "API Keys" page to create keys for access authentication. Keys are globally valid.</Text>
      <Title order={5} mt="md">API Endpoints</Title>
      <Text>Assuming admin port is <Code>1994</Code>, virtual model name is <Code>qc480</Code>:</Text>
      <Text fw={600} mt="xs">OpenAI Compatible:</Text>
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
