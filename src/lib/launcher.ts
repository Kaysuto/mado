import type { Kind } from './api'
import type { Space } from './spaces'

/** Lower case and without accents: what two texts are compared on. */
const plain = (text: string) => text.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase()

/** Whether every word of `query` is somewhere in `label`, whatever the case and the accents. */
export function matches(label: string, query: string) {
  const text = plain(label)
  return plain(query)
    .split(/\s+/)
    .every(word => text.includes(word))
}

/** The space where an item of each kind is found. */
export const SPACE_OF: Record<Kind, Space> = {
  mail: 'inbox',
  merge_request: 'inbox',
  notification: 'inbox',
  message: 'inbox',
  alert: 'inbox',
  ticket: 'tasks',
  task: 'tasks',
  event: 'calendar',
  note: 'notes',
  article: 'news',
}
