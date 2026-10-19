<script lang="ts">
  import type { WorkspaceView } from "$lib/controllers/chat-workspace";
  import type { PeerState } from "$lib/models/connection";
  import { connectionLabels } from "$lib/presentation/connection-labels";
  let { selected, connection, onselect }: { selected: WorkspaceView; connection: PeerState; onselect: (view: WorkspaceView) => void } = $props();
  const tabs: { id: WorkspaceView; label: string }[] = [{ id: "local", label: "Lokal" }, { id: "peer", label: "Peer" }, { id: "connection", label: "Verbindung" }];
  let buttons = $state<HTMLButtonElement[]>([]);
  function keydown(event: KeyboardEvent, index: number) {
    let next: number;
    if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
    else if (event.key === "ArrowLeft") next = (index + tabs.length - 1) % tabs.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = tabs.length - 1;
    else return;
    event.preventDefault();
    onselect(tabs[next].id);
    buttons[next]?.focus();
  }
</script>
<div class="tabs" role="tablist" aria-label="Carnyx-Bereiche">
  {#each tabs as tab, index}
    <button bind:this={buttons[index]} type="button" role="tab" id={`tab-${tab.id}`} aria-controls={`panel-${tab.id}`} aria-selected={selected === tab.id} tabindex={selected === tab.id ? 0 : -1} class:active={selected === tab.id} onclick={() => onselect(tab.id)} onkeydown={(event) => keydown(event, index)}>
      {tab.label}
      {#if tab.id === "peer"}<span class="indicator" class:connected={connection.phase === "connected"} class:working={!["connected", "disconnected"].includes(connection.phase)} title={connectionLabels[connection.phase]}><span class="sr-only">{connectionLabels[connection.phase]}</span></span>{/if}
    </button>
  {/each}
</div>
<style>
  .tabs { display: grid; grid-template-columns: 1fr 1fr 1.25fr; padding: 0 12px; border-bottom: 1px solid var(--border); }
  button { position: relative; display: flex; align-items: center; justify-content: center; gap: 7px; height: 40px; border: 0; border-radius: 6px 6px 0 0; padding: 0 8px; background: transparent; color: var(--muted); font-size: 12px; }
  button:hover { background: var(--surface); }
  button.active { background: var(--surface); color: var(--text); }
  button.active::after { content: ""; position: absolute; bottom: -1px; height: 2px; left: 12px; right: 12px; background: var(--accent); }
  button:focus-visible { outline-offset: -3px; }
  .indicator { width: 6px; height: 6px; border-radius: 50%; background: var(--muted); }
  .indicator.connected { background: var(--accent); }
  .indicator.working { background: var(--warning); }
</style>
