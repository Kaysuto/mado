<script lang="ts">
  import { onMount } from 'svelte'
  import {
    type ProviderView,
    type Reply,
    ask,
    cancel,
    deleteProvider,
    getSetting,
    listModels,
    listProviders,
    saveProvider,
    setSetting,
  } from './lib/api'
  import { i18n } from './lib/i18n.svelte'
  import { PRESETS, PRESET_GROUPS } from './lib/presets'

  const TIERS = ['fast', 'strong'] as const
  type Tier = (typeof TIERS)[number]
  interface Choice {
    provider: string
    model: string
  }
  interface Trial {
    requestId?: string
    reply?: Reply
    seconds?: number
    error?: string
  }

  let providers = $state<ProviderView[]>([])
  let choices = $state<Record<Tier, Choice>>({
    fast: { provider: '', model: '' },
    strong: { provider: '', model: '' },
  })
  let models = $state<Record<string, string[]>>({})
  let trials = $state<Record<Tier, Trial>>({ fast: {}, strong: {} })
  let error = $state('')
  let armed = $state('') // id of the provider whose removal waits for a second click

  // The form to add a provider.
  let presetId = $state(PRESETS[0].id)
  let name = $state('')
  let baseUrl = $state('')
  let key = $state('')
  const preset = $derived(PRESETS.find(p => p.id === presetId) ?? PRESETS[0])

  $effect(() => {
    name = preset.name
    baseUrl = preset.baseUrl
    key = ''
  })

  async function guard(action: () => Promise<void>) {
    try {
      await action()
      error = ''
    } catch (cause) {
      error = String(cause)
    }
  }

  const refresh = () => guard(async () => void (providers = await listProviders()))

  onMount(async () => {
    await refresh()
    const saved = await getSetting<Record<Tier, Choice>>('models')
    if (saved) choices = { ...choices, ...saved }
    for (const tier of TIERS) void loadModels(choices[tier].provider)
  })

  /** Fetches the provider's model list once. A failure only means no suggestions. */
  async function loadModels(provider: string) {
    if (!provider || models[provider]) return
    try {
      models[provider] = await listModels(provider)
    } catch {
      models[provider] = []
    }
  }

  function choose(tier: Tier) {
    trials[tier] = {}
    void loadModels(choices[tier].provider)
    void guard(() => setSetting('models', choices))
  }

  async function test(tier: Tier) {
    const requestId = crypto.randomUUID()
    const started = performance.now()
    trials[tier] = { requestId }
    try {
      const reply = await ask(requestId, choices[tier].provider, {
        model: choices[tier].model,
        messages: [{ role: 'user', content: 'Reply with the single word: ok' }],
      })
      trials[tier] = { reply, seconds: (performance.now() - started) / 1000 }
    } catch (cause) {
      trials[tier] = { error: String(cause) }
    }
  }

  const add = () =>
    guard(async () => {
      await saveProvider({ id: crypto.randomUUID(), kind: preset.kind, name: name.trim(), baseUrl: baseUrl.trim() }, key)
      key = ''
      await refresh()
    })

  function remove(id: string) {
    if (armed !== id) {
      armed = id
      setTimeout(() => armed === id && (armed = ''), 4000)
      return
    }
    armed = ''
    void guard(async () => {
      await deleteProvider(id)
      await refresh()
    })
  }
</script>

<h2>{i18n.t.models.title}</h2>
<p class="hint">{i18n.t.models.intro}</p>

