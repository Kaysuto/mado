import type { ProviderKind } from './api'

export type PresetGroup = 'subscription' | 'api' | 'local' | 'custom'

export interface Preset {
  id: string
  group: PresetGroup
  kind: ProviderKind
  name: string
  baseUrl: string
  /** Whether the service takes an API key. */
  key: 'required' | 'optional' | 'none'
}

/** Ready-made providers. Anything else speaking the OpenAI protocol goes through "custom". */
export const PRESETS: readonly Preset[] = [
  { id: 'claude_code', group: 'subscription', kind: 'claude_code', name: 'Claude', baseUrl: '', key: 'none' },
  { id: 'codex', group: 'subscription', kind: 'codex', name: 'ChatGPT', baseUrl: '', key: 'none' },
  { id: 'anthropic', group: 'api', kind: 'anthropic', name: 'Anthropic', baseUrl: '', key: 'required' },
  { id: 'openai', group: 'api', kind: 'openai_compat', name: 'OpenAI', baseUrl: 'https://api.openai.com/v1', key: 'required' },
  { id: 'gemini', group: 'api', kind: 'openai_compat', name: 'Google Gemini', baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai', key: 'required' },
  { id: 'mistral', group: 'api', kind: 'openai_compat', name: 'Mistral', baseUrl: 'https://api.mistral.ai/v1', key: 'required' },
  { id: 'openrouter', group: 'api', kind: 'openai_compat', name: 'OpenRouter', baseUrl: 'https://openrouter.ai/api/v1', key: 'required' },
  { id: 'groq', group: 'api', kind: 'openai_compat', name: 'Groq', baseUrl: 'https://api.groq.com/openai/v1', key: 'required' },
  { id: 'ollama', group: 'local', kind: 'openai_compat', name: 'Ollama', baseUrl: 'http://localhost:11434/v1', key: 'none' },
  { id: 'lmstudio', group: 'local', kind: 'openai_compat', name: 'LM Studio', baseUrl: 'http://localhost:1234/v1', key: 'none' },
  { id: 'custom', group: 'custom', kind: 'openai_compat', name: '', baseUrl: '', key: 'optional' },
]

export const PRESET_GROUPS: readonly PresetGroup[] = ['subscription', 'api', 'local', 'custom']
