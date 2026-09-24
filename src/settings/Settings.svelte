<script lang="ts">
  import { onMount } from 'svelte'
  import DurationField from '../lib/DurationField.svelte'
  import { locale, t, type Key, type Locale } from '../lib/i18n'
  import {
    addProfile,
    getSettings,
    removeProfile,
    saveProfile,
    setAutostart,
    setLocale,
    setSound,
  } from '../lib/commands'
  import {
    uiState,
    type Escalation,
    type ProfileEdit,
    type ProfileSettings,
    type SettingsView,
  } from '../lib/state'

  const locales: { value: Locale; label: string }[] = [
    { value: 'en', label: 'English' },
    { value: 'pl', label: 'Polski' },
  ]

  // Lock is missing on purpose: hard mode is not built, and offering a ceiling
  // the ladder never reaches would be another button that lies.
  const escalations: Escalation[] = ['toast', 'popup', 'overlay']

  // Modes come from a one-off fetch. Locale and autostart do not — they ride
  // the state event, so the tray and this window can never disagree.
  let settings = $state<SettingsView | null>(null)
  let newMode = $state('')

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

  /** Sends the whole mode back, then re-reads: the backend clamps. */
  async function commit(profile: ProfileSettings) {
    const { key, ...edit } = profile
    await saveProfile(key, edit as ProfileEdit)
    await refresh()
  }

  async function add() {
    if (!newMode.trim()) return
    await addProfile(newMode)
    newMode = ''
    await refresh()
  }

  async function remove(key: string) {
    await removeProfile(key)
    await refresh()
  }

  function label(key: string): string {
    const translated = $t(`mode.${key}` as Key)
    return translated === `mode.${key}` ? key : translated
  }

  /** "5, 3" both ways — a list is easier to type than it is to build a widget for. */
  function parseSnoozes(raw: string): number[] {
    return raw
      .split(',')
      .map((part) => Number(part.trim()))
      .filter((value) => Number.isFinite(value) && value > 0)
  }

  function optional(event: Event): number | null {
    const raw = (event.currentTarget as HTMLInputElement).value.trim()
    if (raw === '') return null
    const value = Number(raw)
    return Number.isFinite(value) && value > 0 ? value : null
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

      <!-- Read-only: capturing a key combination properly is its own piece of
           work, and config.toml can already change it. -->
      <div class="row">
        <span>{$t('settings.hotkey')}</span>
        <kbd>{settings.hotkey}</kbd>
      </div>
      <p class="note" class:warn={!settings.hotkeyRegistered}>
        {settings.hotkeyRegistered ? $t('settings.hotkeyHint') : $t('settings.hotkeyTaken')}
      </p>
    {/if}
  </section>

  {#if settings}
    <section>
      <h2>{$t('settings.modes')}</h2>

      {#each settings.profiles as profile (profile.key)}
        <details class:active={profile.key === settings.activeProfile}>
          <summary>
            <span class="name">{label(profile.key)}</span>
            <span class="summary-times">
              {profile.intervalMin} / {profile.breakMin} {$t('settings.minutes')}
            </span>
          </summary>

          <div class="fields">
            <label>
              {$t('settings.interval')}
              <DurationField
                minutes={profile.intervalMin}
                max={720}
                onchange={(minutes) => {
                  profile.intervalMin = minutes
                  void commit(profile)
                }}
              />
            </label>

            <label>
              {$t('settings.break')}
              <DurationField
                minutes={profile.breakMin}
                max={120}
                onchange={(minutes) => {
                  profile.breakMin = minutes
                  void commit(profile)
                }}
              />
            </label>

            <label>
              {$t('settings.warning')}
              <input
                type="number"
                min="0"
                max="3600"
                bind:value={profile.warningSec}
                onchange={() => commit(profile)}
              />
              <span class="unit">{$t('settings.seconds')}</span>
            </label>

            <label>
              {$t('settings.escalation')}
              <select
                value={profile.maxEscalation}
                onchange={(event) => {
                  profile.maxEscalation = event.currentTarget.value as Escalation
                  void commit(profile)
                }}
              >
                {#each escalations as step (step)}
                  <option value={step}>{$t(`escalation.${step}` as Key)}</option>
                {/each}
              </select>
            </label>

            <label class="wide">
              {$t('settings.snoozes')}
              <input
                type="text"
                value={profile.snoozesMin.join(', ')}
                onchange={(event) => {
                  profile.snoozesMin = parseSnoozes(event.currentTarget.value)
                  void commit(profile)
                }}
              />
            </label>
            <p class="note">{$t('settings.snoozesHint')}</p>

            <h3>{$t('settings.gaming')}</h3>

            <label>
              {$t('settings.matchExtension')}
              <input
                type="number"
                min="1"
                max="120"
                placeholder={$t('settings.off')}
                value={profile.matchExtensionMin ?? ''}
                onchange={(event) => {
                  profile.matchExtensionMin = optional(event)
                  void commit(profile)
                }}
              />
              <span class="unit">{$t('settings.minutes')}</span>
            </label>

            <label>
              {$t('settings.sessionLimit')}
              <input
                type="number"
                min="1"
                max="720"
                placeholder={$t('settings.off')}
                value={profile.sessionLimitMin ?? ''}
                onchange={(event) => {
                  profile.sessionLimitMin = optional(event)
                  void commit(profile)
                }}
              />
              <span class="unit">{$t('settings.minutes')}</span>
            </label>

            <label>
              {$t('settings.sessionBreak')}
              <input
                type="number"
                min="1"
                max="120"
                placeholder={$t('settings.off')}
                value={profile.sessionBreakMin ?? ''}
                onchange={(event) => {
                  profile.sessionBreakMin = optional(event)
                  void commit(profile)
                }}
              />
              <span class="unit">{$t('settings.minutes')}</span>
            </label>

            {#if settings.profiles.length > 1}
              <button class="remove" onclick={() => remove(profile.key)}>
                {$t('settings.remove')}
              </button>
            {/if}
          </div>
        </details>
      {/each}

      <div class="row">
        <input
          type="text"
          placeholder={$t('settings.modeName')}
          bind:value={newMode}
          onkeydown={(event) => {
            if (event.key === 'Enter') void add()
          }}
        />
        <button onclick={add} disabled={!newMode.trim()}>{$t('settings.addMode')}</button>
      </div>
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

  h3 {
    margin: 0.5rem 0 0;
    font-size: 0.7rem;
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

  label,
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .checkbox {
    gap: 0.5rem;
  }

  details {
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.5rem 0.75rem;
  }

  /* The mode you are actually in, so an edit is never made to the wrong one by
     accident. */
  details.active {
    border-color: var(--accent);
  }

  summary {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
    cursor: pointer;
  }

  .fields {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding-top: 0.75rem;
  }

  .name {
    font-weight: 600;
  }

  .summary-times,
  .unit {
    color: var(--muted);
    font-size: 0.8rem;
  }

  label.wide input {
    flex: 1;
  }

  select,
  input[type='number'],
  input[type='text'] {
    font: inherit;
    color: inherit;
    width: 6rem;
    padding: 0.3rem 0.4rem;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
  }

  select {
    width: auto;
  }

  .remove {
    align-self: flex-start;
    margin-top: 0.25rem;
    font-size: 0.8rem;
  }

  .remove:hover {
    border-color: #c96a6a;
    color: #e08b8b;
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: 0.8rem;
  }

  .note.warn {
    color: #e0a34a;
  }

  kbd {
    padding: 0.15rem 0.45rem;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface);
    font-family: inherit;
    font-size: 0.8rem;
  }
</style>
