<script lang="ts">
  import { onMount } from 'svelte'
  import {
    type Period,
    type ReviewNumbers,
    cancel,
    getSetting,
    onItemsChanged,
    openLinks,
    review,
    reviewNumbers,
    setSetting,
  } from '../lib/api'
  import { byProject, duration } from '../lib/focus'
  import { focus } from '../lib/focus.svelte'
  import { i18n } from '../lib/i18n.svelte'
  import { markdown } from '../lib/markdown'

  let { onsettings }: { onsettings: () => void } = $props()

  /** A review the model wrote, kept until the next one of the same period. */
  interface Written {
    at: number
    text: string
  }

  const PERIODS: Period[] = ['day', 'week']

  let period = $state<Period>('day')
  let numbers = $state<ReviewNumbers | null>(null)
  let written = $state<Record<Period, Written | null>>({ day: null, week: null })
  let hasModel = $state(true)
  let writing = $state('') // id of the review being written, to cancel it
  let error = $state('')

  const days = $derived(numbers?.focus.days ?? [])
  const total = $derived(days.reduce((sum, day) => sum + day.seconds, 0))
  const longest = $derived(Math.max(1, ...days.map(day => day.seconds)))
  const projects = $derived(byProject(numbers?.focus.items ?? []))
  const current = $derived(written[period])
  const committed = $derived(numbers?.commits ?? [])
  const weekday = $derived(new Intl.DateTimeFormat(i18n.locale, { weekday: 'short' }))
  const moment = $derived(
    new Intl.DateTimeFormat(i18n.locale, { day: 'numeric', month: 'long', hour: '2-digit', minute: '2-digit' }),
  )

  async function load() {
    const asked = period
    try {
      const counted = await reviewNumbers(asked)
      if (asked === period) numbers = counted
    } catch (cause) {
      error = String(cause)
    }
  }

  function pick(next: Period) {
    period = next
    void setSetting('review.period', next)
  }

  async function write() {
    const requestId = crypto.randomUUID()
    const asked = period
    writing = requestId
    error = ''
    try {
      const reply = await review(requestId, i18n.locale, asked)
      written[asked] = { at: Date.now(), text: reply.text }
      await setSetting(`review.${asked}`, written[asked])
    } catch (cause) {
      error = String(cause)
    } finally {
      writing = ''
    }
  }

  // The figures follow the period shown, and every focus session that ends.
  $effect(() => {
    void focus.today
    void load()
  })

  onMount(() => {
    void (async () => {
      const models = await getSetting<Record<string, { provider?: string }>>('models')
      hasModel = Boolean(models?.fast?.provider || models?.strong?.provider)
      written = {
        day: await getSetting<Written>('review.day'),
        week: await getSetting<Written>('review.week'),
      }
      if ((await getSetting<Period>('review.period')) === 'week') period = 'week'
    })()
    const listening = onItemsChanged(load)
    return () => void listening.then(stop => stop())
  })
</script>

