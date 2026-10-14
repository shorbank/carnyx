import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { requireTauri, tauriChatApi, type ChatApi } from "./chat-api";
import type { Message } from "./message";

export interface Invitation { version: number; peerId: string; publicKey: string; address: string }
export type PeerRole = "initiator" | "responder";
export interface SessionSettings { localAddress: string; localPort: number; role: PeerRole }
export interface PeerSetup { peerId: string; publicKey: string | null; selectedPeer: Invitation | null; session: SessionSettings | null; addresses: string[] }
export interface PeerState { phase: "disconnected" | "listening" | "connecting" | "handshaking" | "connected" | "reconnecting" | "disconnecting"; peerId: string | null; revision: number; error: string | null }
export interface PeerApi {
  setup(): Promise<PeerSetup>;
  state(): Promise<PeerState>;
  code(address: string, port: number): Promise<string>;
  connect(code: string, role: PeerRole, address: string, port: number): Promise<void>;
  reconnect(): Promise<void>;
  disconnect(): Promise<void>;
  subscribe(callback: (state: PeerState) => void): Promise<() => void>;
}
async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> { requireTauri(); return invoke<T>(name, args); }
export const tauriPeerApi: PeerApi = {
  setup: () => command("get_peer_setup"), state: () => command("get_peer_state"),
  code: (address, port) => command("create_connection_code", { address, port }),
  connect: (code, role, address, port) => command("start_peer_session", { code, role, address, port }),
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
