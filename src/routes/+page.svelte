<script lang="ts">
  import "$lib/ui/theme.css";
  import ChatPanel from "$lib/chat/ChatPanel.svelte";
  import { createChatWorkspace } from "$lib/chat/chat-workspace";
  import { tauriChatApi } from "$lib/chat/chat-api";
  import { tauriPeerApi, peerChatApi } from "$lib/chat/peer-api";
  import { tauriWindowApi } from "$lib/chat/window-api";
  import { browserClipboardApi } from "$lib/chat/clipboard-api";
  const workspace = createChatWorkspace({ local: tauriChatApi, peer: peerChatApi, connection: tauriPeerApi, clipboard: browserClipboardApi });
</script>
<main><ChatPanel {workspace} windowApi={tauriWindowApi} /></main>
<style>main { width: 100%; height: 100%; }</style>
