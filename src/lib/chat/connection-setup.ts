import { get, writable } from "svelte/store";
import type { createConnectionStore } from "./connection-store";

export interface SetupFormState {
  address: string;
  port: string;
  peerCode: string;
  ownCode: string;
  loading: boolean;
  validation: string | null;
}

/** Form state survives tab switches. Generated codes are bound to one endpoint revision. */
export function createConnectionSetup(connection: ReturnType<typeof createConnectionStore>) {
  const state = writable<SetupFormState>({ address: "", port: "32147", peerCode: "", ownCode: "", loading: false, validation: null });
  let disposed = false;
  let revision = 0;
  let loading: Promise<void> | undefined;
  const unsubscribe = connection.state.subscribe((snapshot) => {
    if (disposed || !snapshot.setup) return;
    const addresses = snapshot.setup.addresses;
    const current = get(state);
    if (!addresses.includes(current.address)) {
      revision++;
      state.update((s) => ({ ...s, address: addresses[0] ?? "", ownCode: "" }));
    }
  });
  function endpoint() {
    const { address, port } = get(state);
    const addresses = get(connection.state).setup?.addresses ?? [];
    if (!address || !addresses.includes(address)) return { error: "Keine Netzwerkadresse verfügbar. Bitte die Netzwerkverbindung prüfen und erneut laden." };
    if (!/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65535) return { error: "Bitte einen Port zwischen 1 und 65535 eingeben." };
    return { address, port: Number(port), error: null };
  }
  function editable() {
    const snapshot = get(connection.state);
    return !disposed && !snapshot.busy && snapshot.connection.phase === "disconnected";
  }
  function changeEndpoint(patch: Partial<Pick<SetupFormState, "address" | "port">>) {
    if (!editable()) return;
    revision++;
    state.update((s) => ({ ...s, ...patch, ownCode: "", validation: null }));
  }
  return {
    state: { subscribe: state.subscribe },
    load(force = false): Promise<void> {
      if (disposed) return Promise.resolve();
      if (loading) return loading;
      if (!force && get(connection.state).setup) return Promise.resolve();
      state.update((s) => ({ ...s, loading: true }));
      loading = connection.setup().finally(() => {
        loading = undefined;
        if (!disposed) state.update((s) => ({ ...s, loading: false }));
      });
      return loading;
    },
    setAddress: (address: string) => changeEndpoint({ address }),
    setPort: (port: string) => changeEndpoint({ port }),
    setPeerCode(peerCode: string) {
      if (editable()) state.update((s) => ({ ...s, peerCode, validation: null }));
    },
    async generate() {
      if (!editable()) return;
      const value = endpoint();
      if (value.error) { state.update((s) => ({ ...s, validation: value.error })); return; }
      const requested = ++revision;
      state.update((s) => ({ ...s, ownCode: "", validation: null }));
      const code = await connection.code(value.address!, value.port!);
      if (!disposed && requested === revision && code !== null) state.update((s) => ({ ...s, ownCode: code }));
    },
    async connect(role: "initiator" | "responder") {
      if (!editable()) return;
      const value = endpoint();
      const peerCode = get(state).peerCode.trim();
      const validation = value.error || (!peerCode ? "Bitte den Verbindungscode des Gegenübers eingeben." : null);
      if (validation) { state.update((s) => ({ ...s, validation })); return; }
      state.update((s) => ({ ...s, validation: null }));
      await connection.connect(peerCode, role, value.address!, value.port!);
    },
    disconnect: connection.disconnect,
    dispose() { disposed = true; revision++; unsubscribe(); },
  };
}
export type ConnectionSetup = ReturnType<typeof createConnectionSetup>;
