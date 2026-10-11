<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";

  async function startDragging(event: PointerEvent) {
    if (event.button === 0) {
      await getCurrentWindow().startDragging();
    }
  }
</script>

<main class="window">
  <div class="box" onpointerdown={startDragging}>
    <h1>Chat Window</h1>
  </div>
</main>

<style>
  :global(html),
  :global(body) {
    width: 100%;
    height: 100%;
    margin: 0;
  }

  .window {
    display: grid;
    width: 100%;
    height: 100%;
    place-items: center;
    color: #fff;
    user-select: none;
  }

  .box {
    cursor: grab;
    clip-path: inset(0 round 200px);
  }

  .box:active {
    cursor: grabbing;
  }

  .box {
    background: rgba(55, 70, 55, 0.247);
    width: 100%;
    height: 100%;
    border-radius: 200px;
    display: flex;
    justify-content: center;
    align-items: center;
  }
</style>