<header>
  <h1>{i18n.t.spaces.review}</h1>
  <div class="periods" role="group" aria-label={i18n.t.spaces.review}>
    {#each PERIODS as option (option)}
      <button class="chip" aria-pressed={period === option} onclick={() => pick(option)}>
        {i18n.t.review[option]}
      </button>
    {/each}
  </div>
</header>

<dl class="tiles">
  <div class="tile">
    <dt>{i18n.t.review.focus}</dt>
    <dd>{duration(total)}</dd>
  </div>
  <div class="tile">
    <dt>{i18n.t.review.worked}</dt>
    <dd>{numbers?.focus.items.length ?? 0}</dd>
  </div>
  <div class="tile">
    <dt>{i18n.t.review.ticked}</dt>
    <dd>{numbers?.done.length ?? 0}</dd>
  </div>
  <div class="tile">
    <dt>{i18n.t.review.commits}</dt>
    <dd>{committed.reduce((sum, work) => sum + work.commits.length, 0)}</dd>
  </div>
</dl>

<div class="columns">
  <div class="figures">
    {#if period === 'week'}
      <section>
        <h2>{i18n.t.review.perDay}</h2>
        <ol class="bars">
          {#each days as day (day.start)}
            <li>
              <span class="amount">{day.seconds > 0 ? duration(day.seconds) : ''}</span>
              <span class="track"><span class="bar" style:height="{(day.seconds / longest) * 100}%"></span></span>
              <span class="day">{weekday.format(day.start * 1000)}</span>
            </li>
          {/each}
        </ol>
      </section>
    {/if}

    <section>
      <h2>{i18n.t.review.byProject}</h2>
      {#if projects.length === 0}
        <p class="hint">{i18n.t.review.noFocus}</p>
      {/if}
      {#each projects as group (group.project)}
        <div class="project">
          <span>{group.project ?? i18n.t.review.noProject}</span>
          <span class="time">{duration(group.seconds)}</span>
        </div>
        <ul>
          {#each group.items as time (`${time.itemId} ${time.title}`)}
            <li>
              <span class="title">{time.title || i18n.t.focus.free}</span>
              <span class="time">{duration(time.seconds)}</span>
            </li>
          {/each}
        </ul>
      {/each}
    </section>

    {#if committed.length > 0}
      <section>
        <h2>{i18n.t.review.commits}</h2>
        {#each committed as work (work.repository)}
          <div class="project">
            <span>{work.repository}</span>
            <span class="time">{work.commits.length}</span>
          </div>
          <ul>
            {#each work.commits.slice(0, 8) as subject, index (index)}
              <li><span class="title">{subject}</span></li>
            {/each}
          </ul>
        {/each}
      </section>
    {/if}
  </div>

  <section class="card">
    <div class="head">
      <h2>{period === 'day' ? i18n.t.review.ofDay : i18n.t.review.ofWeek}</h2>
      {#if writing}
        <button class="btn" onclick={() => cancel(writing)}>{i18n.t.models.cancel}</button>
      {:else if hasModel}
        <button class="btn" onclick={write}>{current ? i18n.t.today.refresh : i18n.t.review.write}</button>
      {/if}
    </div>

    {#if writing}
      <p class="hint" role="status">{i18n.t.review.writing}</p>
    {/if}
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    {#if !hasModel}
      <p class="hint">{i18n.t.review.noModel}</p>
      <button class="btn" onclick={onsettings}>{i18n.t.inbox.openSettings}</button>
    {:else if current}
      <p class="hint">{i18n.t.review.writtenOn} {moment.format(current.at)}</p>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="md" onclick={openLinks}>{@html markdown(current.text)}</div>
    {:else if !writing}
      <p class="hint">{i18n.t.review.empty}</p>
    {/if}
  </section>
</div>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 20px;
    margin-bottom: 20px;
  }
  h1 {
    margin: 0;
    font-size: 24px;
    font-weight: 600;
  }
  .periods {
    display: flex;
    gap: 6px;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 12px;
    max-width: 900px;
    margin: 0 0 24px;
  }
  .tile {
    display: flex;
    flex-direction: column-reverse;
    gap: 2px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--surface);
  }
  dt {
    color: var(--muted);
    font-size: 13px;
  }
  dd {
    margin: 0;
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
  .columns > * {
    flex: 1 1 320px;
    min-width: 0;
    max-width: 620px;
  }
  .figures {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 600;
  }
  ul,
  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .bars {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 8px;
    height: 140px;
  }
  .bars li {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    min-width: 0;
    color: var(--muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .amount {
    height: 16px;
    white-space: nowrap;
  }
  .track {
    flex: 1;
    display: flex;
    align-items: flex-end;
    width: 100%;
    border-bottom: 1px solid var(--border);
  }
  .bar {
    width: 100%;
    border-radius: 6px 6px 0 0;
    background: var(--accent);
  }
  .project,
  ul li {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 14px;
    padding: 7px 0;
  }
  .project {
    margin-top: 6px;
    color: var(--accent);
    font-size: 13px;
    font-weight: 600;
  }
  ul li {
    border-bottom: 1px solid var(--border);
  }
  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .time {
    flex: none;
    font-variant-numeric: tabular-nums;
  }
  ul .time {
    color: var(--muted);
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
  .head h2 {
    margin: 0;
  }
  .md {
    line-height: 1.55;
  }
  .md :global(h4) {
    margin: 14px 0 4px;
    color: var(--accent);
    font-size: 13px;
    font-weight: 600;
  }
  .md :global(p) {
    margin: 0 0 6px;
  }
  .md :global(ul) {
    margin: 0 0 6px;
    padding-left: 18px;
  }
  .md :global(a) {
    color: var(--accent);
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
