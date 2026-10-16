<script lang="ts">
  import type { Message } from "$lib/models/message";
  import { messageLocalTime } from "$lib/models/message";
  import { formatMessageTime } from "$lib/presentation/message-time";
  import Icon from "$lib/components/ui/Icon.svelte";
  let { message }: { message: Message } = $props();
  let hovered = $state(false);
  let focused = $state(false);
  let dismissed = $state(false);
  const acknowledged = $derived(message.deliveryStatus === "acknowledged");
  const delivery = $derived(acknowledged ? "Beim Gegenüber gespeichert" : "Lokal gespeichert · unbestätigt");
</script>
<li data-message-id={message.id} class:outgoing={message.direction === "outgoing"} class:incoming={message.direction === "incoming"}>
  <span class="sr-only">{message.direction === "incoming" ? "Empfangen" : message.direction === "outgoing" ? "Du" : "Lokal"}</span>
  <p>{message.text}</p>
  <div class="metadata">
    <time datetime={new Date(messageLocalTime(message)).toISOString()}>{formatMessageTime(messageLocalTime(message))}</time>
    {#if message.direction === "outgoing"}
      <button type="button" class="delivery" class:acknowledged aria-label={delivery} title={delivery} onmouseenter={() => { hovered = true; dismissed = false; }} onmouseleave={() => { hovered = false; }} onfocus={() => { focused = true; dismissed = false; }} onblur={() => { focused = false; }} onkeydown={(event) => { if (event.key === "Escape") { dismissed = true; event.preventDefault(); } }}><Icon name={acknowledged ? "check" : "pending"} size={14} /><span class="tooltip" class:shown={(hovered || focused) && !dismissed} aria-hidden="true">{delivery}</span></button>
    {/if}
  </div>
</li>
<style>
  li { justify-self: start; max-width: 90%; min-width: 60px; padding: 9px 11px 7px; border-radius: 10px; background: var(--surface); user-select: text; }
  .outgoing { justify-self: end; background: var(--surface-active); }
  .incoming { border-radius: 10px 10px 10px 3px; }
  p { white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.5; }
  .metadata { display: flex; align-items: center; justify-content: flex-end; gap: 6px; margin-top: 4px; color: var(--muted); font-size: 10px; }
  .delivery { position: relative; display: inline-flex; cursor: help; padding: 0; border: 0; background: transparent; color: inherit; border-radius: 3px; }
  .delivery:hover { background: transparent; }
  .acknowledged { color: var(--accent); }
  .tooltip { display: none; position: absolute; z-index: 1; right: 0; bottom: calc(100% + 6px); width: 180px; padding: 7px 9px; border: 1px solid var(--border); border-radius: 6px; background: var(--background); color: var(--text); font-size: 11px; }
  .tooltip.shown { display: block; }
</style>
