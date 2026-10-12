<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { isTauri } from "@tauri-apps/api/core";
  import ChatPanel from "$lib/chat/ChatPanel.svelte";

  async function startDragging(event: PointerEvent) {
    if (event.button === 0 && isTauri()) {
      await getCurrentWindow().startDragging();
    }
  }
</script>

<main class="window">
  <ChatPanel ondragstart={startDragging} />
</main>

<style>
  :global(html),
  :global(body) {
    width: 100%;
    height: 100%;
    margin: 0;
    overflow: hidden;
    font-family: system-ui, sans-serif;
  }

  .window {
    display: grid;
    width: 100%;
    height: 100%;
    place-items: center;
    color: #fff;
  }
</style>
