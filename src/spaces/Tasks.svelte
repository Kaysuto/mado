<script lang="ts">
  import { onMount } from 'svelte'
  import { type Item, createTask, deleteLocal, listItems, onItemsChanged, openPage, setTaskDone } from '../lib/api'
  import { focus } from '../lib/focus.svelte'
  import { i18n } from '../lib/i18n.svelte'
  import { TASK_GROUPS, groupTasks } from '../lib/tasks'
  import { relativeTime } from '../lib/time'

  let items = $state<Item[]>([])
  let title = $state('')
  let error = $state('')
  const groups = $derived(groupTasks(items))
  const now = Date.now() / 1000

  async function guard(action: () => Promise<void>) {
    try {
      await action()
      error = ''
    } catch (cause) {
      error = String(cause)
    }
  }

  const load = () => guard(async () => void (items = await listItems({ kinds: ['task', 'ticket'] })))

  const add = () =>
    guard(async () => {
      if (!title.trim()) return
      await createTask(title)
      title = ''
    })

  onMount(() => {
    void load()
    const listening = onItemsChanged(load)
    return () => void listening.then(stop => stop())
  })
</script>

<h1>{i18n.t.spaces.tasks}</h1>

<form
  onsubmit={event => {
    event.preventDefault()
    void add()
  }}
>
  <input class="field" placeholder={i18n.t.tasks.add} aria-label={i18n.t.tasks.add} bind:value={title} />
</form>

{#if error}
  <p class="error" role="alert">{error}</p>
{/if}

{#if items.length === 0}
  <p class="hint">{i18n.t.tasks.empty}</p>
{/if}

{#each TASK_GROUPS as group (group)}
  {#if groups[group].length > 0}
    <h2>{i18n.t.tasks.groups[group]} <span class="count">{groups[group].length}</span></h2>
    <ul>
      {#each groups[group] as item (item.id)}
        {@const mine = item.source === 'local'}
        <li class:done={group === 'done'}>
          {#if mine}
            <input
              type="checkbox"
              checked={item.state === 'done'}
              aria-label={i18n.t.tasks.finish}
              onchange={event => guard(() => setTaskDone(item.id, event.currentTarget.checked))}
            />
          {:else}
            <span class="ref">{item.meta.identifier ?? i18n.t.kinds[item.kind]}</span>
          {/if}
          <span class="title">{item.title}</span>
          {#if !mine && item.state}<span class="hint">{item.state}</span>{/if}
          {#if item.endsAt}
            <span class="hint">{i18n.t.tasks.due} {relativeTime(item.endsAt, now, i18n.locale)}</span>
          {/if}
          {#if group !== 'done' && focus.session?.itemId !== item.id}
            <button class="btn" onclick={() => guard(() => focus.start(item))}>{i18n.t.focus.on}</button>
          {/if}
          {#if item.url}
            <button class="btn" onclick={() => openPage(item.url)}>{i18n.t.open}</button>
          {/if}
          {#if mine}
            <button class="btn" onclick={() => guard(() => deleteLocal(item.id))}>{i18n.t.tasks.delete}</button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
{/each}

<style>
  h1 {
    margin: 0 0 16px;
    font-size: 24px;
    font-weight: 600;
  }
  h2 {
    margin: 24px 0 4px;
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .count {
    opacity: 0.6;
  }
  form .field {
    width: 100%;
    max-width: 640px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 8px;
    border-bottom: 1px solid var(--border);
  }
  .title {
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }
  .done .title {
    color: var(--muted);
    font-weight: 400;
    text-decoration: line-through;
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
  .hint {
    color: var(--muted);
    font-size: 12px;
  }
  input[type='checkbox'] {
    flex: none;
    width: 16px;
    height: 16px;
    accent-color: var(--accent);
  }
  .error {
    color: var(--danger);
  }
</style>
