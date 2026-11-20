<script lang="ts">
  import { onMount } from 'svelte'
  import { clock, duration, elapsed } from './lib/focus'
  import { focus } from './lib/focus.svelte'
  import { i18n } from './lib/i18n.svelte'

  let now = $state(Date.now())
  let armed = $state(false) // "Cancel" was clicked once
  let error = $state('')

  onMount(() => {
    void guard(() => focus.restore())
    const ticking = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(ticking)
  })

  async function guard(action: () => Promise<void>) {
    try {
      await action()
      error = ''
    } catch (cause) {
      error = String(cause)
    }
  }

  // Two clicks, so a slip on the button does not lose a long session.
  function cancel() {
    if (!armed) {
      armed = true
      setTimeout(() => (armed = false), 4000)
      return
    }
    armed = false
    focus.cancel()
  }
</script>

<section aria-label={i18n.t.focus.title}>
  {#if focus.session}
    {@const session = focus.session}
    <span class="what">{session.title || i18n.t.focus.free}</span>
    <span class="clock" class:paused={session.runSince === null} role="timer">{clock(elapsed(session, now))}</span>
    <div class="row">
      <button class="btn" onclick={() => focus.toggle()}>
        {session.runSince === null ? i18n.t.focus.resume : i18n.t.focus.pause}
      </button>
      <button class="btn primary" onclick={() => guard(() => focus.finish())}>{i18n.t.focus.finish}</button>
      <button class="btn danger" onclick={cancel}>{armed ? i18n.t.focus.confirm : i18n.t.focus.cancel}</button>
    </div>
  {:else}
    <button class="btn" onclick={() => guard(() => focus.start())}>{i18n.t.focus.start}</button>
    <span class="hint">{i18n.t.focus.today} {duration(focus.today)}</span>
  {/if}
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  .what {
    overflow: hidden;
    font-size: 12px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .clock {
    font-size: 24px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .clock.paused {
    color: var(--muted);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .btn {
    padding: 4px 10px;
    font-size: 12px;
  }
  .danger {
    color: var(--danger);
  }
  .hint {
    color: var(--muted);
    font-size: 12px;
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: 12px;
  }
</style>
