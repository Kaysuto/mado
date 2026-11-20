import { expect, test } from 'vitest'
import type { Item } from './api'
import { checks, linked, projectOf } from './code'

const item = (title: string, occurredAt: number, meta: Item['meta'], kind: Item['kind'] = 'merge_request'): Item => ({
  id: 0,
  source: 'forge',
  externalId: title,
  kind,
  title,
  body: '',
  author: null,
  url: null,
  state: 'open',
  triage: 'inbox',
  occurredAt,
  endsAt: null,
  meta,
  label: null,
})

const titles = (items: Item[]) => items.map(i => i.title)

test('a repository gets the items of its remote, whatever the case, most recent first', () => {
  const items = [
    item('old', 1, { repository: 'Acme/App' }),
    item('elsewhere', 2, { repository: 'acme/api' }),
    item('no repository', 3, {}),
    item('new', 4, { repository: 'acme/app' }),
  ]
  expect(titles(linked(items, { path: '', remote: 'acme/app' }))).toEqual(['new', 'old'])
  expect(linked(items, { path: '', remote: null })).toEqual([])
})

test('a repository also gets the tickets of the projects worked on in it', () => {
  const items = [
    item('by project', 1, { identifier: 'APP-1', project: 'Desktop' }, 'ticket'),
    item('by key', 2, { identifier: 'OPS-7' }, 'ticket'),
    item('other project', 3, { identifier: 'WEB-2' }, 'ticket'),
    item('not a ticket', 4, { project: 'Desktop' }),
  ]
  const links = { Desktop: '/work/app', OPS: '/work/app', WEB: '/work/site' }
  expect(titles(linked(items, { path: '/work/app', remote: null }, links))).toEqual(['by key', 'by project'])
  // A sample repository is nowhere on disk: nothing was worked on in it.
  expect(linked(items, { path: '', remote: null }, { Desktop: '' })).toEqual([])
})

test('a ticket belongs to its project, or to the key of its identifier', () => {
  expect(projectOf(item('', 0, { identifier: 'APP-12', project: 'Desktop' }))).toBe('Desktop')
  expect(projectOf(item('', 0, { identifier: 'APP-12', project: null }))).toBe('APP')
  expect(projectOf(item('', 0, { identifier: 'acme/app #9' }))).toBeNull()
})

test('checks are read from the words of the forge', () => {
  const states = ['SUCCESS', 'FAILURE', 'ERROR', 'PENDING', null].map(state => checks(item('', 0, { checks: state })))
  expect(states).toEqual(['passed', 'failed', 'failed', 'running', null])
})
