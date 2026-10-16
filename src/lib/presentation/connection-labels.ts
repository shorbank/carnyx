import type { PeerState } from "../models/connection.ts";
export const connectionLabels: Record<PeerState["phase"], string> = {
  disconnected: "Nicht verbunden", listening: "Wartet auf Gegenüber", connecting: "Verbindet …",
  handshaking: "Prüft Gegenüber …", connected: "Verschlüsselt verbunden",
  reconnecting: "Verbindet erneut …", disconnecting: "Trennt …",
};

export function connectionPathLabel(path: PeerState["path"]): string {
  return path === "direct" ? "Direkt verbunden" : path === "relay" ? "Über Relay verbunden" : "Verbindungspfad wird ermittelt …";
}
