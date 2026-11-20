import type { Item } from './api'

/** What a forge ties to this repository (merge requests, tickets, alerts), most recent first. */
export function linked(items: Item[], remote: string | null): Item[] {
  if (!remote) return []
  const name = remote.toLowerCase()
  return items
    .filter(item => typeof item.meta.repository === 'string' && item.meta.repository.toLowerCase() === name)
    .sort((a, b) => b.occurredAt - a.occurredAt)
}

export type Checks = 'passed' | 'failed' | 'running'

/** How the automated checks of a merge request went; null when it has none. */
export function checks(item: Item): Checks | null {
  switch (item.meta.checks) {
    case 'SUCCESS':
      return 'passed'
    case 'FAILURE':
    case 'ERROR':
      return 'failed'
    case 'PENDING':
    case 'EXPECTED':
      return 'running'
    default:
      return null
  }
}
