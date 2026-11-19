import type { en } from './locales/en'

/** The spaces of the app, in sidebar order. Alt+1…9 jumps to them. */
export const SPACES = [
  'today',
  'inbox',
  'tasks',
  'calendar',
  'code',
  'news',
  'notes',
  'review',
  'automations',
] as const satisfies readonly (keyof typeof en.spaces)[]

export type Space = (typeof SPACES)[number]
