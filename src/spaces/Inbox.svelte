<script lang="ts">
  import { onMount } from 'svelte'
  import { type Item, type Triage, getSetting, listItems, onItemsChanged, setTriage } from '../lib/api'
  import { i18n } from '../lib/i18n.svelte'
  import { relativeTime } from '../lib/time'

  let { onsettings }: { onsettings: () => void } = $props()

  let triage = $state<Triage>('inbox')
  let search = $state('')
  let items = $state<Item[]>([])
  let error = $state('')
  let hasSource = $state(true)
  let now = $state(Date.now() / 1000)

  let latest = 0
  async function load() {
    const request = ++latest
    try {
      const found = await listItems({ triage, search })
      // A slower, older answer must not replace a newer one.
      if (request !== latest) return
      items = found
      error = ''
      // The sample data is the only source for now.
      hasSource = found.length > 0 || (await getSetting<boolean>('demo')) === true
    } catch (cause) {
      if (request === latest) error = String(cause)
    }
  }

  // Runs again whenever the filter or the search changes.
  $effect(() => {
    void load()
  })

  onMount(() => {
    const listening = onItemsChanged(load)
    const clock = setInterval(() => (now = Date.now() / 1000), 60_000)
    return () => {
      void listening.then(stop => stop())
      clearInterval(clock)
    }
  })
</script>

<header>
  <h1>{i18n.t.spaces.inbox}</h1>
  <input
    class="field"
    type="search"
    placeholder={i18n.t.inbox.search}
    aria-label={i18n.t.inbox.search}
    bind:value={search}
  />
</header>

<div class="filters" role="group">
  {#each ['inbox', 'done'] as const as value (value)}
    <button class="chip" aria-pressed={triage === value} onclick={() => (triage = value)}>
      {value === 'inbox' ? i18n.t.inbox.toHandle : i18n.t.inbox.done}
    </button>
  {/each}
</div>

{#if error}
  <p class="error" role="alert">{error}</p>
{:else if items.length === 0}
  <div class="empty">
    {#if search}
      <p>{i18n.t.inbox.noMatch}</p>
    {:else if !hasSource}
      <p>{i18n.t.inbox.noSource}</p>
      <button class="btn" onclick={onsettings}>{i18n.t.inbox.openSettings}</button>
    {:else}
      <p>{i18n.t.inbox.empty}</p>
    {/if}
  </div>
{:else}
  <ul>
    {#each items as item (item.id)}
      <li>
        <div class="text">
          <div class="top">
            <span class="kind">{i18n.t.kinds[item.kind]}</span>
            <span class="meta">
              {[item.author, relativeTime(item.occurredAt, now, i18n.locale)].filter(Boolean).join(' · ')}
            </span>
          </div>
          <div class="title">{item.title}</div>
          {#if item.body}<div class="body">{item.body}</div>{/if}
        </div>
        <button class="btn" onclick={() => setTriage(item.id, triage === 'inbox' ? 'done' : 'inbox')}>
          {triage === 'inbox' ? i18n.t.inbox.markDone : i18n.t.inbox.restore}
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  header {
    display: flex;
    align-items: center;
    gap: 24px;
    margin-bottom: 16px;
  }
  h1 {
    margin: 0;
    font-size: 24px;
    font-weight: 600;
  }
  header .field {
    flex: 1;
    max-width: 420px;
    margin-left: auto;
  }
  .filters {
    display: flex;
    gap: 6px;
    margin-bottom: 12px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 8px;
    border-bottom: 1px solid var(--border);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 2px;
  }
  .kind {
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11px;
    font-weight: 600;
  }
  .meta {
    color: var(--muted);
    font-size: 12px;
  }
  .title {
    font-weight: 600;
  }
  .body {
    overflow: hidden;
    color: var(--muted);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    padding: 24px 8px;
    color: var(--muted);
  }
  .empty p {
    margin: 0;
  }
  .error {
    color: var(--danger);
  }
</style>
