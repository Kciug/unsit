<script lang="ts">
  import { untrack } from 'svelte'
  import { t } from './i18n'

  interface Props {
    minutes: number
    max: number
    onchange: (minutes: number) => void
  }

  let { minutes, max, onchange }: Props = $props()

  // Minutes stay the stored unit everywhere — one unit in the config file is
  // worth far more than the convenience of writing hours into it. This is
  // presentation only.
  // Read once on purpose: this is which unit to *show*, and it must not jump
  // back under the cursor the moment a typed value stops dividing evenly.
  let unit = $state<'min' | 'h'>(
    untrack(() => (minutes >= 60 && minutes % 15 === 0 ? 'h' : 'min')),
  )

  const shown = $derived(unit === 'h' ? minutes / 60 : minutes)
  const limit = $derived(unit === 'h' ? Math.floor(max / 60) : max)

  function commit(event: Event) {
    const raw = Number((event.currentTarget as HTMLInputElement).value)
    if (!Number.isFinite(raw) || raw <= 0) return
    onchange(unit === 'h' ? Math.round(raw * 60) : Math.round(raw))
  }
</script>

<span class="field">
  <input
    type="number"
    min={unit === 'h' ? 0.25 : 1}
    max={limit}
    step={unit === 'h' ? 0.25 : 1}
    value={shown}
    onchange={commit}
  />
  <select bind:value={unit}>
    <option value="min">{$t('settings.minutes')}</option>
    <option value="h">{$t('settings.hours')}</option>
  </select>
</span>

<style>
  .field {
    display: inline-flex;
    gap: 0.3rem;
  }

  input {
    width: 4.5rem;
  }

  input,
  select {
    font: inherit;
    color: inherit;
    padding: 0.3rem 0.4rem;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
  }
</style>
