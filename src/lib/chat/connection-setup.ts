import { derived, get, writable } from "svelte/store";
import type { createConnectionStore } from "./connection-store";
import type { ClipboardApi } from "./clipboard-api";
import type { ConnectionMode, PeerRole, SessionSettings } from "./peer-api";

export interface SetupFormState {
  mode: ConnectionMode;
  address: string;
  port: string;
  peerCode: string;
  ownCode: string;
  validation: string | null;
  editing: boolean;
  copyFeedback: string;
}
/** Form and draft ownership only. Network lifetime belongs to the connection store/backend. */
export function createConnectionSetup(connection: ReturnType<typeof createConnectionStore>, clipboard?: ClipboardApi) {
  const form = writable<SetupFormState>({ mode: "internet", address: "", port: "32147", peerCode: "", ownCode: "", validation: null, editing: false, copyFeedback: "" });
  const codes: Record<ConnectionMode, string> = { lan: "", internet: "" };
  let disposed = false, revision = 0, hydrated = false, dirty = false, lanDirty = false;
  let currentProfile = "";
  const unsubscribe = connection.state.subscribe((snapshot) => {
    if (disposed || !snapshot.setup) return;
    const { session, selectedPeer, addresses } = snapshot.setup;
    const persistedProfile = JSON.stringify([selectedPeer?.peerId, session]);
    if (!hydrated || (!dirty && persistedProfile !== currentProfile)) {
      const initial = !hydrated;
      hydrated = true; currentProfile = persistedProfile; revision++;
      const mode = session?.mode ?? selectedPeer?.target.mode ?? "internet";
      form.update((s) => ({ ...s, mode, address: session?.mode === "lan" ? session.localAddress : initial ? addresses[0] ?? "" : s.address, port: String(session?.mode === "lan" ? session.localPort : s.port), peerCode: codes[mode], ownCode: "", copyFeedback: "" }));
    }
    const current = get(form);
    if (session?.mode !== "lan" && !lanDirty && !addresses.includes(current.address)) {
      if (current.mode === "lan") revision++;
      form.update((s) => ({ ...s, address: addresses[0] ?? "", ...(s.mode === "lan" ? { ownCode: "", copyFeedback: "" } : {}) }));
    }
    if (get(form).mode === "lan" && get(form).address && !addresses.includes(get(form).address)) {
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
    else if ($form.mode !== saved.mode) reconnectHint = "Der Verbindungsmodus wurde geändert. Bitte neue Codes austauschen.";
    else if (saved.mode === "lan" && !setup.addresses.includes(saved.localAddress)) reconnectHint = "Die gespeicherte Netzwerkadresse ist nicht verfügbar. Bitte die Einrichtung aktualisieren und neue Codes austauschen.";
    else if (saved.mode === "lan" && ($form.address !== saved.localAddress || $form.port !== String(saved.localPort))) reconnectHint = "Netzwerkdetails wurden geändert. Bitte neue Codes austauschen.";
    return { ...$form, loading: setupStatus === "loading" && !setup,
      canReconnect: snapshot.connection.phase === "disconnected" && !busy && !!setup && setupStatus !== "error" && !reconnectHint,
      reconnectHint, showEditor: $form.editing || !setup?.selectedPeer || !saved,
      roleLabel: !saved ? "Verbindung noch nicht gespeichert" : saved.mode === "internet" ? "Internet · beide starten die Verbindung" : saved.role === "responder" ? "Wartet auf Gegenüber" : "Verbindet zum Gegenüber" };
  });
  function settings(role: PeerRole = "initiator"): { value?: SessionSettings; error?: string } {
    const { mode, address, port } = get(form);
    if (mode === "internet") return { value: { mode } };
    const addresses = get(connection.state).setup?.addresses ?? [];
    if (!addresses.length) return { error: "Keine Netzwerkadresse verfügbar. Bitte die Netzwerkverbindung prüfen und erneut laden." };
    if (!address || !addresses.includes(address)) return { error: "Bitte eine verfügbare Netzwerkadresse wählen und die Verbindungscodes aktualisieren." };
    if (!/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65535) return { error: "Bitte einen Port zwischen 1 und 65535 eingeben." };
    return { value: { mode, localAddress: address, localPort: Number(port), role } };
  }
  function editable() {
    const s = get(connection.state);
    return !disposed && !s.busy && (s.setupStatus !== "loading" || !!s.setup) && s.connection.phase === "disconnected";
  }
  function changeEndpoint(patch: Partial<Pick<SetupFormState, "address" | "port">>) {
    if (!editable()) return;
    dirty = true; lanDirty = true; revision++;
    form.update((s) => ({ ...s, ...patch, ownCode: "", copyFeedback: "", validation: null }));
  }
  return {
    state,
    load(force = false): Promise<void> {
      if (disposed || (!force && get(connection.state).setupStatus === "ready")) return Promise.resolve();
      return connection.setup(force);
    },
    edit() { if (editable()) form.update((s) => ({ ...s, editing: true, validation: null })); },
    setMode(mode: ConnectionMode) {
      if (!editable() || get(form).mode === mode) return;
      dirty = true; revision++;
      const current = get(form); codes[current.mode] = current.peerCode;
      form.update((s) => ({ ...s, mode, peerCode: codes[mode], ownCode: "", validation: null, copyFeedback: "" }));
    },
    setAddress: (address: string) => changeEndpoint({ address }),
    setPort: (port: string) => changeEndpoint({ port }),
    setPeerCode(peerCode: string) { if (editable()) { codes[get(form).mode] = peerCode; form.update((s) => ({ ...s, peerCode, validation: null })); } },
    async generate() {
      if (!editable()) return;
      const value = settings();
      if (value.error) { form.update((s) => ({ ...s, validation: value.error! })); return; }
      const requested = ++revision;
      form.update((s) => ({ ...s, ownCode: "", copyFeedback: "", validation: null }));
      const code = await connection.code(value.value!);
      if (!disposed && requested === revision && code !== null) form.update((s) => ({ ...s, ownCode: code }));
    },
    async copyCode() {
      const code = get(form).ownCode;
      if (disposed || !code) return;
      const requested = revision;
      let copyFeedback: string;
      try { if (!clipboard) throw new Error("Clipboard unavailable"); await clipboard.writeText(code); copyFeedback = "Code kopiert."; }
      catch { copyFeedback = "Automatisches Kopieren ist nicht verfügbar. Bitte den Code markieren und kopieren."; }
      if (!disposed && requested === revision && get(form).ownCode === code) form.update((s) => ({ ...s, copyFeedback }));
    },
    async connect(role: PeerRole = "initiator") {
      if (!editable()) return;
      const value = settings(role), peerCode = get(form).peerCode.trim();
      const validation = value.error || (!peerCode ? "Bitte den Verbindungscode des Gegenübers eingeben." : null);
      if (validation) { form.update((s) => ({ ...s, validation })); return; }
      form.update((s) => ({ ...s, validation: null }));
      const connected = await connection.connect(peerCode, value.value!);
      if (!disposed && connected) { dirty = false; form.update((s) => ({ ...s, editing: false })); }
    },
    async reconnect() {
      if (disposed || !get(state).canReconnect) return;
      form.update((s) => ({ ...s, validation: null })); await connection.reconnect();
    },
    disconnect: connection.disconnect,
    dispose() { disposed = true; revision++; unsubscribe(); },
  };
}
export type ConnectionSetup = ReturnType<typeof createConnectionSetup>;
