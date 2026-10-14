import { writable, get } from "svelte/store";
import type { PeerApi, PeerSetup, PeerState, PeerRole } from "./peer-api";
import { peerError } from "./peer-errors.ts";

export interface ConnectionState {
  connection: PeerState;
  setup: PeerSetup | null;
  setupStatus: "idle" | "loading" | "ready" | "error";
  setupError: string | null;
  busy: boolean;
  error: string | null;
}

export function createConnectionStore(api: PeerApi) {
  const state = writable<ConnectionState>({
    connection: { phase: "disconnected", peerId: null, revision: 0, error: null },
    setup: null, setupStatus: "idle", setupError: null, busy: false, error: null,
  });
  let disposed = false;
  let unlisten: (() => void) | undefined;
  let starting: Promise<void> | undefined;
  let loadingSetup: Promise<void> | undefined;
  let setupRequest = 0;

  function apply(connection: PeerState) {
    if (!disposed) state.update((current) => connection.revision >= current.connection.revision ? { ...current, connection } : current);
  }
  async function readState() {
    try { apply(await api.state()); }
    catch (error) { if (!disposed) state.update((s) => ({ ...s, error: s.error ?? peerError(error) })); }
  }
  function setup(force = false): Promise<void> {
    if (disposed) return Promise.resolve();
    if (loadingSetup && !force) return loadingSetup;
    const revision = ++setupRequest;
    state.update((s) => ({ ...s, setupStatus: "loading", setupError: null }));
    const pending = (async () => {
      try {
        const value = await api.setup();
        if (!disposed && revision === setupRequest) state.update((s) => ({ ...s, setup: value, setupStatus: "ready", setupError: null }));
      } catch (error) {
        if (!disposed && revision === setupRequest) state.update((s) => ({ ...s, setupStatus: "error", setupError: peerError(error) }));
      }
    })().finally(() => { if (revision === setupRequest) loadingSetup = undefined; });
    loadingSetup = pending;
    return pending;
  }
  async function action<T>(operation: () => Promise<T>): Promise<T | null> {
    if (disposed || get(state).busy) return null;
    // Reads begun before a mutation must not restore the previous peer afterwards.
    setupRequest++;
    loadingSetup = undefined;
    state.update((s) => ({ ...s, busy: true, error: null }));
    try { const result = await operation(); return disposed ? null : result; }
    catch (error) { if (!disposed) state.update((s) => ({ ...s, error: peerError(error) })); return null; }
    finally {
      if (!disposed) {
        await Promise.all([readState(), setup(true)]);
        if (!disposed) state.update((s) => ({ ...s, busy: false }));
      }
    }
  }
  function start(): Promise<void> {
    if (disposed) return Promise.resolve();
    if (starting) return starting;
    starting = (async () => {
      if (!unlisten) {
        try {
          const stop = await api.subscribe(apply);
          if (disposed) { stop(); return; }
          unlisten = stop;
          state.update((s) => ({ ...s, error: null }));
        } catch (error) {
          if (!disposed) state.update((s) => ({ ...s, error: peerError(error) }));
        }
      }
      if (!disposed) await Promise.all([readState(), setup()]);
    })().finally(() => { starting = undefined; });
    return starting;
  }
  return {
    state: { subscribe: state.subscribe }, setup, start,
    refresh: () => unlisten ? Promise.all([readState(), setup()]).then(() => {}) : start(),
    code: (address: string, port: number) => action(() => api.code(address, port)),
    connect: (code: string, role: PeerRole, address: string, port: number) => action(async () => { await api.connect(code, role, address, port); return true; }),
    reconnect: () => action(async () => { await api.reconnect(); return true; }),
    disconnect: () => action(() => api.disconnect()),
    dispose() { disposed = true; setupRequest++; unlisten?.(); unlisten = undefined; },
  };
}
