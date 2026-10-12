<script lang="ts">
  import { createChatStore } from "./chat-store";
  import MessageList from "./MessageList.svelte";
  import MessageComposer from "./MessageComposer.svelte";

  let { ondragstart }: { ondragstart: (event: PointerEvent) => void } = $props();
  const { messages, sendMessage } = createChatStore();
</script>

<section class="chat" aria-label="Lokaler Chat">
  <header>
    <button type="button" class="drag-handle" onpointerdown={ondragstart}
      aria-label="Chat-Fenster verschieben">
      <strong>Carnyx</strong>
      <span>Lokaler Chat</span>
    </button>
  </header>
  <MessageList messages={$messages} />
  <MessageComposer onsend={sendMessage} />
</section>

<style>
  .chat {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
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
