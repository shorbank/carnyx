<script lang="ts">
  import type { createConnectionStore } from "./connection-store";
  let { connection }: { connection: ReturnType<typeof createConnectionStore> } = $props();
  const status = $derived(connection.state);
  let address = $state("");
  let port = $state(32147);
  let code = $state("");
  let generatedFor = $state("");
  const active = $derived($status.connection.phase !== "disconnected");
  $effect(() => { if (!address && $status.setup?.addresses.length) address = $status.setup.addresses[0]; });
  async function generate() { generatedFor = `${address}:${port}`; await connection.code(address, port); }
</script>
<details ontoggle={(event) => { if (event.currentTarget.open) void connection.setup(); }}>
  <summary>Verbindung einrichten</summary>
  <div class="setup">
    {#if $status.error}<p role="alert">{$status.error}</p>{/if}
    {#if $status.setup}
      <p>Beide tauschen ihren Code über einen vertrauenswürdigen Weg aus. Einer wartet, der andere verbindet.</p>
      <label>Eigene Netzwerkadresse
        <select bind:value={address} disabled={active || $status.busy}>
          {#each $status.setup.addresses as ip}<option value={ip}>{ip}</option>{/each}
        </select>
      </label>
      <label>Port <input type="number" min="1" max="65535" bind:value={port} disabled={active || $status.busy} /></label>
      <button type="button" disabled={!address || !port || active || $status.busy} onclick={() => { void generate(); }}>Eigenen Code anzeigen</button>
      {#if $status.code && generatedFor === `${address}:${port}`}
        <label>Eigener Verbindungscode · markieren und kopieren<textarea readonly value={$status.code} rows="2"></textarea></label>
      {/if}
      <label>Code des Gegenübers<textarea bind:value={code} rows="2" disabled={active || $status.busy}></textarea></label>
      <div class="actions">
        {#if active}
          <button type="button" disabled={$status.busy} onclick={() => { void connection.disconnect(); }}>Trennen</button>
        {:else}
          <button type="button" disabled={!code.trim() || !address || !port || $status.busy} onclick={() => { void connection.connect(code, "responder", address, port); }}>Auf Peer warten</button>
          <button type="button" disabled={!code.trim() || !address || !port || $status.busy} onclick={() => { void connection.connect(code, "initiator", address, port); }}>Verbinden</button>
        {/if}
      </div>
    {:else}
      <button type="button" onclick={() => { void connection.setup(); }}>Einrichtung laden</button>
    {/if}
  </div>
</details>
<style>
  details { padding: 4px 12px; font-size: 0.8rem; }
  summary { cursor: pointer; }
  .setup { display: grid; gap: 6px; max-height: 190px; overflow-y: auto; padding: 6px 0; }
  p { margin: 0; }
  label { display: grid; gap: 3px; }
  input, select, textarea, button { box-sizing: border-box; width: 100%; border: 1px solid rgba(255,255,255,.35); border-radius: 6px; padding: 5px; color: inherit; background: rgba(0,0,0,.35); font: inherit; }
  option { background: #243024; }
  textarea { resize: none; }
  .actions { display: flex; gap: 6px; }
  button { cursor: pointer; }
  button:disabled { opacity: .5; cursor: default; }
</style>
