<script lang="ts">
  import type { Message } from "./message";
  import { messageLocalTime } from "./message";
  import { formatMessageTime } from "./message-time";

  let { message }: { message: Message } = $props();
</script>

<li data-message-id={message.id} class:outgoing={message.direction === "outgoing"} class:incoming={message.direction === "incoming"}>
  <small>{message.direction === "incoming" ? "Empfangen" : message.direction === "outgoing" ? "Du" : "Lokal"}</small>
  <p>{message.text}</p>
  <time datetime={new Date(messageLocalTime(message)).toISOString()}>
    {formatMessageTime(messageLocalTime(message))}
  </time>
  {#if message.direction === "outgoing"}
    <small class="delivery">{message.deliveryStatus === "acknowledged" ? "Beim Gegenüber gespeichert" : "Lokal gespeichert · unbestätigt"}</small>
  {/if}
</li>

<style>
  li {
    padding: 10px 12px;
    border-radius: 12px;
    background: rgba(0, 0, 0, 0.25);
    user-select: text;
  }
  .outgoing { margin-left: 14px; }
  .incoming { margin-right: 14px; }
  small { font-size: 0.7rem; opacity: 0.85; }
  .delivery { display: block; margin-top: 4px; }

  p {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    line-height: 1.4;
  }

  time {
    display: block;
    margin-top: 6px;
    font-size: 0.75rem;
    text-align: right;
  }
</style>
