import { type Locale, pickLocale } from './locale'
import { en } from './locales/en'
import { fr } from './locales/fr'

const dictionaries = { en, fr }

let locale = $state<Locale>(pickLocale(navigator.languages))

export const i18n = {
  get locale() {
    return locale
  },
  set locale(value: Locale) {
    locale = value
  },
  /** Strings of the current language. */
  get t() {
    return dictionaries[locale]
  },
}
