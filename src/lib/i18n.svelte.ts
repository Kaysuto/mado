import { getSetting, setSetting } from './api'
import { LOCALES, type Locale, pickLocale } from './locale'
import { en } from './locales/en'
import { fr } from './locales/fr'

const dictionaries = { en, fr }

// The system language until the saved choice, if any, is read.
let locale = $state<Locale>(pickLocale(navigator.languages))

export const i18n = {
  get locale() {
    return locale
  },
  /** Changes the language and remembers it. */
  set locale(value: Locale) {
    locale = value
    void setSetting('locale', value)
  },
  /** Strings of the current language. */
  get t() {
    return dictionaries[locale]
  },
}

/** Applies the language the user chose on a previous run. */
export async function restoreLocale() {
  const saved = await getSetting<string>('locale')
  const match = LOCALES.find(l => l === saved)
  if (match) locale = match
}
