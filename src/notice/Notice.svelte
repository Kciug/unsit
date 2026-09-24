<script lang="ts">
  import { fade } from 'svelte/transition'
  import { notice } from '../lib/state'
</script>

<!--
  A dumb renderer. The backend sends already-translated text and decides when
  this window appears and disappears, because in a game it stands in for a
  Windows notification and has to behave like one: brief, unclickable, gone.
-->
{#if $notice}
  <main transition:fade={{ duration: 200 }}>
    <h1>{$notice.title}</h1>
    {#if $notice.body}
      <p>{$notice.body}</p>
    {/if}
  </main>
{/if}

<style>
  :global(body) {
    background: transparent;
    overflow: hidden;
  }

  main {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    height: 100vh;
    padding: 0.9rem 1.1rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: rgba(22, 24, 29, 0.96);
    box-sizing: border-box;
    user-select: none;
  }

  h1 {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
  }

  p {
    margin: 0;
    color: var(--muted);
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
  }
</style>
