import { expect, test } from 'vitest'
import { type FocusState, byProject, clock, duration, elapsed, toggled } from './focus'

const running: FocusState = { itemId: null, title: '', startedAt: 1_000, spentMs: 0, runSince: 1_000 }

test('pauses are left out of the time counted', () => {
  const paused = toggled(running, 61_000)
  expect(paused.runSince).toBeNull()
  expect(elapsed(paused, 500_000)).toBe(60_000)

  const resumed = toggled(paused, 500_000)
  expect(elapsed(resumed, 530_000)).toBe(90_000)
})

test('a clock read before its own start counts nothing', () => {
  expect(elapsed(running, 900)).toBe(0)
})

test('times are gathered by project, the one that took the most first', () => {
  const time = (title: string, project: string | null, seconds: number) => ({ itemId: 1, title, project, seconds })
  const groups = byProject([
    time('Login', 'Mobile', 900),
    time('Reading', null, 1_000),
    time('Signup', 'Mobile', 600),
    time('', null, 100),
  ])
  expect(groups.map(group => [group.project, group.seconds])).toEqual([
    ['Mobile', 1_500],
    [null, 1_100],
  ])
  expect(groups[0].items.map(item => item.title)).toEqual(['Login', 'Signup'])
})

test('the clock shows hours only once there is one', () => {
  expect(clock(309_000)).toBe('05:09')
  expect(clock(3_723_000)).toBe('1:02:03')
})

test('a duration is given in minutes, then in hours and minutes', () => {
  expect(duration(0)).toBe('0 min')
  expect(duration(2_700)).toBe('45 min')
  expect(duration(4_800)).toBe('1 h 20')
  expect(duration(7_260)).toBe('2 h 01')
})
