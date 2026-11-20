<script lang="ts">
  import { onMount } from 'svelte'
  import { type SystemReading, listAgentSessions, listItems, onItemsChanged, systemReading } from './lib/api'
  import type { Space } from './lib/spaces'
  import { i18n } from './lib/i18n.svelte'

  let { onshow }: { onshow: (space: Space) => void } = $props()

  let reading = $state<SystemReading | null>(null)
  let working = $state(0)
  let down = $state(0) // servers that stopped answering

  const GIGABYTE = 1024 ** 3
  const size = $derived(new Intl.NumberFormat(i18n.locale, { maximumFractionDigits: 1 }))
  const gigabytes = (bytes: number) => size.format(bytes / GIGABYTE)

  // A reading that fails leaves the bar as it was: there is nothing the user could do about it.
  async function read() {
    reading = await systemReading().catch(() => reading)
  }
  async function count() {
    const sessions = await listAgentSessions().catch(() => [])
    working = sessions.filter(session => session.state === 'working').length
  }

  async function watch() {
    const alerts = await listItems({ kinds: ['alert'] }).catch(() => [])
    down = alerts.filter(alert => alert.meta.server === true && alert.state === 'down').length
  }

  onMount(() => {
    void read()
    void count()
    void watch()
    const watching = onItemsChanged(watch)
    const reads = setInterval(read, 5_000)
    const counts = setInterval(count, 15_000)
    return () => {
      clearInterval(reads)
      clearInterval(counts)
      void watching.then(stop => stop())
    }
  })
</script>

<footer aria-label={i18n.t.status.title}>
  {#if reading}
    <span>{i18n.t.status.cpu} <strong>{Math.round(reading.cpu)} %</strong></span>
    <span>
      {i18n.t.status.memory}
      <strong>{gigabytes(reading.memoryUsed)} / {gigabytes(reading.memoryTotal)} {i18n.t.status.gigabytes}</strong>
    </span>
    {#if reading.disk}
      <span>
        {i18n.t.status.disk}
        <strong>{gigabytes(reading.disk.free)} {i18n.t.status.gigabytes} {i18n.t.status.free}</strong>
      </span>
    {/if}
  {/if}
  <span class="space"></span>
  {#if down > 0}
    <button class="down" onclick={() => onshow('inbox')}>{i18n.t.status.serversDown} <strong>{down}</strong></button>
  {/if}
  {#if working > 0}
    <button onclick={() => onshow('code')}>{i18n.t.status.agents} <strong>{working}</strong></button>
  {/if}
</footer>

<style>
  footer {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 20px;
    min-height: 28px;
    padding: 0 16px;
    border-top: 1px solid var(--border);
    background: var(--surface);
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  strong {
    color: var(--text);
    font-weight: 600;
  }
  .space {
    flex: 1;
  }
  .down {
    color: var(--danger);
  }
  button {
    font-size: 12px;
    color: var(--accent);
  }
  button strong {
    color: inherit;
  }
</style>
