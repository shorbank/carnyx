<script lang="ts">
  import type { Invitation } from "./peer-api";
  let { peer, roleLabel, canReconnect, hint, busy, onreconnect, onedit, compact = false }: {
    peer: Invitation; roleLabel: string; canReconnect: boolean; hint: string; busy: boolean;
    onreconnect: () => void; onedit: () => void; compact?: boolean;
  } = $props();
</script>
<section class="saved-peer" class:compact aria-label="Gespeicherte Verbindung">
  {#if !compact}<h2>Gespeichertes Gegenüber</h2><p class="identity">{peer.peerId.slice(0, 8)} · {peer.target.mode === "lan" ? peer.target.address : "Internet"}</p><p class="hint">{roleLabel}</p>{/if}
  {#if hint}<p class="hint">{hint}</p>{/if}
  <div class="actions"><button type="button" class="primary" disabled={!canReconnect} onclick={onreconnect}>{busy ? "Bitte warten …" : "Erneut verbinden"}</button><button type="button" disabled={busy} onclick={onedit}>Einrichtung ändern</button></div>
</section>
<style>
  .saved-peer { display: grid; gap: 8px; padding: 14px; border-radius: 10px; background: var(--surface); }
  h2 { margin: 0; font-size: 13px; font-weight: 600; }
  .identity { font-size: 12px; overflow-wrap: anywhere; }
  .hint { color: var(--muted); font-size: 12px; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; }
  .actions button { flex: 1; padding: 8px; white-space: nowrap; font-size: 12px; }
  .compact { background: var(--background); padding: 10px 16px; border-bottom: 1px solid var(--border); border-radius: 0; }
  .compact .hint { font-size: 11px; }
</style>
