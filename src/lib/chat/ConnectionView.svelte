<script lang="ts">
  import type { ConnectionState } from "./connection-store";
  import type { ConnectionSetup } from "./connection-setup";
  import SavedPeerActions from "./SavedPeerActions.svelte";
  import Field from "../ui/Field.svelte";
  import Disclosure from "../ui/Disclosure.svelte";
  import StatusNotice from "../ui/StatusNotice.svelte";
  import { connectionLabels } from "./connection-labels";
  import { peerError } from "./peer-errors";
  let { connection, controller }: { connection: ConnectionState; controller: ConnectionSetup } = $props();
  const form = $derived(controller.state);
  const active = $derived(connection.connection.phase !== "disconnected");
  const locked = $derived(active || connection.busy || $form.loading);
</script>
<div class="connection-view">
  <div class="intro"><h1>Verbindung</h1><p>Direkt miteinander schreiben.</p></div>
  <StatusNotice text={connectionLabels[connection.connection.phase]} />
  {#if connection.error || connection.setupError || connection.connection.error || $form.validation}
    <StatusNotice error text={$form.validation ?? connection.error ?? connection.setupError ?? peerError(connection.connection.error)}>{#if connection.setupError}<button type="button" disabled={connection.busy || $form.loading} onclick={() => { void controller.load(true); }}>Einrichtung erneut laden</button>{/if}</StatusNotice>
  {/if}
  {#if !connection.setup}
    <p class="hint">Die Einrichtung lädt deine gespeicherte Verbindung und die verfügbaren Netzwerkadressen.</p>
    <button type="button" disabled={$form.loading} onclick={() => { void controller.load(true); }}>{$form.loading ? "Einrichtung wird geladen …" : "Einrichtung erneut laden"}</button>
  {:else}
    {#if active}
      <section class="step"><h2>Verbindung läuft</h2><p class="hint">Du kannst zum Chat wechseln. Die Verbindung bleibt dabei aktiv.</p><button type="button" disabled={connection.busy} onclick={() => { void controller.disconnect(); }}>Verbindung trennen</button></section>
    {:else}
      {#if connection.setup.selectedPeer}
        <SavedPeerActions peer={connection.setup.selectedPeer} roleLabel={$form.roleLabel} canReconnect={$form.canReconnect} hint={$form.reconnectHint} busy={connection.busy || $form.loading} onreconnect={() => { void controller.reconnect(); }} onedit={controller.edit} />
      {/if}
      {#if $form.showEditor}
      <section class="step">
        <h2><span class="step-number">1</span> Eigenen Code teilen</h2>
        <p class="hint">Tauscht eure Codes über einen vertrauenswürdigen Weg aus.</p>
        <button type="button" disabled={locked || !$form.address} onclick={() => { void controller.generate(); }}>{connection.busy ? "Bitte warten …" : $form.ownCode ? "Code neu erzeugen" : "Eigenen Code anzeigen"}</button>
        {#if $form.ownCode}<Field id="own-code" label="Dein Verbindungscode"><textarea id="own-code" class="code" readonly value={$form.ownCode} rows="3" spellcheck="false"></textarea></Field><button type="button" onclick={() => { void controller.copyCode(); }}>Code kopieren</button>{#if $form.copyFeedback}<p class="hint" role="status">{$form.copyFeedback}</p>{/if}{/if}
      </section>
      <section class="step">
        <h2><span class="step-number">2</span> Gegenüber hinzufügen</h2>
        <Field id="peer-code" label="Verbindungscode des Gegenübers"><textarea id="peer-code" class="code" value={$form.peerCode} oninput={(event) => controller.setPeerCode(event.currentTarget.value)} disabled={locked} placeholder="Code hier einfügen …" rows="3" spellcheck="false"></textarea></Field>
        <p class="hint">Einer wartet, der andere verbindet.</p>
        <div class="actions"><button type="button" disabled={locked || !$form.peerCode.trim() || !$form.address} onclick={() => { void controller.connect("responder"); }}>Auf Peer warten</button><button type="button" class="primary" disabled={locked || !$form.peerCode.trim() || !$form.address} onclick={() => { void controller.connect("initiator"); }}>Verbinden</button></div>
      </section>
      {/if}
    {/if}
    <Disclosure label="Netzwerkdetails">
      <fieldset disabled={locked}><legend>Eigene Netzwerkadresse</legend>
        {#each connection.setup.addresses as address}
          <label class="address"><input type="radio" name="network-address" value={address} checked={$form.address === address} onchange={() => controller.setAddress(address)} /><span>{address}</span></label>
        {/each}
      </fieldset>
      {#if !connection.setup.addresses.length}<StatusNotice error text="Keine Netzwerkadresse verfügbar. Bitte deine Netzwerkverbindung prüfen." />{/if}
      <Field id="network-port" label="Port"><input id="network-port" type="text" inputmode="numeric" value={$form.port} oninput={(event) => controller.setPort(event.currentTarget.value)} disabled={locked} /></Field>
      <button type="button" disabled={locked} onclick={() => { void controller.load(true); }}>Netzwerkadressen neu laden</button>
    </Disclosure>
    <Disclosure label="Weitere Informationen">
      <div class="info"><span>Geräte-ID</span><code>{connection.setup.peerId}</code></div>
      <div class="info"><span>Öffentlicher Schlüssel</span><code>{connection.setup.publicKey ?? "Wird beim Anzeigen eines Codes oder beim Verbinden angelegt."}</code></div>
      <p class="hint">Nachrichten werden lokal gespeichert und nach 24 Stunden entfernt. Die Verbindung ist verschlüsselt; die lokale Nachrichtendatenbank selbst ist nicht verschlüsselt.</p>
      <p class="hint">„Beim Gegenüber gespeichert“ bestätigt die Speicherung, nicht das Lesen.</p>
    </Disclosure>
  {/if}
</div>
<style>
  .connection-view { height: 100%; overflow-y: auto; padding: 18px 16px 24px; display: flex; flex-direction: column; gap: 14px; }
  h1 { margin: 0 0 3px; font-size: 17px; font-weight: 600; }
  h2 { margin: 0; font-size: 13px; font-weight: 600; display: flex; align-items: center; gap: 8px; }
  .intro p, .hint { color: var(--muted); font-size: 12px; }
  .step { display: grid; gap: 10px; padding: 14px; background: var(--surface); border-radius: 10px; }
  .step-number { display: grid; place-items: center; width: 22px; height: 22px; border-radius: 6px; background: var(--surface-active); color: var(--accent); font-size: 11px; }
  .actions { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .actions button { padding: 8px 6px; font-size: 12px; }
  .code { font-family: ui-monospace, SFMono-Regular, Consolas, monospace; font-size: 11px; overflow-wrap: anywhere; }
  fieldset { margin: 0; padding: 0; border: 0; display: grid; gap: 6px; min-width: 0; }
  legend { color: var(--muted); font-size: 12px; margin-bottom: 8px; }
  .address { display: flex; align-items: center; gap: 10px; padding: 9px 10px; background: var(--surface); border: 1px solid var(--border); border-radius: 8px; cursor: pointer; }
  .address:has(input:checked) { border-color: var(--accent); background: var(--surface-active); }
  input[type="radio"] { appearance: none; width: 14px; height: 14px; margin: 0; border: 1px solid var(--muted); border-radius: 50%; background: var(--background); }
  input[type="radio"]:checked { border: 4px solid var(--accent); }
  .info { display: grid; gap: 4px; font-size: 12px; }
  .info span { color: var(--muted); }
  code { overflow-wrap: anywhere; font-size: 11px; user-select: text; }
</style>
