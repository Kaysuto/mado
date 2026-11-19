import { expect, test } from 'vitest'
import { pickLocale } from './locale'

test('picks the first supported language in preference order', () => {
  expect(pickLocale(['fr-FR', 'en-US'])).toBe('fr')
  expect(pickLocale(['de-DE', 'en-GB', 'fr'])).toBe('en')
})

test('falls back to English', () => {
  expect(pickLocale([])).toBe('en')
  expect(pickLocale(['ja-JP'])).toBe('en')
})
