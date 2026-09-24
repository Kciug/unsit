<script lang="ts">
  import { t } from '../lib/i18n'
  import { closeFlyout, openSettings, switchMode, togglePause } from '../lib/commands'
  import { flyout, formatDuration, uiState } from '../lib/state'

  const paused = $derived($uiState.kind === 'paused')

  // A break running is its own countdown; otherwise it is the wait for one.
  const remaining = $derived(
    $uiState.kind === 'break'
      ? Math.max(0, $uiState.requiredSeconds - $uiState.earnedSeconds)
      : ($uiState.secondsLeft ?? null),
  )

  async function pick(key: string) {
    await switchMode(key)
  }

  async function pause() {
    await togglePause()
    await closeFlyout()
  }
</script>

<main>
  <div class="clock">
    <span class="time">{remaining === null ? '—' : formatDuration(remaining)}</span>
    <span class="caption">
      {#if paused}
        {$t('tray.pause')}
      {:else if $uiState.kind === 'break'}
        {$t('overlay.heading')}
      {:else}
        {$t('popup.title')}
      {/if}
    </span>
  </div>

  {#if $flyout}
    <div class="modes">
      {#each $flyout.modes as mode (mode.key)}
        <button class:current={mode.key === $flyout.active} onclick={() => pick(mode.key)}>
          {mode.label}
        </button>
      {/each}
    </div>
  {/if}

  <div class="actions">
    <button onclick={pause}>
      {paused ? $t('tray.resume') : $t('tray.pause')}
    </button>
    <button onclick={() => void openSettings()}>{$t('tray.settings')}</button>
  </div>
</main>

<style>
  :global(html),
  :global(body) {
    height: 100%;
    margin: 0;
    background: transparent;
    overflow: hidden;
  }

  main {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    /* Pinned rather than sized in vh: at fractional DPI the viewport and the
       window disagree by a pixel, and the panel border shows the gap. */
    position: fixed;
    inset: 0;
    padding: 1rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: rgba(22, 24, 29, 0.97);
    box-sizing: border-box;
    user-select: none;
  }

  .clock {
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .time {
    font-size: 2.1rem;
    font-weight: 200;
    font-variant-numeric: tabular-nums;
    line-height: 1.1;
  }

  .caption {
    color: var(--muted);
    font-size: 0.75rem;
  }

  .modes,
  .actions {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .actions {
    flex-direction: row;
    margin-top: auto;
  }

  .actions button {
    flex: 1;
  }

  button {
    padding: 0.4rem 0.6rem;
    font-size: 0.85rem;
    text-align: center;
  }

  /* The mode you are in, so the panel answers "which one am I?" at a glance. */
  .current {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
