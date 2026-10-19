import type { StorageFailure } from "../models/storage.ts";

export function storageErrorText(code: StorageFailure | null): string {
  switch (code) {
    case "key_unavailable": return "Der OS-Schlüsselspeicher ist nicht verfügbar. Entsperre ihn und versuche es erneut.";
    case "key_missing": return "Der Schlüssel dieser Historie fehlt im OS-Schlüsselspeicher. Ohne den ursprünglichen Schlüssel lässt sie sich nicht öffnen.";
    case "unlock_failed": return "Die Historie konnte nicht entschlüsselt oder geprüft werden. Der Schlüssel passt nicht oder die Datei ist beschädigt.";
    case "cipher_unavailable": return "Die benötigte Datenbankverschlüsselung fehlt in diesem App-Build.";
    case "migration_failed": return "Die Verschlüsselung der bisherigen Historie konnte nicht abgeschlossen werden. Prüfe freien Speicherplatz und Schreibrechte und versuche es erneut.";
    case "cleanup_failed": return "Die verschlüsselte Kopie wurde geprüft. Die alte Klartextdatei konnte noch nicht vollständig entfernt werden. Prüfe Schreibrechte und versuche es erneut.";
    case "ipc_unavailable": return "Der Speicherstatus ist momentan nicht erreichbar. Versuche es erneut.";
    default: return "Die Historie ist momentan nicht verfügbar. Prüfe Speicherplatz und Schreibrechte und versuche es erneut.";
  }
}
