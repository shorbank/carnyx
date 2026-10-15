<script lang="ts">
  import AppHeader from "../chat/AppHeader.svelte";
  import ResizeHandle from "../chat/ResizeHandle.svelte";
  import StatusNotice from "../ui/StatusNotice.svelte";
  import type { WindowApi } from "../chat/window-api";
  import { storageErrorText, type StorageStatus } from "./storage-controller";
  let { status, onretry, windowApi }: { status: StorageStatus; onretry: () => void; windowApi: WindowApi } = $props();
  let windowError = $state("");
  async function windowAction(action: () => Promise<void>) {
    try { await action(); windowError = ""; } catch { windowError = "Die Fensteraktion ist momentan nicht verfügbar."; }
  }
</script>
<section class="gate" aria-label="Historie entsperren">
  <AppHeader ondrag={() => void windowAction(windowApi.drag)} onminimize={() => void windowAction(windowApi.minimize)} onclose={() => void windowAction(windowApi.close)} />
  <div class="content">
    <h1>{status.phase === "blocked" ? "Historie nicht verfügbar" : "Historie wird vorbereitet"}</h1>
    {#if status.phase === "blocked"}
      <StatusNotice error text={storageErrorText(status.code)} />
      <p>Carnyx bleibt gesperrt, bis die verschlüsselte Historie bereit ist. Deine Daten werden nicht zurückgesetzt.</p>
      <button class="primary" type="button" onclick={onretry}>Erneut versuchen</button>
    {:else}
      <StatusNotice text="Historie entsperren und Verschlüsselung prüfen …" />
      <p>Beim ersten Start mit bestehender Historie wird diese automatisch verschlüsselt. Das kann einen Moment dauern.</p>
    {/if}
    {#if windowError}<StatusNotice error text={windowError} />{/if}
  </div>
  <ResizeHandle onresize={() => void windowAction(windowApi.resize)} />
</section>
<style>
  .gate { height: 100%; display: flex; flex-direction: column; position: relative; background: var(--background); }
  .content { display: grid; align-content: start; gap: 16px; padding: 24px 18px; overflow: auto; min-height: 0; flex: 1; }
  h1 { margin: 0; font-size: 17px; font-weight: 600; }
  p { color: var(--muted); font-size: 12px; }
  button { justify-self: start; }
</style>
