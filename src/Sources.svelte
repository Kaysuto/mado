<script lang="ts">
  import { onMount } from 'svelte'
  import { type SourceKind, type SourceView, deleteSource, listSources, saveSource, syncNow } from './lib/api'
  import { i18n } from './lib/i18n.svelte'

  const KINDS: SourceKind[] = ['github', 'linear']
  const NAMES: Record<SourceKind, string> = { github: 'GitHub', linear: 'Linear' }

  let sources = $state<SourceView[]>([])
  let kind = $state<SourceKind>('github')
  let name = $state(NAMES.github)
  let token = $state('')
  let busy = $state(false)
  let error = $state('')
  let armed = $state('') // id of the source whose removal waits for a second click

  $effect(() => {
    name = NAMES[kind]
    token = ''
  })

  async function guard(action: () => Promise<void>) {
    busy = true
    try {
      await action()
      error = ''
    } catch (cause) {
      error = String(cause)
    } finally {
      busy = false
    }
  }

  /** Syncs, then reads the sources again so each shows whether it worked. */
  const sync = () =>
    guard(async () => {
      await syncNow()
      sources = await listSources()
    })

  onMount(() => guard(async () => void (sources = await listSources())))

  const add = () =>
    guard(async () => {
      await saveSource({ id: crypto.randomUUID(), kind, name: name.trim() }, token)
      token = ''
      await syncNow()
      sources = await listSources()
    })

  function remove(id: string) {
    if (armed !== id) {
      armed = id
      setTimeout(() => armed === id && (armed = ''), 4000)
      return
    }
    armed = ''
    void guard(async () => {
      await deleteSource(id)
      await syncNow()
      sources = await listSources()
    })
  }
</script>

<h2>{i18n.t.sources.title}</h2>
<p class="hint">{i18n.t.sources.intro}</p>

{#if sources.length === 0}
  <p class="hint">{i18n.t.sources.none}</p>
{/if}
<ul>
  {#each sources as source (source.id)}
    <li>
      <span class="name">{source.name}</span>
      <span class="hint">{NAMES[source.kind]}</span>
      {#if source.error}
        <span class="error" role="alert">{source.error}</span>
      {/if}
      <button class="btn" onclick={() => remove(source.id)}>
        {armed === source.id ? i18n.t.sources.confirmRemove : i18n.t.sources.remove}
      </button>
    </li>
  {/each}
</ul>

<form
  onsubmit={event => {
    event.preventDefault()
    void add()
  }}
>
  <select class="field" aria-label={i18n.t.sources.service} bind:value={kind}>
    {#each KINDS as option (option)}
      <option value={option}>{NAMES[option]}</option>
    {/each}
  </select>
  <input class="field" placeholder={i18n.t.sources.name} aria-label={i18n.t.sources.name} required bind:value={name} />
  <input
    class="field wide"
    type="password"
    autocomplete="off"
    placeholder={i18n.t.sources.token[kind]}
    aria-label={i18n.t.sources.token[kind]}
    required={kind === 'linear'}
    bind:value={token}
  />
  <button class="btn" type="submit" disabled={busy}>{i18n.t.sources.add}</button>
  <button class="btn" type="button" disabled={busy} onclick={sync}>{i18n.t.sources.sync}</button>
</form>
<p class="hint">{i18n.t.sources.hints[kind]}</p>

{#if error}
  <p class="error" role="alert">{error}</p>
{/if}

<style>
  h2 {
    margin: 32px 0 4px;
    font-size: 18px;
    font-weight: 600;
  }
  p {
    max-width: 760px;
    margin: 6px 0;
  }
  .hint {
    color: var(--muted);
    font-size: 13px;
  }
  .name {
    font-weight: 600;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li,
  form {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    max-width: 760px;
  }
  li {
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
  }
  li .btn {
    margin-left: auto;
  }
  form {
    margin-top: 12px;
  }
  .wide {
    flex: 1 1 260px;
  }
  .error {
    color: var(--danger);
    font-size: 13px;
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
