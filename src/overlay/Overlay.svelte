<script lang="ts">
  import { fade } from 'svelte/transition'
  import { t } from '../lib/i18n'
  import { formatDuration, uiState } from '../lib/state'
  import { acceptBreak, escapeBreak } from '../lib/commands'

  const done = $derived($uiState.kind === 'done')
  const remaining = $derived(Math.max(0, $uiState.requiredSeconds - $uiState.earnedSeconds))
</script>

<!-- Fading in matters: an overlay that appears instantly reads as a punishment. -->
<main transition:fade={{ duration: 400 }}>
  {#if done}
    <h1>{$t('overlay.doneHeading')}</h1>
    <p class="subtle">{$t('overlay.doneBody')}</p>

    <!-- The overlay waits here rather than clearing itself, so coming back to
         the desk is how the break ends, not the timer running out. -->
    <button class="primary" onclick={acceptBreak}>{$t('overlay.backToWork')}</button>
  {:else}
    <h1>{$t('overlay.heading')}</h1>

    <p class="clock" class:held={$uiState.counterHeld}>{formatDuration(remaining)}</p>

    {#if $uiState.counterHeld}
      <p class="held-note" transition:fade={{ duration: 150 }}>{$t('overlay.paused')}</p>
    {/if}

    <button class="escape" onclick={escapeBreak}>
      {#if $uiState.escapeMethod === 'hold'}
        {$t('overlay.escapeHold', { seconds: $uiState.escapeHoldSeconds })}
      {:else}
        {$t('overlay.escapeType')}
      {/if}
    </button>
  {/if}
</main>

<style>
  :global(body) {
    background: transparent;
    overflow: hidden;
  }

  main {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    height: 100vh;
    background: rgba(12, 14, 18, 0.88);
    user-select: none;
  }

  h1 {
    margin: 0;
    font-size: 1.5rem;
    font-weight: 500;
    letter-spacing: 0.02em;
  }

  .subtle {
    margin: 0;
    color: var(--muted);
  }

  .clock {
    margin: 0;
    font-size: clamp(4rem, 14vh, 9rem);
    font-variant-numeric: tabular-nums;
    font-weight: 200;
    line-height: 1;
    transition: color 200ms ease;
  }

  .clock.held {
    color: var(--muted);
  }

  .held-note {
    margin: 0;
    color: var(--accent);
    font-size: 1rem;
  }

  .primary {
    margin-top: 1rem;
    padding: 0.6rem 1.5rem;
  }

  /* Deliberately understated: the way out exists, but it does not invite you. */
  .escape {
    margin-top: 2rem;
    background: transparent;
    border-color: transparent;
    color: var(--muted);
    font-size: 0.85rem;
  }

  .escape:hover {
    border-color: var(--border);
    color: var(--text);
  }
</style>
