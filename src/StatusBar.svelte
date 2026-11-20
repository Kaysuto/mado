<script lang="ts">
  import { onMount } from 'svelte'
  import {
    type MediaAction,
    type SystemReading,
    type Track,
    listAgentSessions,
    listItems,
    mediaControl,
    nowPlaying,
    onItemsChanged,
    systemReading,
  } from './lib/api'
  import type { Space } from './lib/spaces'
  import { i18n } from './lib/i18n.svelte'

  let { onshow }: { onshow: (space: Space) => void } = $props()

  let reading = $state<SystemReading | null>(null)
  let track = $state<Track | null>(null)
  let working = $state(0)
  let down = $state(0) // servers that stopped answering

  const GIGABYTE = 1024 ** 3
  const size = $derived(new Intl.NumberFormat(i18n.locale, { maximumFractionDigits: 1 }))
  const gigabytes = (bytes: number) => size.format(bytes / GIGABYTE)

  // A reading that fails leaves the bar as it was: there is nothing the user could do about it.
  async function read() {
    reading = await systemReading().catch(() => reading)
  }
  async function listen() {
    track = await nowPlaying().catch(() => null)
  }
  async function control(action: MediaAction) {
    await mediaControl(action).catch(() => {})
    await listen()
    // A player takes a moment to say it changed track or stopped.
    setTimeout(listen, 500)
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
    void listen()
    void count()
    void watch()
    const watching = onItemsChanged(watch)
    const reads = setInterval(() => (void read(), void listen()), 5_000)
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
  {#if track}
    <span class="music">
      <button aria-label={i18n.t.status.previous} onclick={() => control('previous')}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 2h2v12H3zM14 2v12L6 8z" /></svg>
      </button>
      <button aria-label={track.playing ? i18n.t.status.pause : i18n.t.status.play} onclick={() => control('toggle')}>
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d={track.playing ? 'M3 2h4v12H3zM9 2h4v12H9z' : 'M4 2v12l10-6z'} />
        </svg>
      </button>
      <button aria-label={i18n.t.status.next} onclick={() => control('next')}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M11 2h2v12h-2zM2 2v12l8-6z" /></svg>
      </button>
      <span class="track">
        <strong>{track.title}</strong>{#if track.artist}&nbsp;· {track.artist}{/if}
      </span>
    </span>
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
  .music {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .music button {
    display: flex;
    color: var(--muted);
  }
  .music button:hover {
    color: var(--text);
  }
  svg {
    width: 11px;
    height: 11px;
    fill: currentColor;
  }
  .track {
    max-width: 340px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
