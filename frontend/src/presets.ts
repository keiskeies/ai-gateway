import type { Locale } from './i18n'

export interface PlatformPreset {
  name: string
  nameZh: string
  nameEn: string
  platform_type: string
  base_url: string
  models: ModelPreset[]
}

export interface ModelPreset {
  model_id: string
  display_name: string
  display_name_zh: string
  max_tokens: number
  context_window: number
  capabilities: string[]
}

export const platformPresets: PlatformPreset[] = [
  {
    name: 'OpenAI', nameZh: 'OpenAI', nameEn: 'OpenAI',
    platform_type: 'OpenAI', base_url: 'https://api.openai.com/v1',
    models: [
      { model_id: 'gpt-4o', display_name: 'GPT-4o', display_name_zh: 'GPT-4o', max_tokens: 16384, context_window: 128000, capabilities: ['Vision', 'Tool'] },
      { model_id: 'gpt-4o-mini', display_name: 'GPT-4o Mini', display_name_zh: 'GPT-4o Mini', max_tokens: 16384, context_window: 128000, capabilities: ['Vision', 'Tool'] },
      { model_id: 'gpt-4-turbo', display_name: 'GPT-4 Turbo', display_name_zh: 'GPT-4 Turbo', max_tokens: 4096, context_window: 128000, capabilities: ['Vision'] },
      { model_id: 'gpt-3.5-turbo', display_name: 'GPT-3.5 Turbo', display_name_zh: 'GPT-3.5 Turbo', max_tokens: 4096, context_window: 16385, capabilities: ['Tool'] },
      { model_id: 'o1', display_name: 'o1', display_name_zh: 'o1', max_tokens: 32768, context_window: 200000, capabilities: ['Reasoning'] },
      { model_id: 'o1-mini', display_name: 'o1 Mini', display_name_zh: 'o1 Mini', max_tokens: 65536, context_window: 128000, capabilities: ['Reasoning'] },
      { model_id: 'o3-mini', display_name: 'o3 Mini', display_name_zh: 'o3 Mini', max_tokens: 65536, context_window: 200000, capabilities: ['Reasoning'] },
    ]
  },
  {
    name: 'Anthropic', nameZh: 'Anthropic', nameEn: 'Anthropic',
    platform_type: 'Anthropic', base_url: 'https://api.anthropic.com',
    models: [
      { model_id: 'claude-sonnet-4-20250514', display_name: 'Claude Sonnet 4', display_name_zh: 'Claude Sonnet 4', max_tokens: 16384, context_window: 200000, capabilities: ['Vision', 'Tool'] },
      { model_id: 'claude-3-5-sonnet-20241022', display_name: 'Claude 3.5 Sonnet', display_name_zh: 'Claude 3.5 Sonnet', max_tokens: 8192, context_window: 200000, capabilities: ['Vision', 'Tool'] },
      { model_id: 'claude-3-5-haiku-20241022', display_name: 'Claude 3.5 Haiku', display_name_zh: 'Claude 3.5 Haiku', max_tokens: 8192, context_window: 200000, capabilities: ['Tool'] },
      { model_id: 'claude-3-opus-20240229', display_name: 'Claude 3 Opus', display_name_zh: 'Claude 3 Opus', max_tokens: 4096, context_window: 200000, capabilities: ['Vision'] },
    ]
  },
  {
    name: 'DeepSeek', nameZh: '深度求索', nameEn: 'DeepSeek',
    platform_type: 'OpenAI', base_url: 'https://api.deepseek.com/v1',
    models: [
      { model_id: 'deepseek-chat', display_name: 'DeepSeek Chat', display_name_zh: 'DeepSeek 对话', max_tokens: 8192, context_window: 65536, capabilities: ['Tool'] },
      { model_id: 'deepseek-reasoner', display_name: 'DeepSeek Reasoner', display_name_zh: 'DeepSeek 推理', max_tokens: 8192, context_window: 65536, capabilities: ['Reasoning'] },
    ]
  },
  {
    name: 'NVIDIA', nameZh: '英伟达', nameEn: 'NVIDIA',
    platform_type: 'OpenAI', base_url: 'https://integrate.api.nvidia.com/v1',
    models: [
      { model_id: 'meta/llama-3.3-70b-instruct', display_name: 'Llama 3.3 70B', display_name_zh: 'Llama 3.3 70B', max_tokens: 4096, context_window: 131072, capabilities: [] },
      { model_id: 'nvidia/llama-3.1-nemotron-70b-instruct', display_name: 'Nemotron 70B', display_name_zh: 'Nemotron 70B', max_tokens: 4096, context_window: 131072, capabilities: [] },
      { model_id: 'deepseek-ai/deepseek-r1', display_name: 'DeepSeek R1', display_name_zh: 'DeepSeek R1', max_tokens: 8192, context_window: 131072, capabilities: ['Reasoning'] },
      { model_id: 'qwen/qwen2.5-72b-instruct', display_name: 'Qwen 2.5 72B', display_name_zh: '通义千问 2.5 72B', max_tokens: 4096, context_window: 131072, capabilities: [] },
    ]
  },
  {
    name: 'Moonshot', nameZh: '月之暗面', nameEn: 'Moonshot',
    platform_type: 'OpenAI', base_url: 'https://api.moonshot.cn/v1',
    models: [
      { model_id: 'moonshot-v1-8k', display_name: 'Moonshot v1 8K', display_name_zh: 'Kimi v1 8K', max_tokens: 4096, context_window: 8192, capabilities: [] },
      { model_id: 'moonshot-v1-32k', display_name: 'Moonshot v1 32K', display_name_zh: 'Kimi v1 32K', max_tokens: 4096, context_window: 32768, capabilities: [] },
      { model_id: 'moonshot-v1-128k', display_name: 'Moonshot v1 128K', display_name_zh: 'Kimi v1 128K', max_tokens: 4096, context_window: 131072, capabilities: [] },
    ]
  },
  {
    name: 'ZhipuAI', nameZh: '智谱AI', nameEn: 'ZhipuAI',
    platform_type: 'OpenAI', base_url: 'https://open.bigmodel.cn/api/paas/v4',
    models: [
      { model_id: 'glm-4-plus', display_name: 'GLM-4 Plus', display_name_zh: 'GLM-4 Plus', max_tokens: 4096, context_window: 128000, capabilities: ['Tool'] },
      { model_id: 'glm-4-flash', display_name: 'GLM-4 Flash', display_name_zh: 'GLM-4 Flash', max_tokens: 4096, context_window: 128000, capabilities: ['Tool'] },
      { model_id: 'glm-4-long', display_name: 'GLM-4 Long', display_name_zh: 'GLM-4 Long', max_tokens: 4096, context_window: 1048576, capabilities: [] },
    ]
  },
  {
    name: 'Qwen', nameZh: '通义千问', nameEn: 'Qwen',
    platform_type: 'OpenAI', base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    models: [
      { model_id: 'qwen-max', display_name: 'Qwen Max', display_name_zh: '通义千问 Max', max_tokens: 8192, context_window: 32768, capabilities: ['Tool'] },
      { model_id: 'qwen-plus', display_name: 'Qwen Plus', display_name_zh: '通义千问 Plus', max_tokens: 8192, context_window: 131072, capabilities: ['Tool'] },
      { model_id: 'qwen-turbo', display_name: 'Qwen Turbo', display_name_zh: '通义千问 Turbo', max_tokens: 8192, context_window: 131072, capabilities: ['Tool'] },
      { model_id: 'qwen-long', display_name: 'Qwen Long', display_name_zh: '通义千问 Long', max_tokens: 6000, context_window: 10000000, capabilities: [] },
    ]
  },
  {
    name: 'Doubao', nameZh: '豆包', nameEn: 'Doubao',
    platform_type: 'OpenAI', base_url: 'https://ark.cn-beijing.volces.com/api/v3',
    models: [
      { model_id: 'doubao-pro-32k', display_name: 'Doubao Pro 32K', display_name_zh: '豆包 Pro 32K', max_tokens: 4096, context_window: 32768, capabilities: [] },
      { model_id: 'doubao-pro-128k', display_name: 'Doubao Pro 128K', display_name_zh: '豆包 Pro 128K', max_tokens: 4096, context_window: 131072, capabilities: [] },
      { model_id: 'doubao-lite-32k', display_name: 'Doubao Lite 32K', display_name_zh: '豆包 Lite 32K', max_tokens: 4096, context_window: 32768, capabilities: [] },
    ]
  },
  {
    name: 'SiliconFlow', nameZh: '硅基流动', nameEn: 'SiliconFlow',
    platform_type: 'OpenAI', base_url: 'https://api.siliconflow.cn/v1',
    models: [
      { model_id: 'deepseek-ai/DeepSeek-V3', display_name: 'DeepSeek V3', display_name_zh: 'DeepSeek V3', max_tokens: 8192, context_window: 65536, capabilities: [] },
      { model_id: 'Qwen/Qwen2.5-72B-Instruct', display_name: 'Qwen 2.5 72B', display_name_zh: '通义千问 2.5 72B', max_tokens: 8192, context_window: 32768, capabilities: [] },
    ]
  },
  {
    name: 'Groq', nameZh: 'Groq', nameEn: 'Groq',
    platform_type: 'OpenAI', base_url: 'https://api.groq.com/openai/v1',
    models: [
      { model_id: 'llama-3.3-70b-versatile', display_name: 'Llama 3.3 70B', display_name_zh: 'Llama 3.3 70B', max_tokens: 32768, context_window: 131072, capabilities: [] },
      { model_id: 'mixtral-8x7b-32768', display_name: 'Mixtral 8x7B', display_name_zh: 'Mixtral 8x7B', max_tokens: 32768, context_window: 32768, capabilities: [] },
    ]
  },
  {
    name: 'Google Gemini', nameZh: 'Google Gemini', nameEn: 'Google Gemini',
    platform_type: 'OpenAI', base_url: 'https://generativelanguage.googleapis.com/v1beta/openai',
    models: [
      { model_id: 'gemini-2.5-pro-preview-03-25', display_name: 'Gemini 2.5 Pro', display_name_zh: 'Gemini 2.5 Pro', max_tokens: 65536, context_window: 1048576, capabilities: ['Vision', 'reasoning'] },
      { model_id: 'gemini-2.0-flash', display_name: 'Gemini 2.0 Flash', display_name_zh: 'Gemini 2.0 Flash', max_tokens: 8192, context_window: 1048576, capabilities: ['Vision'] },
      { model_id: 'gemini-2.0-flash-lite', display_name: 'Gemini 2.0 Flash Lite', display_name_zh: 'Gemini 2.0 Flash Lite', max_tokens: 8192, context_window: 1048576, capabilities: [] },
    ]
  },
  {
    name: 'Together AI', nameZh: 'Together AI', nameEn: 'Together AI',
    platform_type: 'OpenAI', base_url: 'https://api.together.xyz/v1',
    models: [
      { model_id: 'meta-llama/Llama-3.3-70B-Instruct-Turbo', display_name: 'Llama 3.3 70B', display_name_zh: 'Llama 3.3 70B', max_tokens: 4096, context_window: 131072, capabilities: [] },
    ]
  },
  {
    name: 'OpenRouter', nameZh: 'OpenRouter', nameEn: 'OpenRouter',
    platform_type: 'OpenAI', base_url: 'https://openrouter.ai/api/v1',
    models: [
      { model_id: 'openai/gpt-4o', display_name: 'GPT-4o', display_name_zh: 'GPT-4o', max_tokens: 16384, context_window: 128000, capabilities: ['Vision'] },
      { model_id: 'anthropic/claude-3.5-sonnet', display_name: 'Claude 3.5 Sonnet', display_name_zh: 'Claude 3.5 Sonnet', max_tokens: 8192, context_window: 200000, capabilities: ['Vision'] },
    ]
  },
  {
    name: 'Ollama', nameZh: 'Ollama', nameEn: 'Ollama',
    platform_type: 'Ollama', base_url: 'http://localhost:11434/v1',
    models: [
      { model_id: 'llama3.1:8b', display_name: 'Llama 3.1 8B', display_name_zh: 'Llama 3.1 8B', max_tokens: 4096, context_window: 8192, capabilities: [] },
      { model_id: 'qwen2.5:7b', display_name: 'Qwen 2.5 7B', display_name_zh: '通义千问 2.5 7B', max_tokens: 4096, context_window: 32768, capabilities: [] },
      { model_id: 'deepseek-r1:7b', display_name: 'DeepSeek R1 7B', display_name_zh: 'DeepSeek R1 7B', max_tokens: 4096, context_window: 32768, capabilities: ['Reasoning'] },
    ]
  },
  {
    name: 'Azure OpenAI', nameZh: 'Azure OpenAI', nameEn: 'Azure OpenAI',
    platform_type: 'Azure', base_url: '',
    models: [
      { model_id: 'gpt-4o', display_name: 'GPT-4o', display_name_zh: 'GPT-4o', max_tokens: 16384, context_window: 128000, capabilities: ['Vision'] },
    ]
  },
]