{#each TIERS as tier (tier)}
  {@const trial = trials[tier]}
  <div class="tier">
    <div class="what">
      <span class="name">{i18n.t.models.tiers[tier]}</span>
      <span class="hint">{i18n.t.models.tierHints[tier]}</span>
    </div>
    <select
      class="field"
      aria-label={i18n.t.models.provider}
      bind:value={choices[tier].provider}
      onchange={() => choose(tier)}
    >
      <option value="">{i18n.t.models.none}</option>
      {#each providers as provider (provider.id)}
        <option value={provider.id}>{provider.name}</option>
      {/each}
    </select>
    <input
      class="field"
      placeholder={i18n.t.models.model}
      aria-label={i18n.t.models.model}
      list="models-{tier}"
      bind:value={choices[tier].model}
      onchange={() => choose(tier)}
    />
    <datalist id="models-{tier}">
      {#each models[choices[tier].provider] ?? [] as model (model)}
        <option value={model}></option>
      {/each}
    </datalist>
    {#if trial.requestId}
      <button class="btn" onclick={() => cancel(trial.requestId!)}>{i18n.t.models.cancel}</button>
    {:else}
      <button class="btn" disabled={!choices[tier].provider} onclick={() => test(tier)}>
        {i18n.t.models.test}
      </button>
    {/if}
  </div>
  {#if trial.requestId}
    <p class="hint" role="status">{i18n.t.models.testing}</p>
  {:else if trial.reply}
    <p class="ok" role="status">
      {i18n.t.models.works} « {trial.reply.text.trim().slice(0, 80)} » · {trial.seconds?.toFixed(1)} s
      {#if trial.reply.inputTokens !== null}
        · {trial.reply.inputTokens} + {trial.reply.outputTokens ?? 0} {i18n.t.models.tokens}
      {/if}
    </p>
  {:else if trial.error}
    <p class="error" role="alert">{trial.error}</p>
  {/if}
{/each}

<h3>{i18n.t.models.providers}</h3>
{#if providers.length === 0}
  <p class="hint">{i18n.t.models.noProvider}</p>
{/if}
<ul>
  {#each providers as provider (provider.id)}
    <li>
      <span class="name">{provider.name}</span>
      <span class="hint">{i18n.t.models.kinds[provider.kind]}</span>
      {#if provider.hasKey}<span class="badge">{i18n.t.models.keySaved}</span>{/if}
      {#if provider.installed === false}<span class="badge warn">{i18n.t.models.notInstalled}</span>{/if}
      <button class="btn" onclick={() => remove(provider.id)}>
        {armed === provider.id ? i18n.t.models.confirmRemove : i18n.t.models.remove}
      </button>
    </li>
  {/each}
</ul>

<form
  onsubmit={event => {
    event.preventDefault()
    void add()
  }}
>
  <select class="field" aria-label={i18n.t.models.provider} bind:value={presetId}>
    {#each PRESET_GROUPS as group (group)}
      <optgroup label={i18n.t.models.groups[group]}>
        {#each PRESETS.filter(p => p.group === group) as option (option.id)}
          <option value={option.id}>{option.name || i18n.t.models.custom}</option>
        {/each}
      </optgroup>
    {/each}
  </select>
  <input class="field" placeholder={i18n.t.models.name} aria-label={i18n.t.models.name} required bind:value={name} />
  {#if preset.kind === 'openai_compat'}
    <input
      class="field wide"
      type="url"
      placeholder="https://…/v1"
      aria-label={i18n.t.models.address}
      required
      bind:value={baseUrl}
    />
  {/if}
  {#if preset.key !== 'none'}
    <input
      class="field wide"
      type="password"
      autocomplete="off"
      placeholder={i18n.t.models.key}
      aria-label={i18n.t.models.key}
      required={preset.key === 'required'}
      bind:value={key}
    />
  {/if}
  <button class="btn" type="submit">{i18n.t.models.add}</button>
</form>
{#if preset.group === 'subscription'}
  <p class="hint">{i18n.t.models.subscriptionHint}</p>
{:else if preset.key !== 'none'}
  <p class="hint">{i18n.t.models.keyHint}</p>
{/if}

{#if error}
  <p class="error" role="alert">{error}</p>
{/if}

<style>
  h2 {
    margin: 32px 0 4px;
    font-size: 18px;
    font-weight: 600;
  }
  h3 {
    margin: 24px 0 8px;
    font-size: 14px;
    font-weight: 600;
  }
  p {
    max-width: 760px;
    margin: 6px 0;
  }
  .hint {
    color: var(--muted);
    font-size: 13px;
  }
  .name {
    font-weight: 600;
  }
  .tier,
  li,
  form {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    max-width: 760px;
  }
  .tier {
    padding: 12px 0 4px;
  }
  .what {
    display: flex;
    flex: 1 1 200px;
    flex-direction: column;
  }
  .tier input {
    flex: 1 1 180px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
  }
  li .btn {
    margin-left: auto;
  }
  form {
    margin-top: 12px;
  }
  .wide {
    flex: 1 1 260px;
  }
  .badge {
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11px;
    font-weight: 600;
  }
  .badge.warn,
  .error {
    color: var(--danger);
  }
  .badge.warn {
    background: rgba(252, 165, 165, 0.15);
  }
  .ok {
    color: var(--ok);
    font-size: 13px;
  }
</style>
