export const LOCALES = ['en', 'fr'] as const
export type Locale = (typeof LOCALES)[number]

// First supported language in the user's preference order; English otherwise.
export function pickLocale(preferred: readonly string[]): Locale {
  for (const tag of preferred) {
    const base = tag.toLowerCase().split('-')[0]
    const match = LOCALES.find(l => l === base)
    if (match) return match
  }
  return 'en'
}
