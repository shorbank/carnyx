const labels: Record<string, string> = {
  peer_missing: "Noch kein Gegenüber gespeichert. Bitte zuerst eine Verbindung einrichten.",
  session_missing: "Bitte die Verbindung einmal vervollständigen, um sie wiederverwenden zu können.",
  invalid_code: "Der Verbindungscode ist ungültig.",
  invalid_address: "Bitte eine lokale IPv4-Adresse und einen gültigen Port wählen.",
  key_unavailable: "Der sichere Schlüsselspeicher ist nicht verfügbar. Bitte Zugriff prüfen; der lokale Chat bleibt verfügbar.",
  peer_key_changed: "Der Schlüssel dieses Peers hat sich geändert. Verbindung abgelehnt.",
  authentication_failed: "Das Gegenüber konnte nicht verifiziert werden. Bitte beide Verbindungscodes prüfen.",
  protocol_error: "Die Peer-Verbindung wurde wegen eines ungültigen Protokolls beendet.",
  network_error: "Die Verbindung ist nicht erreichbar. Bitte Adresse, Port und Netzwerkzugriff prüfen.",
  timeout: "Das Gegenüber antwortet nicht rechtzeitig. Bitte Netzwerkdetails prüfen oder die Verbindungscodes aktualisieren.",
  session_active: "Bitte die aktuelle Verbindung zuerst trennen.",
  not_connected: "Zum Senden muss dieser Peer verbunden sein.",
  message_too_large: "Die Nachricht ist für den Peer-Versand zu groß (maximal 60 KiB einschließlich Nachrichtenformat).",
  storage_error: "Die Nachricht konnte nicht gespeichert werden.",
  not_available: "Peer-Chat ist nur in der Carnyx-Desktop-App verfügbar.",
  unsupported_platform: "Peer-Chat unterstützt derzeit Windows und macOS.",
};
export function peerError(error: unknown): string {
  const code = typeof error === "string" ? error : error && typeof error === "object" && "code" in error ? String(error.code) : "";
  return labels[code] ?? "Die Peer-Aktion konnte nicht abgeschlossen werden. Bitte erneut versuchen.";
}
