const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['day', 86_400],
  ['hour', 3_600],
  ['minute', 60],
]

/** "3 hours ago", "in 2 days"… Both times are Unix seconds. */
export function relativeTime(at: number, now: number, locale: string): string {
  const format = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' })
  const diff = at - now
  for (const [unit, seconds] of UNITS) {
    if (Math.abs(diff) >= seconds) return format.format(Math.trunc(diff / seconds), unit)
  }
  return format.format(0, 'minute')
}
