<script lang="ts">
  import { onMount } from 'svelte'
  import {
    INBOX_KINDS,
    type Item,
    briefing,
    cancel,
    getSetting,
    listItems,
    onItemsChanged,
    openLinks,
    openPage,
    setSetting,
  } from '../lib/api'
  import { addDays, eventsOn, formatWhen, startOfDay } from '../lib/calendar'
  import { i18n } from '../lib/i18n.svelte'
  import type { Locale } from '../lib/locale'
  import { bullets, markdown, sections } from '../lib/markdown'
  import type { Space } from '../lib/spaces'
  import { groupTasks } from '../lib/tasks'

  let { onshow }: { onshow: (view: Space | 'settings') => void } = $props()

  interface Briefing {
    date: string
    lang: Locale
    text: string
  }

  const today = startOfDay(new Date())
  // The local day, as a key for what is kept "for today".
  const date = [today.getFullYear(), today.getMonth() + 1, today.getDate()].join('-')
  const evening = new Date().getHours() >= 18

  let events = $state<Item[]>([])
  let waiting = $state<Item[]>([])
  let inProgress = $state(0)
  let brief = $state<Briefing | null>(null)
  let done = $state<string[]>([])
  let hasModel = $state(true)
  let writing = $state('') // id of the briefing being written, to cancel it
  let error = $state('')

  const reviews = $derived(waiting.filter(item => item.kind === 'merge_request').length)
  const parts = $derived(brief ? sections(brief.text) : [])
  const longDate = $derived(
    new Intl.DateTimeFormat(i18n.locale, { weekday: 'long', day: 'numeric', month: 'long' }).format(today),
  )

  async function load() {
    try {
      const [calendar, inbox, work] = await Promise.all([
        listItems({ kinds: ['event'], since: today.getTime() / 1000, until: addDays(today, 1).getTime() / 1000 }),
        listItems({ kinds: INBOX_KINDS, triage: 'inbox', limit: 1000 }),
        listItems({ kinds: ['task', 'ticket'], limit: 1000 }),
      ])
      events = eventsOn(calendar, today)
      waiting = inbox
      inProgress = groupTasks(work).doing.length
    } catch (cause) {
      error = String(cause)
    }
  }

  async function write() {
    const requestId = crypto.randomUUID()
    writing = requestId
    error = ''
    try {
      const reply = await briefing(requestId, i18n.locale, done)
      brief = { date, lang: i18n.locale, text: reply.text }
      await setSetting('briefing', brief)
    } catch (cause) {
      error = String(cause)
    } finally {
      writing = ''
    }
  }

  function tick(item: string, checked: boolean) {
    done = checked ? [...done, item] : done.filter(other => other !== item)
    void setSetting('briefing.done', { date, items: done })
  }

  onMount(() => {
    void load()
    void (async () => {
      const models = await getSetting<Record<string, { provider?: string }>>('models')
      hasModel = Boolean(models?.fast?.provider || models?.strong?.provider)
      const ticked = await getSetting<{ date: string; items: string[] }>('briefing.done')
      if (ticked?.date === date) done = ticked.items
      const saved = await getSetting<Briefing>('briefing')
      if (saved?.date === date) brief = saved
      // One briefing a day, written the first time the day is opened.
      else if (hasModel) void write()
    })()
    const listening = onItemsChanged(load)
    return () => void listening.then(stop => stop())
  })
</script>

<header>
  <h1>{evening ? i18n.t.today.evening : i18n.t.today.hello}</h1>
  <p class="date">{longDate}</p>
</header>

<div class="tiles">
  <button class="tile" onclick={() => onshow('calendar')}>
    <span class="number">{events.length}</span>
    <span>{i18n.t.today.events}</span>
  </button>
  <button class="tile" onclick={() => onshow('inbox')}>
    <span class="number">{waiting.length}</span>
    <span>{i18n.t.today.waiting}</span>
  </button>
  <button class="tile" onclick={() => onshow('tasks')}>
    <span class="number">{inProgress}</span>
    <span>{i18n.t.today.inProgress}</span>
  </button>
  <button class="tile" onclick={() => onshow('inbox')}>
    <span class="number">{reviews}</span>
    <span>{i18n.t.today.reviews}</span>
  </button>
</div>

