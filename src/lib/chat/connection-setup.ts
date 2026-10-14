import { derived, get, writable } from "svelte/store";
import type { createConnectionStore } from "./connection-store";
import type { ClipboardApi } from "./clipboard-api";
import type { PeerRole } from "./peer-api";

export interface SetupFormState {
  address: string;
  port: string;
  peerCode: string;
  ownCode: string;
  validation: string | null;
  editing: boolean;
  copyFeedback: string;
}

/** Owns form edits and endpoint-bound codes; all network lifecycle stays in the connection store. */
export function createConnectionSetup(connection: ReturnType<typeof createConnectionStore>, clipboard?: ClipboardApi) {
  const form = writable<SetupFormState>({ address: "", port: "32147", peerCode: "", ownCode: "", validation: null, editing: false, copyFeedback: "" });
  let disposed = false;
  let revision = 0;
  let hydrated = false;
  let dirty = false;
  let currentEndpoint = "";
  const unsubscribe = connection.state.subscribe((snapshot) => {
    if (disposed || !snapshot.setup) return;
    const { session, selectedPeer, addresses } = snapshot.setup;
    const persistedEndpoint = session ? `${selectedPeer?.peerId}:${session.localAddress}:${session.localPort}` : "";
    if (!hydrated || (!dirty && persistedEndpoint !== currentEndpoint)) {
      hydrated = true;
      currentEndpoint = persistedEndpoint;
      revision++;
      form.update((s) => ({ ...s, address: session?.localAddress ?? addresses[0] ?? "", port: String(session?.localPort ?? 32147), ownCode: "", copyFeedback: "" }));
    }
    // Only a fresh, unsaved setup may follow discovered addresses automatically.
    const current = get(form);
    if (!session && !dirty && !addresses.includes(current.address)) {
      revision++;
      form.update((s) => ({ ...s, address: addresses[0] ?? "", ownCode: "", copyFeedback: "" }));
    }
    if (get(form).address && !addresses.includes(get(form).address)) {
      revision++;
      form.update((s) => ({ ...s, ownCode: "", copyFeedback: "" }));
    }
  });
  const state = derived([form, connection.state], ([$form, snapshot]) => {
    const { setup, setupStatus, busy } = snapshot;
    const saved = setup?.session;
    let reconnectHint = "";
    if (!setup?.selectedPeer) reconnectHint = "Bitte zuerst ein Gegenüber einrichten.";
    else if (!saved) reconnectHint = "Bitte die Einrichtung einmal vervollständigen.";
    else if (!setup.addresses.includes(saved.localAddress)) reconnectHint = "Die gespeicherte Netzwerkadresse ist nicht verfügbar. Bitte die Einrichtung aktualisieren und neue Codes austauschen.";
    else if ($form.address !== saved.localAddress || $form.port !== String(saved.localPort)) reconnectHint = "Netzwerkdetails wurden geändert. Bitte neue Codes austauschen.";
    const disconnected = snapshot.connection.phase === "disconnected";
    return { ...$form, loading: setupStatus === "loading" && !setup, canReconnect: disconnected && !busy && !!setup && setupStatus !== "error" && !reconnectHint,
      reconnectHint, showEditor: $form.editing || !setup?.selectedPeer || !saved,
      roleLabel: !saved ? "Rolle noch nicht gespeichert" : saved.role === "responder" ? "Wartet auf Gegenüber" : "Verbindet zum Gegenüber" };
  });
  function endpoint() {
    const { address, port } = get(form);
    const addresses = get(connection.state).setup?.addresses ?? [];
    if (!addresses.length) return { error: "Keine Netzwerkadresse verfügbar. Bitte die Netzwerkverbindung prüfen und erneut laden." };
    if (!address || !addresses.includes(address)) return { error: "Bitte eine verfügbare Netzwerkadresse wählen und die Verbindungscodes aktualisieren." };
    if (!/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65535) return { error: "Bitte einen Port zwischen 1 und 65535 eingeben." };
    return { address, port: Number(port), error: null };
  }
  function editable() {
    const snapshot = get(connection.state);
    return !disposed && !snapshot.busy && (snapshot.setupStatus !== "loading" || !!snapshot.setup) && snapshot.connection.phase === "disconnected";
  }
  function changeEndpoint(patch: Partial<Pick<SetupFormState, "address" | "port">>) {
    if (!editable()) return;
    dirty = true;
    revision++;
    form.update((s) => ({ ...s, ...patch, ownCode: "", copyFeedback: "", validation: null }));
  }
  return {
    state,
    load(force = false): Promise<void> {
      if (disposed) return Promise.resolve();
      if (!force && get(connection.state).setupStatus === "ready") return Promise.resolve();
      return connection.setup(force);
    },
    edit() { if (editable()) form.update((s) => ({ ...s, editing: true, validation: null })); },
    setAddress: (address: string) => changeEndpoint({ address }),
    setPort: (port: string) => changeEndpoint({ port }),
    setPeerCode(peerCode: string) {
      if (editable()) form.update((s) => ({ ...s, peerCode, validation: null }));
    },
    async generate() {
      if (!editable()) return;
      const value = endpoint();
      if (value.error) { form.update((s) => ({ ...s, validation: value.error })); return; }
      const requested = ++revision;
      form.update((s) => ({ ...s, ownCode: "", copyFeedback: "", validation: null }));
      const code = await connection.code(value.address!, value.port!);
      if (!disposed && requested === revision && code !== null) form.update((s) => ({ ...s, ownCode: code }));
    },
    async copyCode() {
      const code = get(form).ownCode;
      if (disposed || !code) return;
      const requested = revision;
      let copyFeedback: string;
      try {
        if (!clipboard) throw new Error("Clipboard unavailable");
        await clipboard.writeText(code);
        copyFeedback = "Code kopiert.";
      } catch { copyFeedback = "Automatisches Kopieren ist nicht verfügbar. Bitte den Code markieren und kopieren."; }
      if (!disposed && requested === revision && get(form).ownCode === code) form.update((s) => ({ ...s, copyFeedback }));
    },
    async connect(role: PeerRole) {
      if (!editable()) return;
      const value = endpoint();
      const peerCode = get(form).peerCode.trim();
      const validation = value.error || (!peerCode ? "Bitte den Verbindungscode des Gegenübers eingeben." : null);
      if (validation) { form.update((s) => ({ ...s, validation })); return; }
      form.update((s) => ({ ...s, validation: null }));
      const connected = await connection.connect(peerCode, role, value.address!, value.port!);
      if (!disposed && connected) {
        dirty = false;
        form.update((s) => ({ ...s, editing: false }));
      }
    },
    async reconnect() {
      if (disposed || !get(state).canReconnect) return;
      form.update((s) => ({ ...s, validation: null }));
      await connection.reconnect();
    },
    disconnect: connection.disconnect,
    dispose() { disposed = true; revision++; unsubscribe(); },
  };
}
export type ConnectionSetup = ReturnType<typeof createConnectionSetup>;
