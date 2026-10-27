import type { Locale } from './i18n'

export interface PlatformPreset {
  name: string
  nameZh: string
  nameEn: string
  platform_type: string
  base_url: string
  models: ModelPreset[]
  /** 请求构造方式：OpenAI / Anthropic / Trae / WorkBuddy。缺省由 platform_type 推出 */
  adapter?: string
  /** 额度计量单位：none / credits / currency / requests。缺省 none */
  credit_unit?: string
  /** 私有协议上游（无官方 API，逆向客户端协议）——UI 要显示风险提示 */
  proprietary?: boolean
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
  // ---- 新增：Agnes AI（国内 / 海外两套独立站点与 base_url）----
  {
    name: 'Agnes AI（海外）', nameZh: 'Agnes AI（海外）', nameEn: 'Agnes AI (Global)',
    platform_type: 'OpenAI', base_url: 'https://apihub.agnes-ai.com/v1',
    adapter: 'OpenAI', credit_unit: 'requests',
    models: [
      { model_id: 'agnes-3.0-flash', display_name: 'Agnes 3.0 Flash', display_name_zh: 'Agnes 3.0 Flash', max_tokens: 8192, context_window: 128000, capabilities: ['Tool'] },
      { model_id: 'agnes-2.5-flash', display_name: 'Agnes 2.5 Flash', display_name_zh: 'Agnes 2.5 Flash', max_tokens: 8192, context_window: 128000, capabilities: ['Tool'] },
      { model_id: 'agnes-2.5-pro', display_name: 'Agnes 2.5 Pro', display_name_zh: 'Agnes 2.5 Pro', max_tokens: 8192, context_window: 128000, capabilities: ['Tool', 'Reasoning'] },
    ]
  },
  {
    name: 'Agnes AI（国内）', nameZh: 'Agnes AI（国内）', nameEn: 'Agnes AI (CN)',
    platform_type: 'OpenAI', base_url: 'https://api.agnes-ai.cn/v1',
    adapter: 'OpenAI', credit_unit: 'currency',
    models: [
      { model_id: 'agnes-3.0-flash', display_name: 'Agnes 3.0 Flash', display_name_zh: 'Agnes 3.0 Flash', max_tokens: 8192, context_window: 128000, capabilities: ['Tool'] },
      { model_id: 'agnes-2.5-flash', display_name: 'Agnes 2.5 Flash', display_name_zh: 'Agnes 2.5 Flash', max_tokens: 8192, context_window: 128000, capabilities: ['Tool'] },
    ]
  },
  // ---- 新增：魔搭社区（用户口语称「魔塔」）----
  {
    name: 'ModelScope 魔搭', nameZh: '魔搭社区', nameEn: 'ModelScope',
    platform_type: 'OpenAI', base_url: 'https://api-inference.modelscope.cn/v1',
    adapter: 'OpenAI', credit_unit: 'requests',
    models: [
      { model_id: 'Qwen/Qwen3-235B-A22B-Instruct', display_name: 'Qwen3 235B', display_name_zh: '通义千问3 235B', max_tokens: 8192, context_window: 131072, capabilities: ['Tool'] },
      { model_id: 'deepseek-ai/DeepSeek-V4-Pro', display_name: 'DeepSeek V4 Pro', display_name_zh: 'DeepSeek V4 Pro', max_tokens: 8192, context_window: 131072, capabilities: ['Reasoning'] },
    ]
  },
  // ---- 新增：Trae（私有协议，无官方 API）----
  {
    name: 'Trae（国内）', nameZh: 'Trae（国内）', nameEn: 'Trae (CN)',
    platform_type: 'OpenAI', base_url: 'https://trae-api-cn.mchost.guru',
    adapter: 'Trae', credit_unit: 'requests', proprietary: true,
    models: [
      { model_id: 'glm-5.2', display_name: 'GLM-5.2', display_name_zh: 'GLM-5.2', max_tokens: 8192, context_window: 200000, capabilities: ['Tool'] },
      { model_id: 'glm-5.3', display_name: 'GLM-5.3', display_name_zh: 'GLM-5.3', max_tokens: 8192, context_window: 200000, capabilities: ['Tool'] },
      { model_id: 'kimi-k3', display_name: 'Kimi K3', display_name_zh: 'Kimi K3', max_tokens: 8192, context_window: 200000, capabilities: ['Tool'] },
      { model_id: 'DeepSeek-V4-Pro', display_name: 'DeepSeek V4 Pro', display_name_zh: 'DeepSeek V4 Pro', max_tokens: 8192, context_window: 200000, capabilities: ['Reasoning'] },
    ]
  },
  {
    name: 'Trae（海外）', nameZh: 'Trae（海外）', nameEn: 'Trae (Global)',
    platform_type: 'OpenAI', base_url: 'https://core-normal.traeapi.us',
    adapter: 'Trae', credit_unit: 'currency', proprietary: true,
    models: [
      { model_id: 'gpt-5.4', display_name: 'GPT-5.4', display_name_zh: 'GPT-5.4', max_tokens: 8192, context_window: 200000, capabilities: ['Tool'] },
      { model_id: 'gpt-5.2', display_name: 'GPT-5.2', display_name_zh: 'GPT-5.2', max_tokens: 8192, context_window: 200000, capabilities: ['Tool'] },
      { model_id: 'gemini-3.1-pro', display_name: 'Gemini 3.1 Pro', display_name_zh: 'Gemini 3.1 Pro', max_tokens: 8192, context_window: 1000000, capabilities: ['Tool'] },
    ]
  },
  // ---- 新增：WorkBuddy（私有协议，上游正在收紧）----
  {
    name: 'WorkBuddy（国内）', nameZh: 'WorkBuddy（国内）', nameEn: 'WorkBuddy (CN)',
    platform_type: 'OpenAI', base_url: 'https://copilot.tencent.com',
    adapter: 'WorkBuddy', credit_unit: 'credits', proprietary: true,
    models: [
      { model_id: 'glm-5.3', display_name: 'GLM-5.3', display_name_zh: 'GLM-5.3', max_tokens: 8192, context_window: 200000, capabilities: ['Tool'] },
      { model_id: 'deepseek-v4-pro', display_name: 'DeepSeek V4 Pro', display_name_zh: 'DeepSeek V4 Pro', max_tokens: 8192, context_window: 1000000, capabilities: ['Reasoning'] },
      { model_id: 'kimi-k3', display_name: 'Kimi K3', display_name_zh: 'Kimi K3', max_tokens: 8192, context_window: 200000, capabilities: ['Tool'] },
    ]
  },
  {
    name: 'WorkBuddy（海外）', nameZh: 'WorkBuddy（海外）', nameEn: 'WorkBuddy (Global)',
    platform_type: 'OpenAI', base_url: 'https://www.workbuddy.ai',
    adapter: 'WorkBuddy', credit_unit: 'credits', proprietary: true,
    models: [
      { model_id: 'glm-5.3', display_name: 'GLM-5.3', display_name_zh: 'GLM-5.3', max_tokens: 8192, context_window: 200000, capabilities: ['Tool'] },
      { model_id: 'claude-sonnet-4.6', display_name: 'Claude Sonnet 4.6', display_name_zh: 'Claude Sonnet 4.6', max_tokens: 8192, context_window: 200000, capabilities: ['Tool', 'Vision'] },
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