<div class="columns">
  <section class="card">
    <div class="head">
      <h2>{i18n.t.today.briefing}</h2>
      {#if writing}
        <button class="btn" onclick={() => cancel(writing)}>{i18n.t.models.cancel}</button>
      {:else if hasModel}
        <button class="btn" onclick={write}>{i18n.t.today.refresh}</button>
      {/if}
    </div>

    {#if writing}
      <p class="hint" role="status">{i18n.t.today.writing}</p>
    {/if}
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    {#if !hasModel}
      <p class="hint">{i18n.t.today.noModel}</p>
      <button class="btn" onclick={() => onshow('settings')}>{i18n.t.inbox.openSettings}</button>
    {:else if brief}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="md" onclick={openLinks}>
        {#each parts as part, index (index)}
          {@const items = bullets(part.body)}
          <h3>{part.heading}</h3>
          <!-- The second section is the list of things to handle: each one can be ticked off. -->
          {#if index === 1 && items.length > 0}
            <ul class="checks">
              {#each [...items.filter(i => !done.includes(i)), ...items.filter(i => done.includes(i))] as item (item)}
                <li class:done={done.includes(item)}>
                  <label>
                    <input
                      type="checkbox"
                      checked={done.includes(item)}
                      onchange={event => tick(item, event.currentTarget.checked)}
                    />
                    <span>{@html markdown(item)}</span>
                  </label>
                </li>
              {/each}
            </ul>
            {#if items.every(i => done.includes(i))}
              <p class="hint">{i18n.t.today.allDone}</p>
            {/if}
          {:else}
            {@html markdown(part.body)}
          {/if}
        {:else}
          {@html markdown(brief.text)}
        {/each}
      </div>
    {/if}
  </section>

  <section>
    <h2>{i18n.t.today.agenda}</h2>
    {#if events.length === 0}
      <p class="hint">{i18n.t.calendar.free}</p>
    {/if}
    <ul class="agenda">
      {#each events as event (event.id)}
        <li>
          <span class="when">{formatWhen(event, i18n.locale, i18n.t.calendar.allDay)}</span>
          <span class="title">{event.title || i18n.t.calendar.untitled}</span>
          {#if typeof event.meta.link === 'string'}
            <button class="btn" onclick={() => openPage(event.meta.link as string)}>{i18n.t.calendar.join}</button>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
</div>

<style>
  header {
    margin-bottom: 20px;
  }
  h1 {
    margin: 0;
    font-size: 24px;
    font-weight: 600;
  }
  .date {
    margin: 2px 0 0;
    color: var(--muted);
  }
  .date::first-letter {
    text-transform: uppercase;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 12px;
    max-width: 900px;
    margin-bottom: 24px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--surface);
    color: var(--muted);
    font-size: 13px;
    text-align: left;
  }
  .tile:hover {
    border-color: var(--accent);
  }
  .number {
    color: var(--text);
    font-size: 26px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .columns {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 24px 40px;
  }
  .columns > section {
    flex: 1 1 320px;
    min-width: 0;
    max-width: 620px;
  }
  .card {
    padding: 18px 20px;
    border: 1px solid var(--border);
    border-radius: 16px;
    background: var(--surface);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 4px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 600;
  }
  .head h2 {
    margin: 0;
  }
  h3 {
    margin: 14px 0 4px;
    color: var(--accent);
    font-size: 13px;
    font-weight: 600;
  }
  .md {
    line-height: 1.55;
  }
  .md :global(p) {
    margin: 0 0 6px;
  }
  .md :global(a) {
    color: var(--accent);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .checks label {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 4px 0;
    cursor: pointer;
  }
  .checks :global(p) {
    margin: 0;
  }
  .checks input {
    flex: none;
    width: 15px;
    height: 15px;
    accent-color: var(--accent);
    translate: 0 2px;
  }
  .checks .done {
    color: var(--muted);
    text-decoration: line-through;
  }
  .agenda li {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 9px 0;
    border-bottom: 1px solid var(--border);
  }
  .when {
    flex: none;
    width: 108px;
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .title {
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }
  .hint {
    margin: 6px 0;
    color: var(--muted);
    font-size: 13px;
  }
  .error {
    color: var(--danger);
    font-size: 13px;
  }
</style>
