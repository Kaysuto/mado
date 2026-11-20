<script lang="ts">
  import { onMount, tick, untrack } from 'svelte'
  import {
    type Item,
    type NoteSubject,
    createNote,
    deleteLocal,
    listItems,
    onItemsChanged,
    openLinks,
    openPage,
    saveNote,
  } from '../lib/api'
  import { i18n } from '../lib/i18n.svelte'
  import { markdown } from '../lib/markdown'
  import { notes } from '../lib/notes.svelte'
  import { relativeTime } from '../lib/time'

  let list = $state<Item[]>([])
  let search = $state('')
  let error = $state('')
  /** The note being written. Its text lives here while it is open: the list only follows. */
  let draft = $state<{ id: number; title: string; body: string } | null>(null)
  let about = $state<NoteSubject | null>(null)
  let preview = $state(false)
  let dirty = $state(false) // written since the last save
  let confirming = $state(false) // deleting takes a second click
  let titleField = $state<HTMLInputElement>()
  let pending: ReturnType<typeof setTimeout> | undefined
  let ready = false // the first note has been opened
  const now = Date.now() / 1000

  const subjectOf = (note: Item) => (note.meta.about ?? null) as NoteSubject | null

  async function guard(action: () => Promise<void>) {
    try {
      await action()
      error = ''
    } catch (cause) {
      error = String(cause)
    }
  }

  const load = () => guard(async () => void (list = await listItems({ kinds: ['note'], search, limit: 500 })))

  /** Writes the open note to the database, if it changed since the last time. */
  async function flush() {
    clearTimeout(pending)
    if (!draft || !dirty) return
    dirty = false
    try {
      await saveNote(draft.id, draft.title, draft.body)
      error = ''
    } catch (cause) {
      dirty = true
      error = String(cause)
    }
  }

  // Saved shortly after the last keystroke, not at each one.
  function edited() {
    dirty = true
    confirming = false
    clearTimeout(pending)
    pending = setTimeout(flush, 600)
  }

  async function open(id: number | null) {
    await flush()
    const note = list.find(note => note.id === id)
    notes.selected = note?.id ?? null
    draft = note ? { id: note.id, title: note.title, body: note.body } : null
    about = note ? subjectOf(note) : null
    preview = false
    confirming = false
  }

  const add = () =>
    guard(async () => {
      search = ''
      const id = await createNote('')
      await load()
      await open(id)
      await tick()
      titleField?.focus()
    })

  const remove = () =>
    guard(async () => {
      if (!draft) return
      if (!confirming) {
        confirming = true
        return
      }
      clearTimeout(pending)
      const { id } = draft
      dirty = false
      draft = null
      notes.selected = null
      await deleteLocal(id)
    })

  // Follows a note chosen from the launcher while the space is shown.
  $effect(() => {
    const id = notes.selected
    if (ready && id !== null && id !== untrack(() => draft?.id)) void load().then(() => open(id))
  })

  onMount(() => {
    // Reopens on the note left open, else on the one written last.
    void load()
      .then(() => open(notes.selected ?? list[0]?.id ?? null))
      .then(() => (ready = true))
    const listening = onItemsChanged(load)
    return () => {
      void flush()
      void listening.then(stop => stop())
    }
  })
</script>

<h1>{i18n.t.spaces.notes}</h1>

{#if error}
  <p class="error" role="alert">{error}</p>
{/if}

<div class="notes">
  <aside>
    <div class="bar">
      <input
        class="field"
        type="search"
        placeholder={i18n.t.notes.search}
        aria-label={i18n.t.notes.search}
        bind:value={search}
        oninput={load}
      />
      <button class="btn primary" onclick={add}>{i18n.t.notes.add}</button>
    </div>
    {#if list.length === 0}
      <p class="hint">{search ? i18n.t.notes.none : i18n.t.notes.empty}</p>
    {/if}
    <ul>
      {#each list as note (note.id)}
        {@const subject = subjectOf(note)}
        {@const current = draft?.id === note.id}
        <li>
          <button class="note" aria-current={current ? 'true' : undefined} onclick={() => open(note.id)}>
            <span class="title">{(current ? draft?.title : note.title) || i18n.t.notes.untitled}</span>
            <span class="hint">
              {subject ? `${i18n.t.kinds[subject.kind]} · ` : ''}{relativeTime(note.occurredAt, now, i18n.locale)}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  </aside>

  <section>
    {#if draft}
      <input
        class="field heading"
        placeholder={i18n.t.notes.title}
        aria-label={i18n.t.notes.title}
        bind:this={titleField}
        bind:value={draft.title}
        oninput={edited}
      />
      {#if about}
        <p class="about">
          <span>{i18n.t.notes.about} <strong>{i18n.t.kinds[about.kind]}</strong> · {about.title}</span>
          {#if about.url}
            <button class="btn" onclick={() => openPage(about?.url ?? null)}>{i18n.t.open}</button>
          {/if}
        </p>
      {/if}
      <div class="tools">
        <button class="chip" aria-pressed={!preview} onclick={() => (preview = false)}>{i18n.t.notes.write}</button>
        <button class="chip" aria-pressed={preview} onclick={() => (preview = true)}>{i18n.t.notes.preview}</button>
        <span class="hint state">{dirty ? i18n.t.notes.saving : i18n.t.notes.saved}</span>
        <button class="btn" class:danger={confirming} onclick={remove}>
          {confirming ? i18n.t.notes.confirmDelete : i18n.t.notes.delete}
        </button>
      </div>
      {#if preview}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="md" onclick={openLinks}>{@html markdown(draft.body)}</div>
      {:else}
        <textarea
          class="field"
          placeholder={i18n.t.notes.body}
          aria-label={i18n.t.notes.body}
          bind:value={draft.body}
          oninput={edited}
        ></textarea>
      {/if}
    {:else if list.length > 0}
      <p class="hint">{i18n.t.notes.pick}</p>
    {/if}
  </section>
</div>

<style>
  h1 {
    margin: 0 0 16px;
    font-size: 24px;
    font-weight: 600;
  }
  .notes {
    display: grid;
    grid-template-columns: 280px minmax(0, 1fr);
    gap: 28px;
    align-items: start;
  }
  .bar {
    display: flex;
    gap: 8px;
    margin-bottom: 8px;
  }
  .bar .field {
    flex: 1;
    min-width: 0;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .note {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 8px 10px;
    border-radius: 8px;
    text-align: left;
  }
  .note:hover {
    background: var(--hover);
  }
  .note[aria-current='true'] {
    background: var(--accent-soft);
  }
  .title {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 760px;
  }
  .heading {
    font-size: 18px;
    font-weight: 600;
  }
  .about {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
  .about strong {
    color: var(--accent);
    font-weight: 600;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .state {
    flex: 1;
    text-align: right;
  }
  .danger {
    color: var(--danger);
  }
  textarea {
    min-height: 52vh;
    line-height: 1.55;
    resize: vertical;
  }
  .md {
    min-height: 52vh;
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
  .md :global(ul),
  .md :global(ol) {
    margin: 0 0 6px;
    padding-left: 18px;
  }
  .md :global(a) {
    color: var(--accent);
  }
  .hint {
    color: var(--muted);
    font-size: 12px;
  }
  .error {
    color: var(--danger);
  }
</style>