export function getPresetName(preset: PlatformPreset, locale: Locale): string {
  return locale === 'zh' ? preset.nameZh : preset.nameEn
}

export function getModelDisplayName(model: ModelPreset, locale: Locale): string {
  return locale === 'zh' ? model.display_name_zh : model.display_name
}

export const CAPABILITY_OPTIONS = [
  { value: 'Reasoning', labelZh: '推理', labelEn: 'Reasoning', color: '#722ed1' },
  { value: 'Vision', labelZh: '视觉', labelEn: 'Vision', color: '#1677ff' },
  { value: 'Embedding', labelZh: '嵌入', labelEn: 'Embedding', color: '#13c2c2' },
  { value: 'Rerank', labelZh: '重排', labelEn: 'Rerank', color: '#fa8c16' },
  { value: 'Tool', labelZh: '工具', labelEn: 'Tool', color: '#52c41a' },
] as const

export function getCapabilityLabel(cap: string, locale: Locale): string {
  const opt = CAPABILITY_OPTIONS.find(c => c.value === cap)
  return locale === 'zh' ? (opt?.labelZh ?? cap) : (opt?.labelEn ?? cap)
}

export function getCapabilityColor(cap: string): string {
  const opt = CAPABILITY_OPTIONS.find(c => c.value === cap)
  return opt?.color ?? '#999'
}

export function getModelsForPlatform(platformName: string): ModelPreset[] {
  const preset = platformPresets.find(p => p.name === platformName)
  return preset?.models || []
}
