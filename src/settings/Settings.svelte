<script lang="ts">
  import { locale, t, type Locale } from '../lib/i18n'
  import { setAutostart, setLocale } from '../lib/commands'
  import { uiState } from '../lib/state'

  const locales: { value: Locale; label: string }[] = [
    { value: 'en', label: 'English' },
    { value: 'pl', label: 'Polski' },
  ]

  // The backend owns the locale, so ask it to change and let the state event
  // come back to us. Setting the store directly would drift from the tray menu.
  function choose(event: Event) {
    const value = (event.currentTarget as HTMLSelectElement).value as Locale
    void setLocale(value).catch(() => locale.set(value))
  }
</script>

<main>
  <h1>{$t('settings.title')}</h1>

  <label>
    {$t('settings.language')}
    <select value={$locale} onchange={choose}>
      {#each locales as option (option.value)}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
  </label>

  <label>
    <input
      type="checkbox"
      checked={$uiState.autostart}
      onchange={(event) => void setAutostart(event.currentTarget.checked)}
    />
    {$t('settings.autostart')}
  </label>

  <p class="note">{$t('settings.notImplemented')}</p>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.5rem;
  }

  h1 {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 600;
  }

  label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  select {
    font: inherit;
    color: inherit;
    padding: 0.35rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>
