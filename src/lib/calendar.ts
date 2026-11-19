import type { Item } from './api'

const DAY = 86_400

/** Midnight (local time) of the given day. */
export function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate())
}

/** The same day, `days` later (or earlier). Works across month ends and clock changes. */
export function addDays(date: Date, days: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days)
}

/** The 42 days shown for a month: six weeks from the Monday on or before the 1st. */
export function monthGrid(year: number, month: number): Date[] {
  const first = new Date(year, month, 1)
  const offset = (first.getDay() + 6) % 7 // Monday = 0
  return Array.from({ length: 42 }, (_, index) => new Date(year, month, 1 - offset + index))
}

const seconds = (date: Date) => date.getTime() / 1000

/** Whether an event is going on at some point of the day. An event without an end is a moment. */
export function happensOn(event: Item, day: Date): boolean {
  const start = seconds(startOfDay(day))
  const end = seconds(addDays(day, 1))
  const eventEnd = event.endsAt ?? event.occurredAt + 1
  return event.occurredAt < end && eventEnd > start
}

/** The events of a day: whole-day ones first, then by start time. */
export function eventsOn(events: Item[], day: Date): Item[] {
  return events
    .filter(event => happensOn(event, day))
    .sort(
      (a, b) => Number(b.meta.allDay === true) - Number(a.meta.allDay === true) || a.occurredAt - b.occurredAt,
    )
}

/** Whether a whole-day event really is a single day, to show it without a date range. */
export function isSingleDay(event: Item): boolean {
  return event.endsAt === null || event.endsAt - event.occurredAt <= DAY
}
