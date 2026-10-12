<script lang="ts">
  import type { PeerState } from "./peer-api";
  import { peerError } from "./peer-errors";
  let { connection }: { connection: PeerState } = $props();
  const labels: Record<PeerState["phase"], string> = {
    disconnected: "Nicht verbunden", listening: "Wartet auf Peer", connecting: "Verbindet …",
    handshaking: "Prüft Gegenüber …", connected: "Verschlüsselt verbunden", reconnecting: "Verbindung wird wiederhergestellt …", disconnecting: "Trennt …",
  };
</script>
<p role="status">{labels[connection.phase]}{#if connection.error}<span> · {peerError(connection.error)}</span>{/if}</p>
<style>p { margin: 0; padding: 4px 12px; font-size: 0.75rem; } span { display: block; }</style>
