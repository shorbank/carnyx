import { get, writable } from "svelte/store";
import type { ChatApi } from "../adapters/chat-api.ts";
import type { ClipboardApi } from "../adapters/clipboard-api.ts";
import type { PeerApi } from "../adapters/peer-api.ts";
import { createChatStore, type ChatStore } from "../stores/chat-store.ts";
import { createComposerDraft } from "./composer-draft.ts";
import { createConnectionStore, type ConnectionState } from "../stores/connection-store.ts";
import { createConnectionSetup } from "./connection-setup.ts";

export type WorkspaceView = "local" | "peer" | "connection";
export interface Conversation {
  id: string;
  chat: ChatStore;
  draft: ReturnType<typeof createComposerDraft>;
}
export interface WorkspaceState {
  view: WorkspaceView;
  local: Conversation;
  peer: Conversation | null;
  connection: ConnectionState;
  peerCanSend: boolean;
}
export interface WorkspaceApis {
  local: ChatApi;
  peer: (id: string) => ChatApi;
  connection: PeerApi;
  clipboard?: ClipboardApi;
}

/** Owns conversation lifetimes; view components never open network sessions. */
export function createChatWorkspace(apis: WorkspaceApis) {
  const connection = createConnectionStore(apis.connection);
  const setup = createConnectionSetup(connection, apis.clipboard);
  const conversations = new Map<string, Conversation>();
  const started = new Set<string>();
  let disposed = false;
  let mounted = false;
  let view: WorkspaceView = "local";

  function conversation(id: string): Conversation {
    const existing = conversations.get(id);
    if (existing) return existing;
    const chat = createChatStore(id === "local" ? apis.local : apis.peer(id));
    const draft = createComposerDraft(async (text) => {
      const peer = get(connection.state).connection;
      if (disposed || (id !== "local" && (peer.phase !== "connected" || peer.peerId !== id))) return false;
      return chat.sendMessage(text);
    });
    const value = { id, chat, draft };
    conversations.set(id, value);
    return value;
  }
  const local = conversation("local");
  const state = writable<WorkspaceState>({ view, local, peer: null, connection: get(connection.state), peerCanSend: false });
  function ensureStarted(value: Conversation) {
    if (mounted && !started.has(value.id)) {
      started.add(value.id);
      void value.chat.start();
    }
  }
  function update(snapshot: ConnectionState) {
    if (disposed) return;
    const id = snapshot.setup?.selectedPeer?.peerId;
    const peer = id ? conversation(id) : null;
    state.set({ view, local, peer, connection: snapshot,
      peerCanSend: !!peer && snapshot.connection.phase === "connected" && snapshot.connection.peerId === peer.id });
    if (view === "local") ensureStarted(local);
    if (view === "peer" && peer) ensureStarted(peer);
  }
  const unsubscribe = connection.state.subscribe(update);
  return {
    state: { subscribe: state.subscribe },
    setup,
    async start() {
      if (disposed || mounted) return;
      mounted = true;
      update(get(connection.state));
      await connection.start();
    },
    select(next: WorkspaceView) {
      if (disposed) return;
      view = next;
      update(get(connection.state));
      if (next === "connection") void setup.load();
    },
    async refresh() {
      if (disposed) return;
      const current = get(state);
      const active = current.view === "local" ? current.local : current.view === "peer" ? current.peer : null;
      await Promise.all([connection.refresh(), active?.chat.refresh()]);
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      unsubscribe();
      setup.dispose();
      connection.dispose();
      for (const value of conversations.values()) value.chat.dispose();
    },
  };
}
export type ChatWorkspace = ReturnType<typeof createChatWorkspace>;
