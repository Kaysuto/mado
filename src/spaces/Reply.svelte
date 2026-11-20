<script lang="ts">
  import { type Item, type SavedReply, cancel, draftReply, sendReply, setSetting } from '../lib/api'
  import { i18n } from '../lib/i18n.svelte'

  let {
    item,
    saved,
    onclose,
    onsent,
  }: { item: Item; saved: SavedReply; onclose: () => void; onsent: () => void } = $props()

  // The saved draft only seeds the fields: from then on they belong to this panel.
  // svelte-ignore state_referenced_locally
  let intent = $state(saved.intent)
  // svelte-ignore state_referenced_locally
  let text = $state(saved.text)
  let writing = $state('') // id of the draft being written, to stop it
  let sending = $state(false)
  let error = $state('')
  let stopped = false

  const to = $derived(typeof item.meta.replyTo === 'string' ? item.meta.replyTo : '')

  /** The draft survives a restart: nobody wants to write an answer twice. */
  const keep = () => void setSetting('reply', { id: item.id, intent, text })

  async function draft() {
    writing = crypto.randomUUID()
    stopped = false
    error = ''
    text = ''
    try {
      const reply = await draftReply(writing, item.id, intent, piece => (text += piece))
      text = reply.text.trim()
    } catch (cause) {
      if (!stopped) error = String(cause)
    } finally {
      writing = ''
      keep()
    }
  }

  function stop() {
    stopped = true
    void cancel(writing)
  }

  async function send() {
    sending = true
    error = ''
    try {
      await sendReply(item.id, text)
      await setSetting('reply', null)
      onsent()
    } catch (cause) {
      error = String(cause)
    } finally {
      sending = false
    }
  }

  function close() {
    void setSetting('reply', null)
    onclose()
  }
</script>

<form
  class="reply"
  onsubmit={event => {
    event.preventDefault()
    void send()
  }}
>
  <!-- The address the mail asks to be answered at: shown, so the user sees who gets the reply. -->
  <p class="to">{i18n.t.reply.to} <strong>{to}</strong></p>
  <div class="row">
    <input
      class="field"
      placeholder={i18n.t.reply.intent}
      aria-label={i18n.t.reply.intent}
      disabled={Boolean(writing)}
      bind:value={intent}
      onchange={keep}
    />
    {#if writing}
      <button class="btn" type="button" onclick={stop}>{i18n.t.reply.stop}</button>
    {:else}
      <button class="btn" type="button" disabled={sending} onclick={draft}>{i18n.t.reply.draft}</button>
    {/if}
  </div>
  <textarea
    class="field"
    rows="7"
    placeholder={i18n.t.reply.placeholder}
    aria-label={i18n.t.reply.placeholder}
    readonly={Boolean(writing)}
    bind:value={text}
    onchange={keep}
  ></textarea>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
  <div class="row end">
    <button class="btn" type="button" disabled={sending} onclick={close}>{i18n.t.reply.cancel}</button>
    <button class="btn primary" type="submit" disabled={!text.trim() || Boolean(writing) || sending}>
      {sending ? i18n.t.reply.sending : i18n.t.reply.send}
    </button>
  </div>
</form>

<style>
  .reply {
    display: flex;
    flex: 1 1 100%;
    flex-direction: column;
    gap: 8px;
    padding: 4px 0 8px;
  }
  .to {
    margin: 0;
    color: var(--muted);
    font-size: 12px;
  }
  .to strong {
    color: var(--text);
    font-weight: 600;
    user-select: text;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .row .field {
    flex: 1;
  }
  .end {
    justify-content: flex-end;
  }
  textarea {
    resize: vertical;
    background: var(--bg);
    font-size: 13px;
    line-height: 1.5;
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: 13px;
  }
</style>
