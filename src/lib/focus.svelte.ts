import { type Item, focusTotal, getSetting, logFocus, setSetting } from './api'
import { startOfDay } from './calendar'
import { type FocusState, elapsed, toggled } from './focus'

let session = $state<FocusState | null>(null)
let today = $state(0)

// Saved at every change, so a session in progress survives a restart.
const save = () => void setSetting('focus', session)

async function refresh() {
  today = await focusTotal(startOfDay(new Date()).getTime() / 1000)
}

/** The focus session in progress, shared by the sidebar and by every item that can start one. */
export const focus = {
  get session() {
    return session
  },
  /** Seconds of finished focus today. */
  get today() {
    return today
  },

  async restore() {
    const saved = await getSetting<FocusState>('focus')
    if (saved && typeof saved.startedAt === 'number') session = saved
    await refresh()
  },

  /** Starts a session, on `item` or on nothing in particular. One in progress is finished first. */
  async start(item?: Item) {
    await focus.finish()
    const now = Date.now()
    session = { itemId: item?.id ?? null, title: item?.title ?? '', startedAt: now, spentMs: 0, runSince: now }
    save()
  },

  /** Pauses the session, or takes it up again. */
  toggle() {
    if (!session) return
    session = toggled(session, Date.now())
    save()
  },

  /** Ends the session and records its time. If recording fails, the session is kept. */
  async finish() {
    if (!session) return
    const now = Date.now()
    const seconds = Math.floor(elapsed(session, now) / 1000)
    if (seconds >= 1) {
      await logFocus({
        itemId: session.itemId,
        title: session.title,
        startedAt: Math.floor(session.startedAt / 1000),
        endedAt: Math.ceil(now / 1000),
        seconds,
      })
    }
    session = null
    save()
    await refresh()
  },

  /** Drops the session: nothing is recorded. */
  cancel() {
    session = null
    save()
  },
}
