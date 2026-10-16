<script lang="ts">
  import Field from "../ui/Field.svelte";
  import type { ConnectionMode, PeerRole } from "./peer-api";
  let { mode, ownCode, peerCode, copyFeedback, disabled, busy, ongenerate, oncopy, oncode, onconnect }: {
    mode: ConnectionMode; ownCode: string; peerCode: string; copyFeedback: string; disabled: boolean; busy: boolean;
    ongenerate: () => void; oncopy: () => void; oncode: (value: string) => void; onconnect: (role?: PeerRole) => void;
  } = $props();
</script>
<section class="step">
  <h2><span>1</span> Eigenen Code teilen</h2>
  <p>Tauscht eure Codes über einen vertrauenswürdigen Weg aus.</p>
  <button type="button" {disabled} onclick={ongenerate}>{busy ? "Bitte warten …" : ownCode ? "Code neu erzeugen" : "Eigenen Code anzeigen"}</button>
  {#if ownCode}
    <Field id="own-code" label="Dein Verbindungscode"><textarea id="own-code" readonly value={ownCode} rows="3" spellcheck="false"></textarea></Field>
    <button type="button" onclick={oncopy}>Code kopieren</button>
    {#if copyFeedback}<p role="status">{copyFeedback}</p>{/if}
  {/if}
</section>
<section class="step">
  <h2><span>2</span> Gegenüber hinzufügen</h2>
  <Field id="peer-code" label="Verbindungscode des Gegenübers"><textarea id="peer-code" value={peerCode} oninput={(event) => oncode(event.currentTarget.value)} {disabled} placeholder="Code hier einfügen …" rows="3" spellcheck="false"></textarea></Field>
  {#if mode === "internet"}
    <p>Beide öffnen Carnyx und starten die Verbindung.</p>
    <button type="button" class="primary" disabled={disabled || !peerCode.trim()} onclick={() => onconnect()}>Verbinden</button>
  {:else}
    <p>Einer wartet, der andere verbindet.</p>
    <div class="actions"><button type="button" disabled={disabled || !peerCode.trim()} onclick={() => onconnect("responder")}>Auf Peer warten</button><button type="button" class="primary" disabled={disabled || !peerCode.trim()} onclick={() => onconnect("initiator")}>Verbinden</button></div>
  {/if}
</section>
<style>
  .step { display: grid; gap: 10px; padding: 14px; background: var(--surface); border-radius: 10px; }
  h2 { margin: 0; font-size: 13px; font-weight: 600; display: flex; align-items: center; gap: 8px; }
  h2 span { display: grid; place-items: center; width: 22px; height: 22px; border-radius: 6px; background: var(--surface-active); color: var(--accent); font-size: 11px; }
  p { color: var(--muted); font-size: 12px; }
  textarea { font-family: ui-monospace, SFMono-Regular, Consolas, monospace; font-size: 11px; overflow-wrap: anywhere; }
  .actions { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .actions button { padding: 8px 6px; font-size: 12px; }
</style>
