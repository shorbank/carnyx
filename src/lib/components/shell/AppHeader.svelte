<script lang="ts">
  import IconButton from "$lib/components/ui/IconButton.svelte";
  import appLogo from "../../../../assets/app-icons/Icon-Windows-Default.png";
  let { ondrag, onminimize, onclose }: { ondrag: () => void; onminimize: () => void; onclose: () => void } = $props();
</script>
<header>
  <button type="button" class="drag-handle" aria-label="Carnyx-Fenster verschieben" onpointerdown={(event) => { if (event.button === 0) ondrag(); }}><img class="brand-mark" src={appLogo} alt="" width="24" height="24" draggable="false" /><strong>Carnyx</strong></button>
  <div class="controls"><IconButton icon="minus" label="Fenster minimieren" onclick={onminimize} /><IconButton icon="close" label="App schließen" onclick={onclose} /></div>
</header>
<style>
  header { display: flex; align-items: center; padding: 6px 8px 6px 14px; min-height: 48px; }
  .drag-handle { flex: 1; align-self: stretch; display: flex; align-items: center; gap: 9px; border: 0; border-radius: 0; padding: 0; background: transparent; text-align: left; cursor: grab; user-select: none; }
  .drag-handle:hover { background: transparent; }
  .drag-handle:active { cursor: grabbing; }
  strong { font-size: 13px; font-weight: 600; letter-spacing: .2px; }
  .brand-mark { display: block; width: 24px; height: 24px; flex-shrink: 0; object-fit: contain; }
  .controls { display: flex; gap: 2px; }
</style>
