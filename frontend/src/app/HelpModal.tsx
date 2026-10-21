import { Code, Divider, Modal, Text, Title } from '@mantine/core'
import { useAppContext } from '../ThemeContext'
import { t } from '../i18n'

export function HelpModal({ opened, onClose }: { opened: boolean; onClose: () => void }) {
  const { locale } = useAppContext()
  return (
    <Modal opened={opened} onClose={onClose} title={t(locale, 'documentation')} size="lg">
      {locale === 'zh' ? <DocZh /> : <DocEn />}
    </Modal>
  )
}

function DocZh() {
  return (
    <div style={{ lineHeight: 1.8 }}>
      <Title order={4}>AI Gateway 使用帮助</Title>
      <Text mt="sm">
        <Text span fw={600}>AI Gateway</Text>
        {' '}是一个跨平台 AI 接口聚合与负载均衡工具，支持 OpenAI、Anthropic、Ollama 等多种 AI 平台的统一接入。
      </Text>
      <Divider my="md" />
      <Title order={5}>第一步：添加 AI 平台</Title>
      <Text>进入「平台」页面，点击「添加平台」，选择预设平台或手动填写，配置 API Key 后保存。</Text>
      <Title order={5} mt="md">第二步：创建虚拟模型</Title>
      <Text>虚拟模型的名称即为对外暴露的模型 ID，后端可以挂载多个平台大模型实现负载均衡。默认同时支持 OpenAI 和 Anthropic 协议。</Text>
      <Title order={5} mt="md">第三步：创建 API Key</Title>
      <Text>在「密钥」页面创建 API Key，用于 API 访问认证。API Key 全局通用，无需绑定特定虚拟模型。</Text>
      <Title order={5} mt="md">API 调用方式</Title>
      <Text>假设管理端口为 <Code>1994</Code>，虚拟模型名称为 <Code>qc480</Code>：</Text>
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
      <Text mt="sm">
        <Text span fw={600}>AI Gateway</Text>
        {' '}is a cross-platform AI API aggregation and load balancing tool.
      </Text>
      <Divider my="md" />
      <Title order={5}>Step 1: Add an AI Platform</Title>
      <Text>Navigate to "Platforms", add a platform with API URL and API Key.</Text>
      <Title order={5} mt="md">Step 2: Create Virtual Models</Title>
      <Text>The virtual model name is the model ID exposed to clients. Attach multiple backend models for load balancing.</Text>
      <Title order={5} mt="md">Step 3: Create API Keys</Title>
      <Text>Navigate to "Keys" to create keys for access authentication. Keys are globally valid.</Text>
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
