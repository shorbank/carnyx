<script lang="ts">
  import "$lib/styles/theme.css";
  import ChatPanel from "$lib/components/shell/ChatPanel.svelte";
  import { createChatWorkspace } from "$lib/controllers/chat-workspace";
  import { tauriChatApi } from "$lib/adapters/chat-api";
  import { tauriPeerApi, peerChatApi } from "$lib/adapters/peer-api";
  import { tauriWindowApi } from "$lib/adapters/window-api";
  import { browserClipboardApi } from "$lib/adapters/clipboard-api";
  import { onMount } from "svelte";
  import StorageGate from "$lib/components/storage/StorageGate.svelte";
  import { createStorageController } from "$lib/controllers/storage-controller";
  import { tauriStorageApi } from "$lib/adapters/storage-api";
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
