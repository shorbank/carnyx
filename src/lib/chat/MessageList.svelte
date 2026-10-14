<script lang="ts">
  import { tick } from "svelte";
  import type { Message } from "./message";
  import MessageItem from "./MessageItem.svelte";

  let { messages, ready, visible = true }: { messages: readonly Message[]; ready: boolean; visible?: boolean } = $props();
  let viewport: HTMLDivElement | undefined = $state();
  let restored = false;

  let wasVisible = false;
  let saved = { top: 0, followEnd: true, anchorId: undefined as string | undefined, offset: 0 };

  function position(element: HTMLDivElement, remaining?: Set<string>) {
    const top = element.scrollTop;
    const followEnd = !restored || element.scrollHeight - element.clientHeight - top <= 24;
    const viewportTop = element.getBoundingClientRect().top;
    const anchor = followEnd ? undefined : [...element.querySelectorAll<HTMLElement>("[data-message-id]")]
      .find((row) => (!remaining || remaining.has(row.dataset.messageId!)) && row.getBoundingClientRect().bottom > viewportTop);
    return { top, followEnd, anchorId: anchor?.dataset.messageId,
      offset: anchor ? anchor.getBoundingClientRect().top - viewportTop : 0 };
  }
  function remember() {
    if (viewport && viewport.clientHeight > 0) saved = position(viewport);
  }

  $effect.pre(() => {
    // Always observe messages, including when already following the bottom.
    const currentMessages = messages;
    const element = viewport;
    if (!element || !ready) return;
    const remaining = new Set(currentMessages.map((message) => message.id));
    if (!visible) {
      if (wasVisible) saved = position(element, remaining);
      wasVisible = false;
      return;
    }
    // A previously hidden pane cannot be measured until after its DOM update.
    const previous = wasVisible ? position(element, remaining) : saved;
    wasVisible = true;
    let cancelled = false;
    void tick().then(() => {
      if (cancelled) return;
      restored = true;
      const anchor = previous.anchorId ? [...element.querySelectorAll<HTMLElement>("[data-message-id]")]
        .find((row) => row.dataset.messageId === previous.anchorId) : undefined;
      element.scrollTop = previous.followEnd ? element.scrollHeight : previous.top;
      if (!previous.followEnd && anchor) {
        element.scrollTop += anchor.getBoundingClientRect().top - element.getBoundingClientRect().top - previous.offset;
      }
      saved = position(element);
    });
    return () => { cancelled = true; };
  });

</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users need to focus this region to scroll it.) -->
<div class="messages" bind:this={viewport} onscroll={remember} role="region" aria-label="Nachrichten" tabindex="0">
  {#if ready && messages.length === 0}
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
    padding: 16px;
  }

  .messages:focus-visible {
    outline: 2px solid var(--accent);
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
    font-size: 12px;
    color: var(--muted);
    padding-top: 30px;
  }
</style>
