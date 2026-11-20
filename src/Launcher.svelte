<script lang="ts">
  import { onMount } from 'svelte'
  import {
    type Item,
    type Repository,
    createNote,
    createTask,
    listItems,
    listRepositories,
    onLauncherCalled,
    openPage,
    openRepository,
    syncNow,
  } from './lib/api'
  import { i18n } from './lib/i18n.svelte'
  import { SPACE_OF, matches } from './lib/launcher'
  import { notes } from './lib/notes.svelte'
  import { SPACES, type Space } from './lib/spaces'

  let {
    onshow,
    onask,
  }: {
    onshow: (view: Space | 'settings') => void
    /** Opens the assistant, on `question` when there is one. */
    onask: (question: string) => void
  } = $props()

  /** One line of the launcher: what it is, what choosing it does. */
  interface Entry {
    key: string
    label: string
    hint: string
    run: () => unknown
  }

  let dialog = $state<HTMLDialogElement>()
  let query = $state('')
  let active = $state(0)
  let error = $state('')
  let found = $state<Item[]>([])
  let repositories = $state<Repository[]>([])

  const text = $derived(query.trim())

  function openItem(item: Item) {
    if (item.kind === 'note') {
      notes.selected = item.id
      onshow('notes')
    } else if (item.url) openPage(item.url)
    else onshow(SPACE_OF[item.kind])
  }

  const commands = $derived<Entry[]>([
    ...SPACES.map(space => ({
      key: space,
      label: i18n.t.spaces[space],
      hint: i18n.t.launcher.go,
      run: () => onshow(space),
    })),
    { key: 'settings', label: i18n.t.settings, hint: i18n.t.launcher.go, run: () => onshow('settings') },
    { key: 'assistant', label: i18n.t.assistant.title, hint: i18n.t.launcher.go, run: () => onask('') },
    // Not waited for: a sync can take a while, and its failures show in the settings.
    { key: 'sync', label: i18n.t.sources.sync, hint: i18n.t.launcher.action, run: () => void syncNow() },
  ])

  /** What the typed text offers, the most likely first: Enter takes the first line. */
  const entries = $derived<Entry[]>([
    ...commands.filter(command => matches(command.label, text)),
    ...(text
      ? [
          // A sample repository is nowhere on disk: there is nothing to open.
          ...repositories
            .filter(repository => repository.path && matches(repository.name, text))
            .slice(0, 5)
            .map(repository => ({
              key: `repository:${repository.path}`,
              label: repository.name,
              hint: i18n.t.launcher.project,
              run: () => openRepository(repository.path, 'editor'),
            })),
          ...found.map(item => ({
            key: `item:${item.id}`,
            label: item.title || i18n.t.inbox.untitled,
            hint: i18n.t.kinds[item.kind],
            run: () => openItem(item),
          })),
          { key: 'task', label: text, hint: i18n.t.launcher.task, run: () => createTask(text) },
          {
            key: 'note',
            label: text,
            hint: i18n.t.launcher.note,
            run: async () => {
              notes.selected = await createNote(text)
              onshow('notes')
            },
          },
          { key: 'ask', label: text, hint: i18n.t.launcher.ask, run: () => onask(text) },
        ]
      : []),
  ])

  // Searches the items shortly after the last keystroke. Only the latest search is kept.
  let latest = 0
  $effect(() => {
    const search = text
    const request = ++latest
    if (!search) {
      found = []
      return
    }
    const timer = setTimeout(async () => {
      const items = await listItems({ search, limit: 8 }).catch(() => [])
      if (request === latest) found = items
    }, 120)
    return () => clearTimeout(timer)
  })

  $effect(() => {
    document.getElementById(`launcher-${active}`)?.scrollIntoView({ block: 'nearest' })
  })

  export function open() {
    if (!dialog || dialog.open) return
    query = ''
    active = 0
    error = ''
    dialog.showModal()
    void listRepositories()
      .then(list => (repositories = list))
      .catch(() => {})
  }

  /** Does what a line says, then leaves. A failure keeps the launcher open to tell it. */
  async function run(entry: Entry | undefined) {
    if (!entry) return
    try {
      await entry.run()
      dialog?.close()
    } catch (cause) {
      error = String(cause)
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      const step = event.key === 'ArrowDown' ? 1 : -1
      active = (active + step + entries.length) % entries.length
    } else if (event.key === 'Enter' && !event.isComposing) {
      event.preventDefault()
      void run(entries[active])
    }
  }

  function shortcut(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault()
      if (dialog?.open) dialog.close()
      else open()
    }
  }

  onMount(() => {
    const listening = onLauncherCalled(open)
    return () => void listening.then(stop => stop())
  })
</script>

<svelte:window onkeydown={shortcut} />

<!-- A click beside the launcher closes it, as Escape does. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dialog} aria-label={i18n.t.launcher.open} onclick={event => event.target === dialog && dialog.close()}>
  <input
    role="combobox"
    aria-expanded="true"
    aria-controls="launcher-list"
    aria-activedescendant="launcher-{active}"
    aria-label={i18n.t.launcher.open}
    placeholder={i18n.t.launcher.placeholder}
    autocomplete="off"
    spellcheck="false"
    bind:value={query}
    oninput={() => (active = 0)}
    {onkeydown}
  />
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
  <div id="launcher-list" role="listbox" aria-label={i18n.t.launcher.open}>
    {#each entries as entry, index (entry.key)}
      <button
        id="launcher-{index}"
        role="option"
        tabindex="-1"
        aria-selected={index === active}
        onmousemove={() => (active = index)}
        onclick={() => run(entry)}
      >
        <span class="label">{entry.label}</span>
        <span class="hint">{entry.hint}</span>
      </button>
    {/each}
  </div>
</dialog>

<style>
  dialog {
    width: min(620px, 90vw);
    margin-top: 12vh;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.45);
  }
  input {
    box-sizing: border-box;
    width: 100%;
    padding: 16px 18px;
    border: none;
    border-bottom: 1px solid var(--border);
    outline: none;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 16px;
  }
  [role='listbox'] {
    max-height: 52vh;
    padding: 6px;
    overflow: auto;
  }
  [role='option'] {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    width: 100%;
    padding: 9px 12px;
    border-radius: 8px;
    text-align: left;
  }
  [role='option'][aria-selected='true'] {
    background: var(--accent-soft);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    flex: none;
    color: var(--muted);
    font-size: 12px;
  }
  .error {
    margin: 0;
    padding: 10px 18px 0;
    color: var(--danger);
  }
</style>
