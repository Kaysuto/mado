import type { FocusTime } from './api'

/** A focus session in progress. Times are milliseconds, as `Date.now()` gives them. */
export interface FocusState {
  /** The item worked on, if any. */
  itemId: number | null
  title: string
  startedAt: number
  /** Time counted before the last pause. */
  spentMs: number
  /** When the clock was last started; null while paused. */
  runSince: number | null
}

/** Time counted so far, pauses left out. */
export function elapsed(session: FocusState, now: number): number {
  return session.spentMs + (session.runSince === null ? 0 : Math.max(0, now - session.runSince))
}

/** The session paused if it was running, running again if it was paused. */
export function toggled(session: FocusState, now: number): FocusState {
  return session.runSince === null
    ? { ...session, runSince: now }
    : { ...session, spentMs: elapsed(session, now), runSince: null }
}

export interface ProjectTime {
  project: string | null
  seconds: number
  items: FocusTime[]
}

/** Times gathered by project, most time first. What has no project comes together under `null`. */
export function byProject(times: FocusTime[]): ProjectTime[] {
  const groups = new Map<string | null, ProjectTime>()
  for (const time of times) {
    const group = groups.get(time.project) ?? { project: time.project, seconds: 0, items: [] }
    group.seconds += time.seconds
    group.items.push(time)
    groups.set(time.project, group)
  }
  return [...groups.values()].sort((a, b) => b.seconds - a.seconds)
}

/** "05:09", or "1:02:03" from the first hour on. */
export function clock(ms: number): string {
  const total = Math.floor(ms / 1000)
  const two = (value: number) => String(value).padStart(2, '0')
  const rest = `${two(Math.floor((total % 3600) / 60))}:${two(total % 60)}`
  return total >= 3600 ? `${Math.floor(total / 3600)}:${rest}` : rest
}

/** "45 min", or "1 h 20" from the first hour on. */
export function duration(seconds: number): string {
  const minutes = Math.round(seconds / 60)
  return minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, '0')}`
}
