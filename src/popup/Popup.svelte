<script lang="ts">
  import { t } from '../lib/i18n'
  import { uiState } from '../lib/state'
  import { acceptBreak, extendForMatch, snooze } from '../lib/commands'

  const canSnooze = $derived($uiState.snoozesLeft > 0 && $uiState.nextSnoozeMinutes !== null)
</script>

<main>
  <h1>{$t('popup.title')}</h1>

  {#if $uiState.kind === 'prompt' && $uiState.snoozesLeft === 0}
    <p class="note">{$t('popup.overdue')}</p>
  {/if}

  <div class="actions">
    <button class="primary" onclick={acceptBreak}>{$t('popup.start')}</button>

    {#if canSnooze}
      <button onclick={snooze}>
        {$t('popup.snooze', { minutes: $uiState.nextSnoozeMinutes ?? 0 })}
      </button>
    {/if}

    {#if $uiState.matchExtensionAvailable}
      <button onclick={extendForMatch}>
        {$t('popup.finishMatch', { minutes: $uiState.matchExtensionMinutes })}
      </button>
    {/if}
  </div>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 1rem;
  }

  h1 {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
  }

  .note {
    margin: 0;
    color: var(--muted);
    font-size: 0.85rem;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
</style>
