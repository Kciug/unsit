<script lang="ts">
  import { fade } from 'svelte/transition'
  import { t } from '../lib/i18n'
  import { formatDuration, uiState } from '../lib/state'
  import { acceptBreak, escapeBreak } from '../lib/commands'

  const done = $derived($uiState.kind === 'done')
  const remaining = $derived(Math.max(0, $uiState.requiredSeconds - $uiState.earnedSeconds))
  const holdSeconds = $derived($uiState.escapeHoldSeconds || 5)

  let heldFor = $state(0)
  let holdStartedAt: number | null = null
  let holdTimer: ReturnType<typeof setInterval> | null = null

  const holdProgress = $derived(Math.min(1, heldFor / holdSeconds))

  function startHold() {
    if (holdTimer !== null) return

    holdStartedAt = Date.now()
    heldFor = 0
    holdTimer = setInterval(() => {
      if (holdStartedAt === null) return
      // Read from the clock rather than counting ticks, for the same reason the
      // backend keeps deadlines as timestamps: if the WebView throttles this
      // interval the elapsed time still comes out right, the bar just fills in
      // coarser steps.
      heldFor = (Date.now() - holdStartedAt) / 1000
      if (heldFor >= holdSeconds) {
        releaseHold()
        void escapeBreak()
      }
    }, 50)
  }

  function releaseHold() {
    if (holdTimer !== null) clearInterval(holdTimer)
    holdTimer = null
    holdStartedAt = null
    heldFor = 0
  }
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

    <!-- Friction, not a lock. Holding is the whole cost of leaving early, so a
         plain click must not be enough. -->
    <button
      class="escape"
      onpointerdown={startHold}
      onpointerup={releaseHold}
      onpointerleave={releaseHold}
      onpointercancel={releaseHold}
    >
      <span class="fill" style="width: {holdProgress * 100}%"></span>
      <span class="label">{$t('overlay.escapeHold', { seconds: holdSeconds })}</span>
    </button>
  {/if}
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
    align-items: center;
    justify-content: center;
    gap: 1rem;
    /* Pinned to all four edges rather than sized with 100vh. At fractional DPI
       scaling the viewport height and the window height disagree by a pixel,
       and that pixel shows as a transparent strip along the bottom. */
    position: fixed;
    inset: 0;
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
    position: relative;
    overflow: hidden;
    margin-top: 2rem;
    background: transparent;
    border-color: transparent;
    color: var(--muted);
    font-size: 0.85rem;
    touch-action: none;
  }

  .escape:hover {
    border-color: var(--border);
    color: var(--text);
  }

  /* Fills as you hold, so the cost is visible while you are paying it. */
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: var(--border);
    pointer-events: none;
  }

  .label {
    position: relative;
  }
</style>
