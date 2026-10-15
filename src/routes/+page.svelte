<script lang="ts">
  import "$lib/ui/theme.css";
  import ChatPanel from "$lib/chat/ChatPanel.svelte";
  import { createChatWorkspace } from "$lib/chat/chat-workspace";
  import { tauriChatApi } from "$lib/chat/chat-api";
  import { tauriPeerApi, peerChatApi } from "$lib/chat/peer-api";
  import { tauriWindowApi } from "$lib/chat/window-api";
  import { browserClipboardApi } from "$lib/chat/clipboard-api";
  import { onMount } from "svelte";
  import StorageGate from "$lib/storage/StorageGate.svelte";
  import { createStorageController } from "$lib/storage/storage-controller";
  import { tauriStorageApi } from "$lib/storage/storage-api";
  const storage = createStorageController(tauriStorageApi);
  const storageStatus = storage.state;
  onMount(() => { void storage.start(); return () => storage.dispose(); });
  const workspace = createChatWorkspace({ local: tauriChatApi, peer: peerChatApi, connection: tauriPeerApi, clipboard: browserClipboardApi });
</script>
<main>
  {#if $storageStatus.phase === "ready"}
    <ChatPanel {workspace} windowApi={tauriWindowApi} />
  {:else}
    <StorageGate status={$storageStatus} onretry={() => void storage.retry()} windowApi={tauriWindowApi} />
  {/if}
</main>
<style>main { width: 100%; height: 100%; }</style>
