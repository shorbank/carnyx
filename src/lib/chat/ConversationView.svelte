<script lang="ts">
  import type { Conversation } from "./chat-workspace";
  import ChatStatus from "./ChatStatus.svelte";
  import MessageList from "./MessageList.svelte";
  import MessageComposer from "./MessageComposer.svelte";
  let { conversation, canSend = true, visible = true }: { conversation: Conversation; canSend?: boolean; visible?: boolean } = $props();
  const chat = $derived(conversation.chat.state);
</script>
<div class="conversation">
  <div><ChatStatus state={$chat} onretry={conversation.chat.refresh} /></div>
  <MessageList messages={$chat.messages} ready={$chat.loaded} {visible} />
  <MessageComposer controller={conversation.draft} disabled={!$chat.loaded || !canSend} disabledReason={!canSend ? "Zum Senden im Tab Verbindung verbinden." : !$chat.loaded ? "Nachrichten werden noch nicht angezeigt." : ""} />
</div>
<style>.conversation { display: grid; grid-template-rows: auto minmax(0, 1fr) auto; height: 100%; min-height: 0; }</style>
