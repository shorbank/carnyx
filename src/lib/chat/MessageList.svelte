<script lang="ts">
  import { tick } from "svelte";
  import type { Message } from "./message";
  import MessageItem from "./MessageItem.svelte";

  let { messages }: { messages: readonly Message[] } = $props();
  let viewport: HTMLDivElement | undefined = $state();

  $effect.pre(() => {
    const count = messages.length;
    const element = viewport;
    if (!element || count === 0) return;

    // Measure against the old DOM before the new message changes its height.
    const previousTop = element.scrollTop;
    const followEnd = count === 1 ||
      element.scrollHeight - element.clientHeight - previousTop <= 24;
    let cancelled = false;

    void tick().then(() => {
      if (!cancelled) {
        element.scrollTop = followEnd ? element.scrollHeight : previousTop;
      }
    });

    return () => { cancelled = true; };
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users need to focus this region to scroll it.) -->
<div class="messages" bind:this={viewport} role="region" aria-label="Nachrichten" tabindex="0">
  {#if messages.length === 0}
    <p class="empty">Noch keine Nachrichten</p>
  {:else}
    <ol>
      {#each messages as message (message.id)}
        <MessageItem {message} />
      {/each}
    </ol>
  {/if}
</div>

<style>
  .messages {
    min-height: 0;
    overflow-y: auto;
    overflow-anchor: none;
    padding: 12px;
  }

  .messages:focus-visible {
    outline: 2px solid white;
    outline-offset: -4px;
  }

  ol {
    display: grid;
    gap: 8px;
    padding: 0;
    margin: 0;
    list-style: none;
  }

  .empty {
    text-align: center;
    font-size: 0.9rem;
  }
</style>
