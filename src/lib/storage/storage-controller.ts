import { readonly, writable } from "svelte/store";

export type StorageFailure = "key_unavailable" | "key_missing" | "unlock_failed" | "cipher_unavailable" | "migration_failed" | "cleanup_failed" | "storage_unavailable" | "ipc_unavailable";
export interface StorageStatus {
  phase: "initializing" | "ready" | "blocked";
  code: StorageFailure | null;
  revision: number;
}
export interface StorageApi {
  status(): Promise<StorageStatus>;
  retry(): Promise<void>;
  subscribe(callback: (status: StorageStatus) => void): Promise<() => void>;
}
export function createStorageController(api: StorageApi) {
  const state = writable<StorageStatus>({ phase: "initializing", code: null, revision: -1 });
  let revision = -1;
  let disposed = false;
  let unsubscribe: (() => void) | undefined;
  let starting: Promise<void> | undefined;
  let retrying = false;
  let ready = false;
  const accept = (status: StorageStatus) => {
    if (disposed || status.revision < revision || ready) return;
    revision = status.revision;
    ready = status.phase === "ready";
    state.set(status);
  };
  const fail = () => {
    if (!disposed && !ready) state.set({ phase: "blocked", code: "ipc_unavailable", revision });
  };
  const start = () => {
    if (disposed) return Promise.resolve();
    starting ??= (async () => {
      try {
        const stop = await api.subscribe(accept);
        if (disposed) { stop(); return; }
        unsubscribe = stop;
        accept(await api.status());
      } catch { fail(); }
    })();
    return starting;
  };
  return {
    state: readonly(state),
    start,
    async retry() {
      if (disposed || retrying || ready) return;
      retrying = true;
      try {
        await start();
        if (disposed || ready) return;
        if (!unsubscribe) { starting = undefined; await start(); }
        if (disposed || ready || !unsubscribe) return;
        await api.retry();
        if (!disposed) accept(await api.status());
      } catch { fail(); }
      finally { retrying = false; }
    },
    dispose() { disposed = true; unsubscribe?.(); unsubscribe = undefined; },
  };
}

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
