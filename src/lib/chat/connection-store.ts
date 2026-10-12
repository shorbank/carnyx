import { writable, get } from "svelte/store";
import type { PeerApi, PeerSetup, PeerState } from "./peer-api";
import { peerError } from "./peer-errors.ts";

export function createConnectionStore(api: PeerApi) {
  const state = writable<{ connection: PeerState; setup: PeerSetup | null; busy: boolean; error: string | null; code: string }>({
    connection: { phase: "disconnected", peerId: null, revision: 0, error: null }, setup: null, busy: false, error: null, code: "",
  });
  let disposed = false;
  let unlisten: (() => void) | undefined;
  let starting: Promise<void> | undefined;
  let setupRequest = 0;
  function apply(connection: PeerState) {
    if (!disposed) state.update((current) => connection.revision >= current.connection.revision ? { ...current, connection } : current);
  }
  async function readState() { try { apply(await api.state()); } catch (error) { if (!disposed) state.update((s) => ({ ...s, error: peerError(error) })); } }
  function refresh(): Promise<void> { return unlisten ? readState() : start(); }
  async function setup() {
    const revision = ++setupRequest;
    try { const value = await api.setup(); if (!disposed && revision === setupRequest) state.update((s) => ({ ...s, setup: value, error: null })); }
    catch (error) { if (!disposed && revision === setupRequest) state.update((s) => ({ ...s, error: peerError(error) })); }
  }
  async function action(operation: () => Promise<void>) {
    if (disposed || get(state).busy) return;
    state.update((s) => ({ ...s, busy: true, error: null }));
    try { await operation(); }
    catch (error) { if (!disposed) state.update((s) => ({ ...s, error: peerError(error) })); }
    finally { if (!disposed) { await refresh(); state.update((s) => ({ ...s, busy: false })); } }
  }
  function start(): Promise<void> {
      if (disposed) return Promise.resolve();
      if (starting) return starting;
      if (unlisten) return readState();
      starting = (async () => { try { const stop = await api.subscribe(apply); if (disposed) { stop(); return; } unlisten = stop; await readState(); }
        catch (error) { if (!disposed) state.update((s) => ({ ...s, error: peerError(error) })); } })().finally(() => { starting = undefined; });
      return starting;
  }
  return {
    state: { subscribe: state.subscribe }, refresh, setup, start,
    code: (address: string, port: number) => action(async () => { const code = await api.code(address, port); if (!disposed) state.update((s) => ({ ...s, code })); }),
    connect: (code: string, role: "initiator" | "responder", address: string, port: number) => action(async () => { await api.connect(code, role, address, port); await setup(); }),
    disconnect: () => action(() => api.disconnect()),
    dispose() { disposed = true; setupRequest++; unlisten?.(); unlisten = undefined; },
  };
}
