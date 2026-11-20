<script lang="ts">
  import { onMount } from 'svelte'
  import {
    type Engine,
    type Item,
    type Repository,
    getSetting,
    launchAgent,
    listAgents,
    listRepositories,
    setSetting,
  } from '../lib/api'
  import { ENGINES, type Links, projectOf, sameRemote } from '../lib/code'
  import { i18n } from '../lib/i18n.svelte'

  /** `path` is the repository the ticket was shown under, when it was shown under one. */
  let { item, path = '', onclose }: { item: Item; path?: string; onclose: () => void } = $props()

  let repositories = $state<Repository[]>([])
  let engines = $state<Engine[]>([])
  // svelte-ignore state_referenced_locally
  let chosen = $state(path)
  let engine = $state<Engine | ''>('')
  let links: Links = {}
  let loaded = $state(false)
  let starting = $state(false)
  let error = $state('')

  const project = $derived(projectOf(item))

  onMount(async () => {
    try {
      // Sample repositories are nowhere on disk: an agent cannot work in them.
      repositories = (await listRepositories()).filter(repository => repository.path)
      engines = await listAgents()
      links = (await getSetting<Links>('code.links')) ?? {}
      const usual = await getSetting<Engine>('code.agent')
      engine = engines.find(installed => installed === usual) ?? engines[0] ?? ''
      // The repository of the ticket itself, else the one last used for its project.
      const known =
        repositories.find(repository => sameRemote(item, repository.remote))?.path ??
        (project ? links[project] : undefined)
      if (!chosen && repositories.some(repository => repository.path === known)) chosen = known ?? ''
    } catch (cause) {
      error = String(cause)
    }
    loaded = true
  })

  async function start() {
    if (!chosen || !engine) return
    starting = true
    error = ''
    try {
      await launchAgent(item.id, chosen, engine)
      // Remembered, so the next ticket of this project needs no choice.
      if (project) await setSetting('code.links', { ...links, [project]: chosen })
      await setSetting('code.agent', engine)
      onclose()
    } catch (cause) {
      error = String(cause)
    } finally {
      starting = false
    }
  }
</script>

<form
  class="launch"
  onsubmit={event => {
    event.preventDefault()
    void start()
  }}
>
  <div class="row">
    <select class="field" aria-label={i18n.t.agent.repository} bind:value={chosen}>
      <option value="" disabled>{i18n.t.agent.choose}</option>
      {#each repositories as repository (repository.path)}
        <option value={repository.path}>{repository.name}</option>
      {/each}
    </select>
    <select class="field" aria-label={i18n.t.agent.on} bind:value={engine}>
      {#each engines as installed (installed)}
        <option value={installed}>{ENGINES[installed]}</option>
      {/each}
    </select>
    <button class="btn primary" type="submit" disabled={!chosen || !engine || starting}>
      {i18n.t.agent.start}
    </button>
    <button class="btn" type="button" onclick={onclose}>{i18n.t.agent.cancel}</button>
  </div>
  {#if loaded && engines.length === 0}
    <p class="hint">{i18n.t.agent.noAgent}</p>
  {:else if loaded && repositories.length === 0}
    <p class="hint">{i18n.t.agent.noRepository}</p>
  {:else}
    <p class="hint">{i18n.t.agent.hint}</p>
  {/if}
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
</form>

<style>
  .launch {
    flex-basis: 100%;
    margin: 8px 0 4px;
    padding: 12px 16px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }
  select {
    min-width: 160px;
  }
  p {
    margin: 8px 0 0;
    font-size: 12px;
  }
  .hint {
    color: var(--muted);
  }
  .error {
    color: var(--danger);
  }
</style>
