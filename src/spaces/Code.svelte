<script lang="ts">
  import { onMount } from 'svelte'
  import {
    type Item,
    type Repository,
    getSetting,
    listItems,
    listRepositories,
    onItemsChanged,
    openPage,
    openRepository,
    pickFolder,
    setSetting,
  } from '../lib/api'
  import { checks, linked } from '../lib/code'
  import { i18n } from '../lib/i18n.svelte'
  import { relativeTime } from '../lib/time'

  let repositories = $state<Repository[] | null>(null)
  let items = $state<Item[]>([])
  let folders = $state<string[]>([])
  let editor = $state('')
  let terminal = $state('')
  let managing = $state(false)
  let error = $state('')
  const now = Date.now() / 1000

  async function guard(action: () => Promise<void>) {
    try {
      await action()
      error = ''
    } catch (cause) {
      error = String(cause)
    }
  }

  const load = () => guard(async () => void (repositories = await listRepositories()))
  const loadItems = () =>
    guard(async () => void (items = await listItems({ kinds: ['merge_request', 'ticket', 'alert'] })))

  /** Keeps the chosen folders, then looks into them again. */
  const keep = (next: string[]) =>
    guard(async () => {
      folders = next
      await setSetting('code.folders', next)
      await load()
    })

  const add = () =>
    guard(async () => {
      const folder = await pickFolder()
      if (folder && !folders.includes(folder)) await keep([...folders, folder])
    })

  onMount(() => {
    void guard(async () => {
      folders = (await getSetting<string[]>('code.folders')) ?? []
      editor = (await getSetting<string>('code.editor')) ?? ''
      terminal = (await getSetting<string>('code.terminal')) ?? ''
    })
    void load()
    void loadItems()
    const listening = onItemsChanged(loadItems)
    return () => void listening.then(stop => stop())
  })
</script>

<header>
  <h1>{i18n.t.spaces.code}</h1>
  <button class="chip" aria-pressed={managing} onclick={() => (managing = !managing)}>
    {i18n.t.code.folders}
  </button>
  <button class="btn" onclick={load}>{i18n.t.code.refresh}</button>
</header>

{#if managing}
  <section>
    <ul>
      {#each folders as folder (folder)}
        <li class="row">
          <span class="path">{folder}</span>
          <button class="btn" onclick={() => keep(folders.filter(kept => kept !== folder))}>
            {i18n.t.code.remove}
          </button>
        </li>
      {/each}
    </ul>
    <div class="row">
      <button class="btn primary" onclick={add}>{i18n.t.code.addFolder}</button>
      <label>
        {i18n.t.code.editor}
        <input
          class="field"
          placeholder="code ."
          bind:value={editor}
          onchange={() => guard(() => setSetting('code.editor', editor))}
        />
      </label>
      <label>
        {i18n.t.code.terminal}
        <input
          class="field"
          bind:value={terminal}
          onchange={() => guard(() => setSetting('code.terminal', terminal))}
        />
      </label>
    </div>
    <p class="hint">{i18n.t.code.toolsHint}</p>
  </section>
{/if}

{#if error}
  <p class="error" role="alert">{error}</p>
{/if}

{#if repositories?.length === 0}
  <p class="hint">{folders.length === 0 ? i18n.t.code.empty : i18n.t.code.none}</p>
  {#if folders.length === 0}
    <button class="btn primary" onclick={add}>{i18n.t.code.addFolder}</button>
  {/if}
{/if}

<ul>
  {#each repositories ?? [] as repository (repository.path + repository.name)}
    <li>
      <div class="row">
        <span class="name">{repository.name}</span>
        {#if repository.group}<span class="hint">{repository.group}</span>{/if}
        {#if repository.branch}<span class="ref">{repository.branch}</span>{/if}
        {#if repository.changes}
          <span class="changes">{repository.changes} {i18n.t.code.changes}</span>
        {/if}
        {#if repository.ahead}<span class="hint">↑ {repository.ahead} {i18n.t.code.ahead}</span>{/if}
        {#if repository.behind}<span class="hint">↓ {repository.behind} {i18n.t.code.behind}</span>{/if}
        <span class="space"></span>
        {#if repository.lastCommit}
          <span class="hint">{relativeTime(repository.lastCommit, now, i18n.locale)}</span>
        {/if}
        {#if repository.path}
          <button class="btn" onclick={() => guard(() => openRepository(repository.path, 'editor'))}>
            {i18n.t.code.editor}
          </button>
          <button class="btn" onclick={() => guard(() => openRepository(repository.path, 'terminal'))}>
            {i18n.t.code.terminal}
          </button>
        {/if}
      </div>
      {#each linked(items, repository.remote) as item (item.id)}
        {@const ci = checks(item)}
        <div class="row linked">
          <span class="ref">{item.meta.identifier ?? i18n.t.kinds[item.kind]}</span>
          <span class="title">{item.title}</span>
          {#if item.state}<span class="hint">{item.state}</span>{/if}
          {#if ci}<span class="ci {ci}">{i18n.t.code.checks[ci]}</span>{/if}
          {#if item.url}
            <button class="btn" onclick={() => openPage(item.url)}>{i18n.t.open}</button>
          {/if}
        </div>
      {/each}
    </li>
  {/each}
</ul>

<style>
  header,
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
  }
  header {
    margin-bottom: 16px;
  }
  h1 {
    flex: 1;
    margin: 0;
    font-size: 24px;
    font-weight: 600;
  }
  section {
    margin-bottom: 16px;
    padding: 4px 16px 12px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
  }
  section .row {
    padding: 8px 0;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  ul > li {
    padding: 10px 8px;
    border-bottom: 1px solid var(--border);
  }
  .path,
  .title,
  .space {
    flex: 1;
    min-width: 0;
  }
  .path {
    overflow-wrap: anywhere;
  }
  .name {
    font-weight: 600;
  }
  .linked {
    margin-top: 8px;
    padding-left: 16px;
  }
  label {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: 12px;
  }
  .ref {
    flex: none;
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11px;
    font-weight: 600;
  }
  .hint,
  .changes,
  .ci {
    color: var(--muted);
    font-size: 12px;
  }
  .changes,
  .ci.running {
    color: var(--accent);
  }
  .ci.passed {
    color: var(--ok);
  }
  .ci.failed,
  .error {
    color: var(--danger);
  }
</style>
