<script lang="ts">
  import { onMount } from 'svelte'
  import {
    INBOX_KINDS,
    ITEM_ACTIONS,
    type ItemAction,
    actOnItem,
    type Item,
    type Triage,
    listItems,
    listSources,
    getSetting,
    onItemsChanged,
    openPage,
    setTriage,
  } from '../lib/api'
  import { i18n } from '../lib/i18n.svelte'
  import { relativeTime } from '../lib/time'

  let { onsettings }: { onsettings: () => void } = $props()

  let triage = $state<Triage>('inbox')
  let search = $state('')
  let items = $state<Item[]>([])
  let error = $state('')
  let hasSource = $state(true)
  let actionError = $state('')
  let acting = $state<number | null>(null)

  /** What the item's own source says it can do to it. */
  const actionsOf = (item: Item) =>
    ITEM_ACTIONS.filter(action => Array.isArray(item.meta.actions) && item.meta.actions.includes(action))

  async function act(item: Item, action: ItemAction) {
    acting = item.id
    try {
      await actOnItem(item.id, action)
      actionError = ''
    } catch (cause) {
      actionError = String(cause)
    } finally {
      acting = null
    }
  }
  let now = $state(Date.now() / 1000)

  let latest = 0
  async function load() {
    const request = ++latest
    try {
      const found = await listItems({ kinds: INBOX_KINDS, triage, search })
      // A slower, older answer must not replace a newer one.
      if (request !== latest) return
      items = found
      error = ''
      hasSource =
        found.length > 0 || (await getSetting<boolean>('demo')) === true || (await listSources()).length > 0
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

{#if actionError}
  <p class="error" role="alert">{actionError}</p>
{/if}

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
          <div class="title" class:read={item.state === 'read'}>{item.title || i18n.t.inbox.untitled}</div>
          {#if item.body}<div class="body">{item.body}</div>{/if}
        </div>
        {#if item.url}
          <button class="btn" onclick={() => openPage(item.url)}>{i18n.t.open}</button>
        {/if}
        {#each actionsOf(item) as action (action)}
          <button class="btn" disabled={acting === item.id} onclick={() => act(item, action)}>
            {i18n.t.actions[action]}
          </button>
        {/each}
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
  .title.read {
    font-weight: 400;
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
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
