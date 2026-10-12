<script lang="ts">
  import type { Message } from "./message";

  let { message }: { message: Message } = $props();
  const timeFormatter = new Intl.DateTimeFormat("de-AT", {
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  });
</script>

<li>
  <p>{message.text}</p>
  <time datetime={new Date(message.createdAt).toISOString()}>
    {timeFormatter.format(message.createdAt)}
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
