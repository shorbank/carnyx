<script lang="ts">
  import type { Conversation } from "$lib/controllers/chat-workspace";
  import ChatStatus from "$lib/components/chat/ChatStatus.svelte";
  import MessageList from "$lib/components/chat/MessageList.svelte";
  import MessageComposer from "$lib/components/chat/MessageComposer.svelte";
  let { conversation, canSend = true, visible = true }: { conversation: Conversation; canSend?: boolean; visible?: boolean } = $props();
  const chat = $derived(conversation.chat.state);
</script>
<div class="conversation">
  <div><ChatStatus state={$chat} onretry={conversation.chat.refresh} /></div>
  <MessageList messages={$chat.messages} ready={$chat.loaded} {visible} />
  <MessageComposer controller={conversation.draft} disabled={!$chat.loaded || !canSend} disabledReason={!canSend ? "Zum Senden die Verbindung wiederherstellen." : !$chat.loaded ? "Nachrichten werden noch nicht angezeigt." : ""} />
</div>
<style>.conversation { display: grid; grid-template-rows: auto minmax(0, 1fr) auto; height: 100%; min-height: 0; }</style>
