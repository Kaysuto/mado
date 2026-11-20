<script lang="ts">
  import { type EventDraft, cancel, createEvent, draftEvent, eventPage, openPage } from '../lib/api'
  import { dayKey } from '../lib/calendar'
  import { i18n } from '../lib/i18n.svelte'

  let { day, onadded }: { day: Date; onadded: (day: Date) => void } = $props()

  let text = $state('')
  let draft = $state<EventDraft | null>(null) // the event read from the sentence, for the user to check
  let reading = $state('') // id of the reading in progress, to stop it
  let saving = $state(false)
  let online = $state(false) // whether an online calendar has a page that creates the event
  let error = $state('')
  let stopped = false

  async function read() {
    reading = crypto.randomUUID()
    stopped = false
    error = ''
    try {
      draft = await draftEvent(reading, text)
    } catch (cause) {
      if (!stopped) {
        // No model, or an answer it could not give: the event is still one form away.
        error = `${i18n.t.calendar.byHand} (${String(cause)})`
        draft = { title: text.trim(), start: dayKey(day), end: dayKey(day), allDay: true, location: '' }
      }
    } finally {
      reading = ''
    }
    if (draft) online = await eventPage(draft).then(Boolean, () => false)
  }

  function stop() {
    stopped = true
    void cancel(reading)
  }

  function setAllDay(allDay: boolean) {
    if (!draft) return
    const first = draft.start.slice(0, 10)
    draft = allDay
      ? { ...draft, allDay, start: first, end: draft.end.slice(0, 10) }
      : { ...draft, allDay, start: `${first}T09:00`, end: `${first}T10:00` }
  }

  function close() {
    draft = null
    text = ''
    error = ''
  }

  async function save() {
    if (!draft) return
    saving = true
    error = ''
    try {
      await createEvent(draft)
      const [year, month, date] = draft.start.slice(0, 10).split('-').map(Number)
      onadded(new Date(year, month - 1, date))
      close()
    } catch (cause) {
      error = String(cause)
    } finally {
      saving = false
    }
  }

  /** The event is created on the calendar's own page; it comes back with the next sync. */
  async function createOnline() {
    if (!draft) return
    error = ''
    try {
      const page = await eventPage(draft)
      if (page) {
        openPage(page)
        close()
      }
    } catch (cause) {
      error = String(cause)
    }
  }
</script>

{#if draft}
  <form
    class="event"
    onsubmit={event => {
      event.preventDefault()
      void save()
    }}
  >
    <input
      class="field"
      placeholder={i18n.t.calendar.title}
      aria-label={i18n.t.calendar.title}
      required
      bind:value={draft.title}
    />
    <label class="check">
      <input type="checkbox" checked={draft.allDay} onchange={event => setAllDay(event.currentTarget.checked)} />
      {i18n.t.calendar.allDay}
    </label>
    <div class="row">
      {#if draft.allDay}
        <label>{i18n.t.calendar.starts} <input class="field" type="date" required bind:value={draft.start} /></label>
        <label>{i18n.t.calendar.lastDay} <input class="field" type="date" required bind:value={draft.end} /></label>
      {:else}
        <label>
          {i18n.t.calendar.starts}
          <input class="field" type="datetime-local" required bind:value={draft.start} />
        </label>
        <label>
          {i18n.t.calendar.ends}
          <input class="field" type="datetime-local" required bind:value={draft.end} />
        </label>
      {/if}
    </div>
    <input
      class="field"
      placeholder={i18n.t.calendar.place}
      aria-label={i18n.t.calendar.place}
      bind:value={draft.location}
    />
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    <div class="row end">
      <button class="btn" type="button" disabled={saving} onclick={close}>{i18n.t.calendar.discard}</button>
      {#if online}
        <button class="btn" type="button" disabled={saving} onclick={createOnline}>{i18n.t.calendar.online}</button>
      {/if}
      <button class="btn primary" type="submit" disabled={saving}>{i18n.t.calendar.save}</button>
    </div>
  </form>
{:else}
  <form
    class="row"
    onsubmit={event => {
      event.preventDefault()
      if (!reading && text.trim()) void read()
    }}
  >
    <input
      class="field grow"
      placeholder={i18n.t.calendar.sentence}
      aria-label={i18n.t.calendar.sentenceLabel}
      readonly={Boolean(reading)}
      bind:value={text}
    />
    {#if reading}
      <button class="btn" type="button" onclick={stop}>{i18n.t.calendar.stop}</button>
    {:else}
      <button class="btn" type="submit" disabled={!text.trim()}>{i18n.t.calendar.add}</button>
    {/if}
  </form>
{/if}

<style>
  .event {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 12px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  .event .row {
    margin-bottom: 0;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .end {
    justify-content: flex-end;
  }
  label {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: 12px;
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: 13px;
  }
</style>
