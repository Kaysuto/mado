import { expect, test } from 'vitest'
import type { Item } from './api'
import { dayKey, eventsOn, monthGrid } from './calendar'

test('a day is written the way date fields want it, in local time', () => {
  expect(dayKey(new Date(2026, 0, 5, 23, 30))).toBe('2026-01-05')
})

function event(title: string, start: Date, end: Date | null, allDay = false): Item {
  return {
    id: 0,
    source: 'calendar',
    externalId: title,
    kind: 'event',
    title,
    body: '',
    author: null,
    url: null,
    state: '',
    triage: 'inbox',
    occurredAt: start.getTime() / 1000,
    endsAt: end ? end.getTime() / 1000 : null,
    meta: { allDay },
    label: null,
  }
}

const ymd = (date: Date) => [date.getFullYear(), date.getMonth() + 1, date.getDate()]

test('a month is shown as six weeks starting on a Monday', () => {
  // November 2026 starts on a Sunday: the grid starts on Monday 26 October.
  const grid = monthGrid(2026, 10)
  expect(grid).toHaveLength(42)
  expect(ymd(grid[0])).toEqual([2026, 10, 26])
  expect(grid.every((day, index) => day.getDay() === (index + 1) % 7)).toBe(true)
  expect(ymd(grid[41])).toEqual([2026, 12, 6])
})

test('a month starting on a Monday starts the grid', () => {
  expect(ymd(monthGrid(2026, 5)[0])).toEqual([2026, 6, 1])
})

test('a day lists what happens on it, whole days first, then by time', () => {
  const day = new Date(2026, 10, 19)
  const events = [
    event('Afternoon', new Date(2026, 10, 19, 15), new Date(2026, 10, 19, 16)),
    event('Morning', new Date(2026, 10, 19, 9), new Date(2026, 10, 19, 10)),
    event('Holiday', new Date(2026, 10, 19), new Date(2026, 10, 20), true),
    event('Trip', new Date(2026, 10, 18), new Date(2026, 10, 21), true),
    event('Reminder without end', new Date(2026, 10, 19, 12), null),
    event('Yesterday', new Date(2026, 10, 18, 9), new Date(2026, 10, 18, 10)),
    event('Tomorrow', new Date(2026, 10, 20, 9), new Date(2026, 10, 20, 10)),
    // A whole-day event ends at the next midnight: it must not spill onto the next day.
    event('Day before, whole day', new Date(2026, 10, 18), new Date(2026, 10, 19), true),
  ]
  expect(eventsOn(events, day).map(e => e.title)).toEqual([
    'Trip',
    'Holiday',
    'Morning',
    'Reminder without end',
    'Afternoon',
  ])
})
