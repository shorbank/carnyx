import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { requireTauri, tauriChatApi, type ChatApi } from "./chat-api";
import type { Message } from "./message";

export type ConnectionMode = "lan" | "internet";
export type ConnectionTarget = { mode: "lan"; address: string } | { mode: "internet"; endpointId: string; relayUrl: string };
export interface Invitation { peerId: string; publicKey: string; target: ConnectionTarget }
export type PeerRole = "initiator" | "responder";
export type SessionSettings = { mode: "lan"; localAddress: string; localPort: number; role: PeerRole } | { mode: "internet" };
export interface PeerSetup { peerId: string; publicKey: string | null; selectedPeer: Invitation | null; session: SessionSettings | null; addresses: string[] }
export interface PeerState { phase: "disconnected" | "listening" | "connecting" | "handshaking" | "connected" | "reconnecting" | "disconnecting"; peerId: string | null; revision: number; error: string | null; mode: ConnectionMode | null; path: "direct" | "relay" | "unknown" | null }
export interface PeerApi {
  setup(): Promise<PeerSetup>;
  state(): Promise<PeerState>;
  code(settings: SessionSettings): Promise<string>;
  connect(code: string, settings: SessionSettings): Promise<void>;
  reconnect(): Promise<void>;
  disconnect(): Promise<void>;
  subscribe(callback: (state: PeerState) => void): Promise<() => void>;
}
async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> { requireTauri(); return invoke<T>(name, args); }
export const tauriPeerApi: PeerApi = {
  setup: () => command("get_peer_setup"), state: () => command("get_peer_state"),
  code: (settings) => command("create_connection_code", { settings }),
  connect: (code, settings) => command("start_peer_session", { code, settings }),
  reconnect: () => command("reconnect_peer"),
  disconnect: () => command("disconnect_peer"),
  async subscribe(callback) { requireTauri(); return listen<PeerState>("peer_state_changed", ({ payload }) => callback(payload), { target: "main" }); },
};
export function peerChatApi(peerId: string): ChatApi {
  return {
    refreshAfterSend: true,
    listMessages: () => command<Message[]>("list_peer_messages", { peerId }),
    sendMessage: (text) => command<Message>("send_peer_message", { peerId, text }),
    subscribeExpired: tauriChatApi.subscribeExpired,
    subscribeChanged: tauriChatApi.subscribeChanged,
  };
}
