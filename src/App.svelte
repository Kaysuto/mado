<script lang="ts">
  import { i18n } from './lib/i18n.svelte'
  import { LOCALES } from './lib/locale'
  import { SPACES, type Space } from './lib/spaces'

  let current = $state<Space>('today')

  function onkeydown(event: KeyboardEvent) {
    const space = SPACES[Number(event.key) - 1]
    if (event.altKey && space) {
      event.preventDefault()
      current = space
    }
  }

  $effect(() => {
    document.documentElement.lang = i18n.locale
  })
</script>

<svelte:window {onkeydown} />

<div class="shell">
  <nav class="sidebar" aria-label="Mado">
    <div class="brand">
      <span class="logo" aria-hidden="true">窓</span>
      <span>Mado</span>
    </div>
    <ul>
      {#each SPACES as space, index (space)}
        <li>
          <button
            class="nav"
            aria-current={current === space ? 'page' : undefined}
            onclick={() => (current = space)}
          >
            <span>{i18n.t.spaces[space]}</span>
            <kbd>Alt+{index + 1}</kbd>
          </button>
        </li>
      {/each}
    </ul>
    <label class="lang">
      <span>{i18n.t.language}</span>
      <select bind:value={i18n.locale}>
        {#each LOCALES as locale (locale)}
          <option value={locale}>{locale.toUpperCase()}</option>
        {/each}
      </select>
    </label>
  </nav>

  <main>
    <h1>{i18n.t.spaces[current]}</h1>
    <p class="muted">{i18n.t.comingSoon}</p>
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
  .lang {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 8px;
    font-size: 12px;
    color: var(--muted);
  }
  select {
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
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
