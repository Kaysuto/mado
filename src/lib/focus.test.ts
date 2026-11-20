import { expect, test } from 'vitest'
import { type FocusState, clock, duration, elapsed, toggled } from './focus'

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
