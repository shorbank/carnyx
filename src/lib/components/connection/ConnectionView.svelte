<script lang="ts">
  import type { ConnectionState } from "$lib/stores/connection-store";
  import type { ConnectionSetup } from "$lib/controllers/connection-setup";
  import SavedPeerActions from "$lib/components/connection/SavedPeerActions.svelte";
  import ConnectionModeSelector from "$lib/components/connection/ConnectionModeSelector.svelte";
  import ConnectionCodeFields from "$lib/components/connection/ConnectionCodeFields.svelte";
  import LanSettings from "$lib/components/connection/LanSettings.svelte";
  import Disclosure from "$lib/components/ui/Disclosure.svelte";
  import StatusNotice from "$lib/components/ui/StatusNotice.svelte";
  import { connectionLabels, connectionPathLabel } from "$lib/presentation/connection-labels";
  import { peerError } from "$lib/presentation/peer-errors";
  let { connection, controller }: { connection: ConnectionState; controller: ConnectionSetup } = $props();
  const form = $derived(controller.state);
  const active = $derived(connection.connection.phase !== "disconnected");
  const locked = $derived(active || connection.busy || $form.loading);
</script>
<div class="connection-view">
  <div class="intro"><h1>Verbindung</h1><p>Verschlüsselt miteinander schreiben.</p></div>
  <StatusNotice text={connectionLabels[connection.connection.phase]} />
  {#if connection.error || connection.setupError || connection.connection.error || $form.validation}
    <StatusNotice error text={$form.validation ?? connection.error ?? connection.setupError ?? peerError(connection.connection.error)}>{#if connection.setupError}<button type="button" disabled={connection.busy || $form.loading} onclick={() => { void controller.load(true); }}>Einrichtung erneut laden</button>{/if}</StatusNotice>
  {/if}
  {#if !connection.setup}
    <p class="hint">Die gespeicherte Verbindung wird geladen.</p>
    <button type="button" disabled={$form.loading} onclick={() => { void controller.load(true); }}>{$form.loading ? "Einrichtung wird geladen …" : "Einrichtung erneut laden"}</button>
  {:else}
    {#if active}
      <section class="step"><h2>Verbindung läuft</h2><p class="hint">Du kannst zum Chat wechseln. Die Verbindung bleibt dabei aktiv.</p><button type="button" disabled={connection.busy} onclick={() => { void controller.disconnect(); }}>Verbindung trennen</button></section>
    {:else}
      {#if connection.setup.selectedPeer}
        <SavedPeerActions peer={connection.setup.selectedPeer} roleLabel={$form.roleLabel} canReconnect={$form.canReconnect} hint={$form.reconnectHint} busy={connection.busy || $form.loading} onreconnect={() => { void controller.reconnect(); }} onedit={controller.edit} />
      {/if}
      {#if $form.showEditor}
        <ConnectionModeSelector mode={$form.mode} disabled={locked} onchange={controller.setMode} />
        {#if $form.mode === "internet"}<p class="hint">Internet nutzt einen öffentlichen EU-Relay im Pilotbetrieb. Der Betreiber kann Verbindungsmetadaten verarbeiten. Beide Apps müssen gleichzeitig eine Sitzung gestartet haben.</p>{/if}
        <ConnectionCodeFields mode={$form.mode} ownCode={$form.ownCode} peerCode={$form.peerCode} copyFeedback={$form.copyFeedback} disabled={locked} busy={connection.busy} ongenerate={() => { void controller.generate(); }} oncopy={() => { void controller.copyCode(); }} oncode={controller.setPeerCode} onconnect={(role) => { void controller.connect(role); }} />
      {/if}
    {/if}
    <Disclosure label="Netzwerkdetails">
      {#if $form.mode === "lan"}
        <LanSettings addresses={connection.setup.addresses} address={$form.address} port={$form.port} disabled={locked} onaddress={controller.setAddress} onport={controller.setPort} onrefresh={() => { void controller.load(true); }} />
      {:else}
        <p class="hint">Internet · öffentlicher EU-Relay. Carnyx versucht automatisch eine direkte Verbindung. Ohne direkte Erreichbarkeit bleibt die Verbindung über den Relay verschlüsselt.</p>
        <p class="hint">Keine öffentliche Geräteadress-Veröffentlichung. Ein Relay-Wechsel erfordert neue Codes.</p>
      {/if}
      {#if connection.connection.phase === "connected"}<p role="status">{connectionPathLabel(connection.connection.path)}</p>{/if}
    </Disclosure>
    <Disclosure label="Weitere Informationen">
      <div class="info"><span>Geräte-ID</span><code>{connection.setup.peerId}</code></div>
      <div class="info"><span>Öffentlicher Schlüssel</span><code>{connection.setup.publicKey ?? "Wird beim Anzeigen eines Codes oder beim Verbinden angelegt."}</code></div>
      <p class="hint">Nachrichten werden lokal verschlüsselt gespeichert und nach 24 Stunden entfernt. Der Schlüssel liegt im OS-Schlüsselspeicher. Auch die Verbindung ist Ende-zu-Ende verschlüsselt.</p>
      <p class="hint">„Beim Gegenüber gespeichert“ bestätigt die Speicherung, nicht das Lesen.</p>
    </Disclosure>
  {/if}
</div>
<style>
  .connection-view { height: 100%; overflow-y: auto; padding: 18px 16px 24px; display: flex; flex-direction: column; gap: 14px; }
  h1 { margin: 0 0 3px; font-size: 17px; font-weight: 600; }
  h2 { margin: 0; font-size: 13px; font-weight: 600; }
  .intro p, .hint { color: var(--muted); font-size: 12px; }
  .step { display: grid; gap: 10px; padding: 14px; background: var(--surface); border-radius: 10px; }
  .info { display: grid; gap: 4px; font-size: 12px; }
  .info span { color: var(--muted); }
  code { overflow-wrap: anywhere; font-size: 11px; user-select: text; }
</style>
