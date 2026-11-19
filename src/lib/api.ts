// The only door between the interface and the Rust core. Types mirror `src-tauri/src/items.rs`.
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { openUrl } from '@tauri-apps/plugin-opener'

export type Kind =
  | 'mail'
  | 'ticket'
  | 'merge_request'
  | 'notification'
  | 'event'
  | 'task'
  | 'note'
  | 'message'
  | 'alert'
  | 'article'

/** Where the user stands with an item. Kept across syncs. */
export type Triage = 'inbox' | 'done' | 'later'

export interface Item {
  id: number
  source: string
  externalId: string
  kind: Kind
  title: string
  body: string
  author: string | null
  url: string | null
  /** State in the source's own words. */
  state: string
  triage: Triage
  /** Unix seconds: received, created or starting. */
  occurredAt: number
  endsAt: number | null
  meta: Record<string, unknown>
}

export interface ItemQuery {
  kinds?: Kind[]
  triage?: Triage
  search?: string
  /** Unix seconds: keeps what is still going on at `since` and what starts before `until`. */
  since?: number
  until?: number
  limit?: number
}

/** A source that could not be synced, and why. */
export interface SyncFailure {
  source: string
  message: string
}

export const listItems = (query: ItemQuery) => invoke<Item[]>('list_items', { query })
export const setTriage = (id: number, triage: Triage) => invoke<void>('set_triage', { id, triage })
export const getSetting = <T>(key: string) => invoke<T | null>('get_setting', { key })
export const setSetting = (key: string, value: unknown) => invoke<void>('set_setting', { key, value })
export const syncNow = () => invoke<SyncFailure[]>('sync_now')

/** What arrives from outside and waits for a decision. Tasks, notes, events and articles have their own spaces. */
export const INBOX_KINDS: Kind[] = ['mail', 'merge_request', 'notification', 'ticket', 'message', 'alert']

export const createTask = (title: string) => invoke<void>('create_task', { title })
export const setTaskDone = (id: number, done: boolean) => invoke<void>('set_task_done', { id, done })
export const deleteTask = (id: number) => invoke<void>('delete_task', { id })

/** Opens a web page in the browser. Anything that is not an https address is ignored. */
export function openPage(url: string | null) {
  if (url && /^https:\/\/\S+$/.test(url)) void openUrl(url)
}

/** What a source can do to one of its items. Each item lists its own in `meta.actions`. */
export const ITEM_ACTIONS = ['mark_read', 'archive'] as const
export type ItemAction = (typeof ITEM_ACTIONS)[number]

export const actOnItem = (id: number, action: ItemAction) => invoke<void>('act_on_item', { id, action })

// ---------- sources (types mirror `src-tauri/src/connectors/mod.rs`) ----------

export type SourceKind = 'github' | 'linear' | 'ical' | 'imap'

export interface Source {
  id: string
  kind: SourceKind
  name: string
  /** What this kind of source needs besides its secret (a mail server, a user name…). */
  options: Record<string, unknown>
}

/** A source as stored. The token itself never leaves the core. */
export interface SourceView extends Source {
  hasToken: boolean
  /** Why its last sync failed, if it did. */
  error: string | null
}

export const listSources = () => invoke<SourceView[]>('list_sources')
/** `token` replaces the stored one; leaving it out keeps the current one. */
export const saveSource = (source: Source, token?: string) => invoke<void>('save_source', { source, token })
export const deleteSource = (id: string) => invoke<void>('delete_source', { id })

// ---------- models (types mirror `src-tauri/src/llm/mod.rs`) ----------

export type ProviderKind = 'anthropic' | 'openai_compat' | 'claude_code' | 'codex'

export interface Provider {
  id: string
  kind: ProviderKind
  name: string
  /** Required for `openai_compat`; empty otherwise. */
  baseUrl: string
}

/** A provider as stored. The key itself never leaves the core. */
export interface ProviderView extends Provider {
  hasKey: boolean
  /** Subscription providers: whether their command-line tool is installed. */
  installed: boolean | null
}

export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
}

export interface AskRequest {
  model: string
  system?: string
  messages: ChatMessage[]
}

export interface Reply {
  text: string
  inputTokens: number | null
  outputTokens: number | null
}

export const listProviders = () => invoke<ProviderView[]>('list_providers')
/** `key` replaces the stored API key; leaving it out keeps the current one. */
export const saveProvider = (provider: Provider, key?: string) =>
  invoke<void>('save_provider', { provider, key })
export const deleteProvider = (id: string) => invoke<void>('delete_provider', { id })
export const listModels = (providerId: string) => invoke<string[]>('list_models', { providerId })
/** `requestId` is any unique string; pass it to `cancel` to stop the call. */
export const ask = (requestId: string, providerId: string, request: AskRequest) =>
  invoke<Reply>('ask', { requestId, providerId, request })
export const cancel = (requestId: string) => invoke<void>('cancel', { requestId })

/** Calls `handler` whenever stored items may have changed. Resolves to the function that stops it. */
export const onItemsChanged = (handler: () => void): Promise<UnlistenFn> =>
  listen('items-changed', handler)
