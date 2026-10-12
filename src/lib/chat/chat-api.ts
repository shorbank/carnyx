import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Message } from "./message";

export interface ChatApi {
  listMessages(): Promise<readonly Message[]>;
  sendMessage(text: string): Promise<Message>;
  subscribeExpired(callback: (ids: readonly string[]) => void): Promise<() => void>;
}

function requireTauri() {
  if (!isTauri()) throw { code: "not_available" };
}

export const tauriChatApi: ChatApi = {
  async listMessages() {
    requireTauri();
    return invoke<Message[]>("list_messages");
  },
  async sendMessage(text) {
    requireTauri();
    return invoke<Message>("send_message", { text });
  },
  async subscribeExpired(callback) {
    requireTauri();
    return listen<{ ids: string[] }>("messages_expired", ({ payload }) => callback(payload.ids));
  },
};
