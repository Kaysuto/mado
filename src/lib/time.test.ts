import { expect, test } from 'vitest'
import { relativeTime } from './time'

const NOW = 1_000_000

test('uses the largest unit that fits, in the past and in the future', () => {
  expect(relativeTime(NOW - 25 * 60, NOW, 'en')).toBe('25 minutes ago')
  expect(relativeTime(NOW - 3 * 3600, NOW, 'en')).toBe('3 hours ago')
  expect(relativeTime(NOW + 2 * 86_400, NOW, 'en')).toBe('in 2 days')
})

test('rounds towards now', () => {
  expect(relativeTime(NOW - 119 * 60, NOW, 'en')).toBe('1 hour ago')
})

test('follows the language', () => {
  expect(relativeTime(NOW - 3 * 3600, NOW, 'fr')).toBe('il y a 3 heures')
  expect(relativeTime(NOW - 86_400, NOW, 'fr')).toBe('hier')
})

test('anything under a minute is now', () => {
  expect(relativeTime(NOW - 20, NOW, 'en')).toBe('this minute')
})
