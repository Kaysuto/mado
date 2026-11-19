<script lang="ts">
  import { onMount } from 'svelte'
  import { type Item, listItems, onItemsChanged, openPage } from '../lib/api'
  import { addDays, eventsOn, isSingleDay, monthGrid, startOfDay } from '../lib/calendar'
  import { i18n } from '../lib/i18n.svelte'

  const today = startOfDay(new Date())
  let month = $state(new Date(today.getFullYear(), today.getMonth(), 1))
  let selected = $state(today)
  let events = $state<Item[]>([])
  let error = $state('')

  const grid = $derived(monthGrid(month.getFullYear(), month.getMonth()))
  const agenda = $derived(eventsOn(events, selected))

  const monthName = $derived(new Intl.DateTimeFormat(i18n.locale, { month: 'long', year: 'numeric' }).format(month))
  const dayName = $derived(
    new Intl.DateTimeFormat(i18n.locale, { weekday: 'long', day: 'numeric', month: 'long' }).format(selected),
  )
  // 5 January 2026 is a Monday: the week header follows the language.
  const weekdays = $derived(
    Array.from({ length: 7 }, (_, index) =>
      new Intl.DateTimeFormat(i18n.locale, { weekday: 'short' }).format(new Date(2026, 0, 5 + index)),
    ),
  )
  const clock = $derived(new Intl.DateTimeFormat(i18n.locale, { hour: '2-digit', minute: '2-digit' }))

  const same = (a: Date, b: Date) => a.getTime() === b.getTime()

  let latest = 0
  async function load() {
    const request = ++latest
    try {
      const found = await listItems({
        kinds: ['event'],
        since: grid[0].getTime() / 1000,
        until: addDays(grid[41], 1).getTime() / 1000,
        limit: 1000,
      })
      if (request === latest) {
        events = found
        error = ''
      }
    } catch (cause) {
      if (request === latest) error = String(cause)
    }
  }

  // Runs again whenever another month is shown.
  $effect(() => {
    void load()
  })

  onMount(() => {
    const listening = onItemsChanged(load)
    return () => void listening.then(stop => stop())
  })

  function move(months: number) {
    month = new Date(month.getFullYear(), month.getMonth() + months, 1)
  }

  function goToday() {
    month = new Date(today.getFullYear(), today.getMonth(), 1)
    selected = today
  }

  function pick(day: Date) {
    selected = day
    if (day.getMonth() !== month.getMonth()) month = new Date(day.getFullYear(), day.getMonth(), 1)
  }

  function when(event: Item): string {
    if (event.meta.allDay === true) return i18n.t.calendar.allDay
    const start = clock.format(event.occurredAt * 1000)
    return event.endsAt ? `${start} – ${clock.format(event.endsAt * 1000)}` : start
  }
</script>

<header>
  <h1>{i18n.t.spaces.calendar}</h1>
  <div class="nav">
    <button class="btn" aria-label={i18n.t.calendar.previous} onclick={() => move(-1)}>‹</button>
    <span class="month" aria-live="polite">{monthName}</span>
    <button class="btn" aria-label={i18n.t.calendar.next} onclick={() => move(1)}>›</button>
    <button class="btn" onclick={goToday}>{i18n.t.calendar.today}</button>
  </div>
</header>

{#if error}
  <p class="error" role="alert">{error}</p>
{/if}

<div class="layout">
  <div class="grid">
    {#each weekdays as weekday (weekday)}
      <span class="weekday">{weekday}</span>
    {/each}
    {#each grid as day (day.getTime())}
      {@const count = eventsOn(events, day).length}
      <button
        class="day"
        class:other={day.getMonth() !== month.getMonth()}
        class:today={same(day, today)}
        aria-pressed={same(day, selected)}
        aria-label="{day.toLocaleDateString(i18n.locale, { day: 'numeric', month: 'long' })}, {count} {i18n.t.calendar
          .events}"
        onclick={() => pick(day)}
      >
        <span>{day.getDate()}</span>
        {#if count > 0}<span class="dot" aria-hidden="true"></span>{/if}
      </button>
    {/each}
  </div>

  <section aria-label={dayName}>
    <h2>{dayName}</h2>
    {#if agenda.length === 0}
      <p class="hint">{i18n.t.calendar.free}</p>
    {/if}
    <ul>
      {#each agenda as event (event.id)}
        <li>
          <span class="when">{when(event)}</span>
          <span class="what">
            <span class="title">{event.title || i18n.t.calendar.untitled}</span>
            {#if typeof event.meta.location === 'string' && event.meta.location}
              <span class="hint">{event.meta.location}</span>
            {/if}
            {#if event.meta.allDay === true && !isSingleDay(event)}
              <span class="hint">
                {new Date(event.occurredAt * 1000).toLocaleDateString(i18n.locale, { day: 'numeric', month: 'short' })}
                – {addDays(new Date(event.endsAt! * 1000), -1).toLocaleDateString(i18n.locale, {
                  day: 'numeric',
                  month: 'short',
                })}
              </span>
            {/if}
          </span>
          {#if typeof event.meta.link === 'string'}
            <button class="btn join" onclick={() => openPage(event.meta.link as string)}>{i18n.t.calendar.join}</button>
          {/if}
          {#if event.url}
            <button class="btn" onclick={() => openPage(event.url)}>{i18n.t.open}</button>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
</div>

<style>
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 16px 32px;
    margin-bottom: 20px;
  }
  h1 {
    margin: 0;
    font-size: 24px;
    font-weight: 600;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .month {
    min-width: 150px;
    font-weight: 600;
    text-align: center;
  }
  .layout {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 32px 48px;
  }
  .grid {
    display: grid;
    flex: 0 1 420px;
    grid-template-columns: repeat(7, 1fr);
    gap: 4px;
  }
  .weekday {
    padding-bottom: 4px;
    color: var(--muted);
    font-size: 11px;
    font-weight: 600;
    text-align: center;
    text-transform: uppercase;
  }
  .day {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    aspect-ratio: 1;
    border-radius: 10px;
    font-size: 13px;
  }
  .day:hover {
    background: var(--hover);
  }
  .day.other {
    color: var(--muted);
    opacity: 0.6;
  }
  .day.today {
    color: var(--accent);
    font-weight: 700;
  }
  .day[aria-pressed='true'] {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--accent);
  }
  section {
    flex: 1 1 320px;
    min-width: 0;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 600;
  }
  /* Month and day names come lowercase in some languages; only the first letter is raised. */
  .month::first-letter,
  h2::first-letter {
    text-transform: uppercase;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }
  .when {
    flex: none;
    width: 108px;
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  .title {
    font-weight: 600;
  }
  .hint {
    color: var(--muted);
    font-size: 12px;
  }
  .join {
    border-color: transparent;
    background: var(--accent);
    color: #111113;
  }
  .join:hover {
    background: var(--accent);
    filter: brightness(1.1);
  }
  .error {
    color: var(--danger);
  }
</style>
