import { expect, test } from 'vitest'
import type { Item } from './api'
import { groupTasks } from './tasks'

function item(title: string, extra: Partial<Item> = {}): Item {
  return {
    id: 0,
    source: 'tracker',
    externalId: title,
    kind: 'ticket',
    title,
    body: '',
    author: null,
    url: null,
    state: '',
    triage: 'inbox',
    occurredAt: 0,
    endsAt: null,
    meta: {},
    label: null,
    ...extra,
  }
}

const titles = (items: Item[]) => items.map(i => i.title)

test('tickets in progress, things to do and finished tasks are told apart', () => {
  const groups = groupTasks([
    item('Started ticket', { meta: { stateType: 'started' } }),
    item('Backlog ticket', { meta: { stateType: 'backlog' } }),
    item('My task', { source: 'local', kind: 'task', state: 'todo' }),
    item('My finished task', { source: 'local', kind: 'task', state: 'done' }),
  ])
  expect(titles(groups.doing)).toEqual(['Started ticket'])
  expect(titles(groups.todo)).toEqual(['Backlog ticket', 'My task'])
  expect(titles(groups.done)).toEqual(['My finished task'])
})

test('most urgent first, then most recent; no priority goes last', () => {
  const groups = groupTasks([
    item('No priority, recent', { occurredAt: 50 }),
    item('Low', { meta: { priority: 4 } }),
    item('Urgent, old', { meta: { priority: 1 }, occurredAt: 1 }),
    item('Urgent, recent', { meta: { priority: 1 }, occurredAt: 9 }),
    item('Priority zero means none', { meta: { priority: 0 }, occurredAt: 40 }),
  ])
  expect(titles(groups.todo)).toEqual([
    'Urgent, recent',
    'Urgent, old',
    'Low',
    'No priority, recent',
    'Priority zero means none',
  ])
})
