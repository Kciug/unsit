<script lang="ts">
  import { onMount } from 'svelte'
  import { locale, t, type Key, type Locale } from '../lib/i18n'
  import {
    getSettings,
    setAutostart,
    setLocale,
    setProfileTimes,
    setSound,
  } from '../lib/commands'
  import { uiState, type ProfileSettings, type SettingsView } from '../lib/state'

  const locales: { value: Locale; label: string }[] = [
    { value: 'en', label: 'English' },
    { value: 'pl', label: 'Polski' },
  ]

  // Profiles come from a one-off fetch. Locale and autostart do not — they ride
  // the state event, so the tray and this window can never disagree.
  let settings = $state<SettingsView | null>(null)

  async function refresh() {
    settings = await getSettings()
  }

  onMount(refresh)

  function chooseLocale(event: Event) {
    const value = (event.currentTarget as HTMLSelectElement).value as Locale
    // Ask the backend and let the state event come back. Setting the store
    // directly would drift from the tray menu.
    void setLocale(value).catch(() => locale.set(value))
  }

  async function commit(profile: ProfileSettings) {
    await setProfileTimes(profile.key, profile.intervalMin, profile.breakMin)
    // Re-read, because the backend clamps: a typed 0 comes back as 1.
    await refresh()
  }

  function label(key: string): string {
    const translated = $t(`mode.${key}` as Key)
    return translated === `mode.${key}` ? key : translated
  }
</script>

<main>
  <h1>{$t('settings.title')}</h1>

  <section>
    <h2>{$t('settings.general')}</h2>

    <label>
      {$t('settings.language')}
      <select value={$locale} onchange={chooseLocale}>
        {#each locales as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>

    <label class="checkbox">
      <input
        type="checkbox"
        checked={$uiState.autostart}
        onchange={(event) => void setAutostart(event.currentTarget.checked)}
      />
      {$t('settings.autostart')}
    </label>

    {#if settings}
      <label class="checkbox">
        <input
          type="checkbox"
          checked={settings.sound}
          onchange={async (event) => {
            await setSound(event.currentTarget.checked)
            await refresh()
          }}
        />
        {$t('settings.sound')}
      </label>
    {/if}
  </section>

  {#if settings}
    <section>
      <h2>{$t('settings.modes')}</h2>

      {#each settings.profiles as profile (profile.key)}
        <div class="profile" class:active={profile.key === settings.activeProfile}>
          <span class="name">{label(profile.key)}</span>

          <label>
            {$t('settings.interval')}
            <input
              type="number"
              min="1"
              max="480"
              bind:value={profile.intervalMin}
              onchange={() => commit(profile)}
            />
            <span class="unit">{$t('settings.minutes')}</span>
          </label>

          <label>
            {$t('settings.break')}
            <input
              type="number"
              min="1"
              max="120"
              bind:value={profile.breakMin}
              onchange={() => commit(profile)}
            />
            <span class="unit">{$t('settings.minutes')}</span>
          </label>
        </div>
      {/each}
    </section>
  {/if}

  <p class="note">{$t('settings.more')}</p>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    padding: 1.5rem;
  }

  h1 {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 600;
  }

  h2 {
    margin: 0 0 0.75rem;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .checkbox {
    gap: 0.5rem;
  }

  .profile {
    display: grid;
    grid-template-columns: 5.5rem 1fr 1fr;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: 6px;
  }

  /* The mode you are actually in, so an edit is never made to the wrong one by
     accident. */
  .profile.active {
    border-color: var(--accent);
  }

  .name {
    font-weight: 600;
  }

  .unit {
    color: var(--muted);
    font-size: 0.8rem;
  }

  select,
  input[type='number'] {
    font: inherit;
    color: inherit;
    width: 4.5rem;
    padding: 0.3rem 0.4rem;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
  }

  select {
    width: auto;
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: 0.8rem;
  }
</style>
