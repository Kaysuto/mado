import type { Item } from './api'

export const TASK_GROUPS = ['doing', 'todo', 'done'] as const
export type TaskGroup = (typeof TASK_GROUPS)[number]

/** Where a task or a ticket stands, whatever tool it comes from. */
export function groupOf(item: Item): TaskGroup {
  if (item.source === 'local') return item.state === 'done' ? 'done' : 'todo'
  return item.meta.stateType === 'started' ? 'doing' : 'todo'
}

/** 1 is urgent, 4 is low; no priority sorts last. */
const rank = (item: Item) => (typeof item.meta.priority === 'number' && item.meta.priority) || 5

/** Splits the list by progress. In a group: most urgent first, then most recent. */
export function groupTasks(items: Item[]): Record<TaskGroup, Item[]> {
  const groups: Record<TaskGroup, Item[]> = { doing: [], todo: [], done: [] }
  for (const item of items) groups[groupOf(item)].push(item)
  for (const group of TASK_GROUPS) {
    groups[group].sort((a, b) => rank(a) - rank(b) || b.occurredAt - a.occurredAt)
  }
  return groups
}
