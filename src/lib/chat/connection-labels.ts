import type { PeerState } from "./peer-api";
export const connectionLabels: Record<PeerState["phase"], string> = {
  disconnected: "Nicht verbunden", listening: "Wartet auf Gegenüber", connecting: "Verbindet …",
  handshaking: "Prüft Gegenüber …", connected: "Verschlüsselt verbunden",
  reconnecting: "Verbindet erneut …", disconnecting: "Trennt …",
};
