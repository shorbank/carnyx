<script lang="ts">
  import type { Message } from "./message";
  import { messageLocalTime } from "./message";
  import { formatMessageTime } from "./message-time";

  let { message }: { message: Message } = $props();
</script>

<li data-message-id={message.id}>
  <p>{message.text}</p>
  <time datetime={new Date(messageLocalTime(message)).toISOString()}>
    {formatMessageTime(messageLocalTime(message))}
  </time>
</li>

<style>
  li {
    padding: 10px 12px;
    border-radius: 12px;
    background: rgba(0, 0, 0, 0.25);
    user-select: text;
  }

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
