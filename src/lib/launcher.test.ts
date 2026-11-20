import { expect, test } from 'vitest'
import { matches } from './launcher'

test('a label matches whatever the case, the accents and the order of the words', () => {
  expect(matches('Boîte', 'boi')).toBe(true)
  expect(matches('Réunion d’équipe', 'EQUIPE reu')).toBe(true)
  expect(matches('Agenda', 'agenda demain')).toBe(false)
})

test('everything matches while nothing is typed', () => {
  expect(matches('Notes', '')).toBe(true)
  expect(matches('Notes', '   ')).toBe(true)
})
