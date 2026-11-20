<script lang="ts">
  import { onMount } from 'svelte'
  import { type SystemReading, listAgentSessions, systemReading } from './lib/api'
  import { i18n } from './lib/i18n.svelte'

  let { oncode }: { oncode: () => void } = $props()

  let reading = $state<SystemReading | null>(null)
  let working = $state(0)

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

  onMount(() => {
    void read()
    void count()
    const reads = setInterval(read, 5_000)
    const counts = setInterval(count, 15_000)
    return () => {
      clearInterval(reads)
      clearInterval(counts)
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
  {#if working > 0}
    <button onclick={oncode}>{i18n.t.status.agents} <strong>{working}</strong></button>
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
  button {
    margin-left: auto;
    font-size: 12px;
    color: var(--accent);
  }
  button strong {
    color: inherit;
  }
</style>
