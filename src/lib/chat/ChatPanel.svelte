<script lang="ts">
  import { createChatStore } from "./chat-store";
  import { createComposerDraft } from "./composer-draft";
  import { createConnectionStore } from "./connection-store";
  import { onMount } from "svelte";
  import { tauriChatApi } from "./chat-api";
  import { tauriPeerApi, peerChatApi } from "./peer-api";
  import ConnectionPanel from "./ConnectionPanel.svelte";
  import ConnectionStatus from "./ConnectionStatus.svelte";
  import ChatStatus from "./ChatStatus.svelte";
  import MessageList from "./MessageList.svelte";
  import MessageComposer from "./MessageComposer.svelte";

  let { ondragstart }: { ondragstart: (event: PointerEvent) => void } = $props();
  const connection = createConnectionStore(tauriPeerApi);
  const peerState = connection.state;
  let mode = $state<"local" | "peer">("local");
  const conversations = new Map<string, { chat: ReturnType<typeof createChatStore>; draft: ReturnType<typeof createComposerDraft> }>();
  function conversation(key: string) {
    let value = conversations.get(key);
    if (!value) {
      const chat = createChatStore(key === "local" ? tauriChatApi : peerChatApi(key));
      value = { chat, draft: createComposerDraft(chat.sendMessage) };
      conversations.set(key, value);
    }
    return value;
  }
  const peerId = $derived($peerState.setup?.selectedPeer?.peerId ?? null);
  const key = $derived(mode === "peer" && peerId ? peerId : "local");
  const active = $derived(conversation(key));
  const chatState = $derived(active.chat.state);
  const canSend = $derived($chatState.loaded && (key === "local" || ($peerState.connection.phase === "connected" && $peerState.connection.peerId === key)));
  $effect(() => { void active.chat.start(); });
  onMount(() => {
    void connection.start();
    const onfocus = () => { void connection.refresh(); void active.chat.refresh(); };
    window.addEventListener("focus", onfocus);
    return () => { window.removeEventListener("focus", onfocus); connection.dispose(); for (const value of conversations.values()) value.chat.dispose(); };
  });
</script>

<section class="chat" aria-label={key === "local" ? "Lokaler Chat" : "Peer-Chat"}>
  <header>
    <button type="button" class="drag-handle" onpointerdown={ondragstart} aria-label="Chat-Fenster verschieben">
      <strong>Carnyx</strong><span>{key === "local" ? "Lokaler Chat" : "Peer-Chat"}</span>
    </button>
  </header>
  <div>
    <nav aria-label="Unterhaltung" style="display:flex;gap:8px;padding:4px 12px">
      <button type="button" aria-pressed={mode === "local"} onclick={() => { mode = "local"; }}>Lokal</button>
      <button type="button" aria-pressed={mode === "peer"} disabled={!peerId} onclick={() => { mode = "peer"; }}>Peer</button>
    </nav>
    <ConnectionStatus connection={$peerState.connection} />
    <ConnectionPanel {connection} />
  </div>
  <div><ChatStatus state={$chatState} onretry={active.chat.refresh} /></div>
  {#key key}
    <MessageList messages={$chatState.messages} ready={$chatState.loaded} />
    <MessageComposer onsend={active.chat.sendMessage} disabled={!canSend} controller={active.draft} />
  {/key}
</section>

<style>
  .chat {
    display: grid;
    grid-template-rows: auto auto auto minmax(0, 1fr) auto;
    width: 100%;
    height: 100%;
    overflow: hidden;
    border-radius: 20px;
    background: rgba(55, 70, 55, 0.247);
  }

  header {
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
  }

  .drag-handle {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    padding: 16px;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: grab;
    user-select: none;
  }

  .drag-handle:active {
    cursor: grabbing;
  }

  .drag-handle:focus-visible {
    outline: 2px solid white;
    outline-offset: -4px;
  }

  span {
    font-size: 0.8rem;
  }
</style>
