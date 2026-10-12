import { writable, type Readable } from "svelte/store";
import type { Message } from "./message";

export interface ChatStore {
  messages: Readable<readonly Message[]>;
  sendMessage(text: string): boolean;
}

export function createChatStore(): ChatStore {
  const messages = writable<readonly Message[]>([]);

  return {
    messages: { subscribe: messages.subscribe },
    sendMessage(text) {
      const normalizedText = text.trim();
      if (!normalizedText) return false;

      const message: Message = {
        id: crypto.randomUUID(),
        text: normalizedText,
        createdAt: Date.now(),
      };
      messages.update((current) => [...current, message]);
      return true;
    },
  };
}
