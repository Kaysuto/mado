<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { type ChatMessage, cancel, chat, getSetting, openLinks, setSetting } from './lib/api'
  import { i18n } from './lib/i18n.svelte'
  import { markdown } from './lib/markdown'

  let { onclose }: { onclose: () => void } = $props()

  let messages = $state<ChatMessage[]>([])
  let draft = $state('')
  let writing = $state('') // id of the answer being written, to stop it
  let partial = $state('') // what has been written of it so far
  let error = $state('')
  let stopped = false
  let list = $state<HTMLElement>()
  let input = $state<HTMLTextAreaElement>()

  onMount(async () => {
    const saved = await getSetting<ChatMessage[]>('chat')
    if (Array.isArray(saved)) messages = saved
    input?.focus()
  })

  // Keeps the latest text in view while it is being written.
  $effect(() => {
    void [messages.length, partial]
    void tick().then(() => list?.scrollTo({ top: list.scrollHeight }))
  })

  async function send() {
    const text = draft.trim()
    if (!text || writing) return
    draft = ''
    error = ''
    stopped = false
    messages = [...messages, { role: 'user', content: text }]
    const requestId = crypto.randomUUID()
    writing = requestId
    partial = ''
    try {
      const reply = await chat(requestId, i18n.locale, messages, piece => (partial += piece))
      messages = [...messages, { role: 'assistant', content: reply.text }]
    } catch (cause) {
      // What was already written when the answer was stopped is kept.
      if (partial.trim()) messages = [...messages, { role: 'assistant', content: partial }]
      if (!stopped) error = String(cause)
    } finally {
      writing = ''
      partial = ''
      void setSetting('chat', messages)
    }
  }

  function stop() {
    stopped = true
    void cancel(writing)
  }

  function reset() {
    messages = []
    error = ''
    void setSetting('chat', [])
    input?.focus()
  }

  function onkeydown(event: KeyboardEvent) {
    // Enter sends; Shift+Enter makes a new line. Never while a character is being composed.
    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
      event.preventDefault()
      void send()
    }
  }
</script>

<aside aria-label={i18n.t.assistant.title}>
  <div class="head">
    <h2>{i18n.t.assistant.title}</h2>
    <button class="btn" disabled={Boolean(writing) || messages.length === 0} onclick={reset}>
      {i18n.t.assistant.fresh}
    </button>
    <button class="btn" aria-label={i18n.t.assistant.close} onclick={onclose}>×</button>
  </div>

  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="messages" bind:this={list} onclick={openLinks} aria-live="polite">
    {#if messages.length === 0 && !writing}
      <p class="hint">{i18n.t.assistant.empty}</p>
    {/if}
    {#each messages as message, index (index)}
      <div class="message {message.role}">
        {#if message.role === 'user'}
          {message.content}
        {:else}
          {@html markdown(message.content)}
        {/if}
      </div>
    {/each}
    {#if writing}
      <div class="message assistant">
        {#if partial}
          {@html markdown(partial)}
        {:else}
          <span class="hint">{i18n.t.assistant.thinking}</span>
        {/if}
      </div>
    {/if}
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  </div>

  <form
    onsubmit={event => {
      event.preventDefault()
      void send()
    }}
  >
    <textarea
      class="field"
      rows="2"
      placeholder={i18n.t.assistant.placeholder}
      aria-label={i18n.t.assistant.placeholder}
      bind:this={input}
      bind:value={draft}
      {onkeydown}
    ></textarea>
    {#if writing}
      <button class="btn" type="button" onclick={stop}>{i18n.t.assistant.stop}</button>
    {:else}
      <button class="btn primary" type="submit" disabled={!draft.trim()}>{i18n.t.assistant.send}</button>
    {/if}
  </form>
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-left: 1px solid var(--border);
    background: var(--surface);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .messages {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    padding: 16px;
    overflow-y: auto;
  }
  .message {
    max-width: 92%;
    padding: 8px 12px;
    border-radius: 12px;
    font-size: 13px;
    line-height: 1.5;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .message.user {
    align-self: flex-end;
    background: var(--accent-soft);
    white-space: pre-wrap;
  }
  .message.assistant {
    align-self: flex-start;
    background: var(--hover);
  }
  .message :global(p),
  .message :global(ul),
  .message :global(ol),
  .message :global(pre) {
    margin: 0 0 6px;
  }
  .message :global(:last-child) {
    margin-bottom: 0;
  }
  .message :global(ul),
  .message :global(ol) {
    padding-left: 18px;
  }
  .message :global(h4) {
    margin: 8px 0 4px;
    font-size: 13px;
  }
  .message :global(pre) {
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--bg);
    overflow-x: auto;
  }
  .message :global(code) {
    font-family: 'Cascadia Code', Consolas, monospace;
    font-size: 12px;
  }
  .message :global(a) {
    color: var(--accent);
  }
  form {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    padding: 12px 16px 16px;
    border-top: 1px solid var(--border);
  }
  textarea {
    flex: 1;
    resize: none;
    background: var(--bg);
    font-size: 13px;
  }
  .hint {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: 13px;
  }
</style>
