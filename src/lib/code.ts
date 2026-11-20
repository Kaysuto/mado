import type { Engine, Item, Repository } from './api'

export const ENGINES: Record<Engine, string> = { claude_code: 'Claude Code', codex: 'Codex' }

/** The repository last used for each project of tickets: project → path. Kept in `code.links`. */
export type Links = Record<string, string>

/** What a ticket shares with the others of its project: the project's name, or the key of its
 * identifier (`APP` in `APP-12`). Null when it has neither. */
export function projectOf(item: Item): string | null {
  if (typeof item.meta.project === 'string' && item.meta.project) return item.meta.project
  const identifier = typeof item.meta.identifier === 'string' ? item.meta.identifier : ''
  return /^([A-Za-z][A-Za-z0-9]*)-\d+$/.exec(identifier)?.[1] ?? null
}

/** Whether a forge ties this item to the repository pushed to `remote`. */
export function sameRemote(item: Item, remote: string | null): boolean {
  return (
    Boolean(remote) &&
    typeof item.meta.repository === 'string' &&
    item.meta.repository.toLowerCase() === remote?.toLowerCase()
  )
}

/** What belongs under a repository, most recent first: what a forge ties to its remote (merge
 * requests, tickets, alerts), and the tickets of the projects the user worked on in it. */
export function linked(items: Item[], repository: Pick<Repository, 'path' | 'remote'>, links: Links = {}): Item[] {
  const here = (item: Item) => {
    const project = item.kind === 'ticket' ? projectOf(item) : null
    return Boolean(repository.path) && project !== null && links[project] === repository.path
  }
  return items
    .filter(item => sameRemote(item, repository.remote) || here(item))
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
