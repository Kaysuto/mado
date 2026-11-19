<script lang="ts">
  import { onMount } from 'svelte'
  import Settings from './Settings.svelte'
  import { getSetting, setSetting } from './lib/api'
  import { i18n, restoreLocale } from './lib/i18n.svelte'
  import { SPACES, type Space } from './lib/spaces'
  import Calendar from './spaces/Calendar.svelte'
  import Inbox from './spaces/Inbox.svelte'
  import Tasks from './spaces/Tasks.svelte'

  type View = Space | 'settings'
  const VIEWS: readonly View[] = [...SPACES, 'settings']

  let view = $state<View>('today')

  /** Opens a view and remembers it, so the app reopens where it was left. */
  function show(next: View) {
    view = next
    void setSetting('view', next)
  }

  function onkeydown(event: KeyboardEvent) {
    const space = SPACES[Number(event.key) - 1]
    if (event.altKey && space) {
      event.preventDefault()
      show(space)
    }
  }

  onMount(async () => {
    await restoreLocale()
    const saved = await getSetting<string>('view')
    view = VIEWS.find(v => v === saved) ?? view
  })

  $effect(() => {
    document.documentElement.lang = i18n.locale
  })
</script>

<svelte:window {onkeydown} />

<div class="shell">
  <nav class="sidebar" aria-label="Madow">
    <div class="brand">
      <span class="logo" aria-hidden="true">窓</span>
      <span>Madow</span>
    </div>
    <ul>
      {#each SPACES as space, index (space)}
        <li>
          <button
            class="nav"
            aria-current={view === space ? 'page' : undefined}
            onclick={() => show(space)}
          >
            <span>{i18n.t.spaces[space]}</span>
            <kbd>Alt+{index + 1}</kbd>
          </button>
        </li>
      {/each}
    </ul>
    <button
      class="nav"
      aria-current={view === 'settings' ? 'page' : undefined}
      onclick={() => show('settings')}
    >
      {i18n.t.settings}
    </button>
  </nav>

  <main>
    {#if view === 'inbox'}
      <Inbox onsettings={() => show('settings')} />
    {:else if view === 'calendar'}
      <Calendar />
    {:else if view === 'tasks'}
      <Tasks />
    {:else if view === 'settings'}
      <Settings />
    {:else}
      <h1>{i18n.t.spaces[view]}</h1>
      <p class="muted">{i18n.t.comingSoon}</p>
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 220px 1fr;
    height: 100%;
  }
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px 12px;
    border-right: 1px solid var(--border);
    background: var(--surface);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px;
    font-size: 16px;
    font-weight: 600;
  }
  .logo {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 15px;
  }
  ul {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 8px 10px;
    border-radius: 8px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 500;
    text-align: left;
  }
  .nav:hover {
    background: var(--hover);
    color: var(--text);
  }
  .nav[aria-current='page'] {
    background: var(--accent-soft);
    color: var(--accent);
  }
  kbd {
    font: inherit;
    font-size: 10px;
    opacity: 0.6;
  }
  main {
    padding: 32px 40px;
    overflow: auto;
  }
  h1 {
    margin: 0 0 8px;
    font-size: 24px;
    font-weight: 600;
  }
  .muted {
    margin: 0;
    color: var(--muted);
  }
</style>
