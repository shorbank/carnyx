import { readonly, writable } from "svelte/store";

import type { StorageApi } from "../adapters/storage-api.ts";
import type { StorageStatus } from "../models/storage.ts";

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
