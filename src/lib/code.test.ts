import { expect, test } from 'vitest'
import type { Item } from './api'
import { checks, linked } from './code'

const item = (title: string, occurredAt: number, meta: Item['meta']): Item => ({
  id: 0,
  source: 'forge',
  externalId: title,
  kind: 'merge_request',
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

test('a repository gets the items of its remote, whatever the case, most recent first', () => {
  const items = [
    item('old', 1, { repository: 'Acme/App' }),
    item('elsewhere', 2, { repository: 'acme/api' }),
    item('no repository', 3, {}),
    item('new', 4, { repository: 'acme/app' }),
  ]
  expect(linked(items, 'acme/app').map(i => i.title)).toEqual(['new', 'old'])
  expect(linked(items, null)).toEqual([])
})

test('checks are read from the words of the forge', () => {
  const states = ['SUCCESS', 'FAILURE', 'ERROR', 'PENDING', null].map(state => checks(item('', 0, { checks: state })))
  expect(states).toEqual(['passed', 'failed', 'failed', 'running', null])
})
