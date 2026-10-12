import { get, writable, type Readable } from "svelte/store";
import type { ChatApi } from "./chat-api";
import type { Message } from "./message";
import { messageLocalTime } from "./message.ts";
import { peerError } from "./peer-errors.ts";

export interface ChatIssue {
  operation: "load" | "send" | "connection";
  message: string;
}

export interface ChatState {
  messages: readonly Message[];
  loaded: boolean;
  loading: boolean;
  sending: boolean;
  error: ChatIssue | null;
}

export interface ChatStore {
  state: Readable<ChatState>;
  start(): Promise<void>;
  refresh(): Promise<void>;
  sendMessage(text: string): Promise<boolean>;
  dispose(): void;
}

function issue(error: unknown, operation: ChatIssue["operation"]): ChatIssue {
  const code = error && typeof error === "object" && "code" in error ? error.code : "internal_error";
  let message = operation === "send"
    ? "Nachricht konnte nicht gespeichert werden. Bitte erneut senden."
    : "Nachrichten konnten nicht geladen werden. Bitte erneut versuchen.";
  if (code === "empty_text") message = "Bitte eine Nachricht eingeben.";
  if (code === "invalid_time") message = "Die Systemzeit konnte nicht gelesen werden.";
  if (code === "not_available") message = "Der Chat ist nur in der Carnyx-Desktop-App verfügbar.";
  if (["not_connected", "message_too_large", "protocol_error"].includes(String(code))) message = peerError(error);
  return { operation, message };
}

export function createChatStore(api: ChatApi): ChatStore {
  const state = writable<ChatState>({ messages: [], loaded: false, loading: true, sending: false, error: null });
  let disposed = false;
  let revision = 0;
  let unlisten: (() => void) | undefined;
  let unlistenChanged: (() => void) | undefined;
  let starting: Promise<void> | undefined;
  let loading: Promise<void> | undefined;
  let refreshRequested = false;
  const expiredDuringSend = new Set<string>();

  function load(): Promise<void> {
    if (disposed) return Promise.resolve();
    refreshRequested = true;
    if (loading) return loading;
    loading = (async () => {
      state.update((current) => ({ ...current, loading: true }));
      while (refreshRequested && !disposed) {
        refreshRequested = false;
        const requestedRevision = revision;
        try {
          const messages = await api.listMessages();
          if (disposed) break;
          if (requestedRevision !== revision) {
            refreshRequested = true;
            continue;
          }
          state.update((current) => ({ ...current, messages, loaded: true,
            error: current.error?.operation === "send" ? current.error : null }));
        } catch (error) {
          if (disposed) break;
          if (requestedRevision !== revision) { refreshRequested = true; continue; }
          state.update((current) => ({ ...current, error: issue(error, "load") }));
        }
      }
    })().finally(() => {
      loading = undefined;
      if (!disposed) state.update((current) => ({ ...current, loading: false }));
    });
    return loading;
  }

  function expired(ids: readonly string[]) {
    if (disposed || !ids.length) return;
    const removed = new Set(ids);
    if (get(state).sending) for (const id of ids) expiredDuringSend.add(id);
    revision++;
    state.update((current) => ({ ...current, messages: current.messages.filter((message) => !removed.has(message.id)) }));
    // A snapshot already in flight might still contain the deleted rows.
    if (loading) refreshRequested = true;
  }

  function start(): Promise<void> {
    if (disposed) return Promise.resolve();
    if (starting) return starting;
    if (unlisten) return load();
    starting = (async () => {
      state.update((current) => ({ ...current, loading: true, error: null }));
      try {
        const unsubscribe = await api.subscribeExpired(expired);
        if (disposed) { unsubscribe(); return; }
        unlisten = unsubscribe;
        if (api.subscribeChanged) {
          const unsubscribeChanged = await api.subscribeChanged(() => { revision++; void load(); });
          if (disposed) { unsubscribeChanged(); return; }
          unlistenChanged = unsubscribeChanged;
        }
        await load();
      } catch (error) {
        unlisten?.(); unlistenChanged?.(); unlisten = undefined; unlistenChanged = undefined;
        if (!disposed) state.update((current) => ({ ...current, error: issue(error, "connection"), loading: false }));
      }
    })().finally(() => { starting = undefined; });
    return starting;
  }

  return {
    state: { subscribe: state.subscribe },
    start,
    refresh: () => unlisten ? load() : start(),
    async sendMessage(text) {
      const current = get(state);
      if (disposed || !current.loaded || current.sending || !text.trim()) return false;
      expiredDuringSend.clear();
      state.update((value) => ({ ...value, sending: true,
        error: value.error?.operation === "send" ? null : value.error }));
      try {
        const message = await api.sendMessage(text);
        if (disposed) return false;
        // A save response may be delivered after a long standby and expiry.
        if (expiredDuringSend.has(message.id)) return true;
        revision++;
        if (api.refreshAfterSend) { void load(); return true; }
        state.update((value) => ({ ...value,
          messages: value.messages.some((existing) => existing.id === message.id) ? value.messages :
            [...value.messages, message].sort((a, b) => messageLocalTime(a) - messageLocalTime(b)) }));
        return true;
      } catch (error) {
        if (!disposed) state.update((value) => ({ ...value, error: issue(error, "send") }));
        return false;
      } finally {
        expiredDuringSend.clear();
        if (!disposed) state.update((value) => ({ ...value, sending: false }));
      }
    },
    dispose() {
      disposed = true;
      expiredDuringSend.clear();
      unlisten?.();
      unlisten = undefined;
      unlistenChanged?.(); unlistenChanged = undefined;
    },
  };
}
