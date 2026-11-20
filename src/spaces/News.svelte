<script lang="ts">
  import { onMount } from 'svelte'
  import { type Item, type Triage, listItems, onItemsChanged, openPage, setTriage } from '../lib/api'
  import { i18n } from '../lib/i18n.svelte'
  import { relativeTime } from '../lib/time'

  let { onsettings }: { onsettings: () => void } = $props()

  /** Where an article stands: to read, kept for later, read. */
  const SHELVES = { inbox: 'unread', later: 'later', done: 'read' } as const
  const TRIAGES = Object.keys(SHELVES) as Triage[]

  let triage = $state<Triage>('inbox')
  let items = $state<Item[]>([])
  /** Whether any article is known at all: none means no feed is followed yet. */
  let any = $state(true)
  let error = $state('')
  const now = Date.now() / 1000

  let latest = 0
  async function load() {
    const request = ++latest
    try {
      const found = await listItems({ kinds: ['article'], triage })
      // A slower, older answer must not replace a newer one.
      if (request !== latest) return
      items = found
      any = found.length > 0 || (await listItems({ kinds: ['article'], limit: 1 })).length > 0
      error = ''
    } catch (cause) {
      if (request === latest) error = String(cause)
    }
  }

  const move = (item: Item, to: Triage) => setTriage(item.id, to).catch(cause => (error = String(cause)))

  // Runs again whenever the shelf changes.
  $effect(() => {
    void triage
    void load()
  })

  onMount(() => {
    const listening = onItemsChanged(load)
    return () => void listening.then(stop => stop())
  })
</script>

<header>
  <h1>{i18n.t.spaces.news}</h1>
  <div class="shelves" role="group" aria-label={i18n.t.spaces.news}>
    {#each TRIAGES as shelf (shelf)}
      <button class="chip" aria-pressed={triage === shelf} onclick={() => (triage = shelf)}>
        {i18n.t.news[SHELVES[shelf]]}
      </button>
    {/each}
  </div>
</header>

{#if error}
  <p class="error" role="alert">{error}</p>
{:else if !any}
  <p class="hint">{i18n.t.news.noFeed}</p>
  <button class="btn" onclick={onsettings}>{i18n.t.inbox.openSettings}</button>
{:else if items.length === 0}
  <p class="hint">{i18n.t.news.empty}</p>
{/if}

<ul>
  {#each items as item (item.id)}
    <li>
      <div class="text">
        <div class="meta">
          {[item.author, item.occurredAt ? relativeTime(item.occurredAt, now, i18n.locale) : '']
            .filter(Boolean)
            .join(' · ')}
        </div>
        <div class="title">{item.title || i18n.t.inbox.untitled}</div>
        {#if item.body}<div class="body">{item.body}</div>{/if}
      </div>
      {#if item.url}
        <button class="btn" onclick={() => openPage(item.url)}>{i18n.t.open}</button>
      {/if}
      {#if triage === 'inbox'}
        <button class="btn" onclick={() => move(item, 'later')}>{i18n.t.news.later}</button>
      {/if}
      {#if triage === 'done'}
        <button class="btn" onclick={() => move(item, 'inbox')}>{i18n.t.news.restore}</button>
      {:else}
        <button class="btn" onclick={() => move(item, 'done')}>{i18n.t.news.markRead}</button>
      {/if}
    </li>
  {/each}
</ul>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 20px;
    margin-bottom: 16px;
  }
  h1 {
    margin: 0;
    font-size: 24px;
    font-weight: 600;
  }
  .shelves {
    display: flex;
    gap: 6px;
  }
  ul {
    max-width: 900px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 8px;
    border-bottom: 1px solid var(--border);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .meta,
  .body,
  .hint {
    color: var(--muted);
    font-size: 12px;
  }
  .title {
    font-weight: 600;
  }
  .body {
    margin-top: 2px;
    font-size: 13px;
  }
  .error {
    color: var(--danger);
  }
</style>
