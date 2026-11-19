// The only door between the interface and the Rust core. Types mirror `src-tauri/src/items.rs`.
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

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

/** Calls `handler` whenever stored items may have changed. Resolves to the function that stops it. */
export const onItemsChanged = (handler: () => void): Promise<UnlistenFn> =>
  listen('items-changed', handler)
