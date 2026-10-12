import assert from "node:assert/strict";
import { test } from "node:test";
import { get } from "svelte/store";
import { createChatStore } from "./chat-store.ts";

test("rejects empty and whitespace-only input without notifying subscribers", () => {
  const chat = createChatStore();
  let notifications = 0;
  const unsubscribe = chat.messages.subscribe(() => notifications++);
  for (const input of ["", "   ", "\n\t "]) {
    assert.equal(chat.sendMessage(input), false);
  }
  assert.deepEqual(get(chat.messages), []);
  assert.equal(notifications, 1);
  unsubscribe();
});

test("trims outer whitespace while preserving inner line breaks and literal text", () => {
  const chat = createChatStore();
  const before = Date.now();
  assert.equal(chat.sendMessage(" \nHallo\n  <b>Welt</b>\t "), true);
  const [message] = get(chat.messages);
  assert.equal(message.text, "Hallo\n  <b>Welt</b>");
  assert.ok(message.createdAt >= before && message.createdAt <= Date.now());
});

test("appends messages in order with distinct IDs without mutating previous lists", () => {
  const chat = createChatStore();
  chat.sendMessage("Erste");
  const previous = get(chat.messages);
  chat.sendMessage("Zweite");
  chat.sendMessage("Zweite");
  const current = get(chat.messages);
  assert.deepEqual(current.map((message) => message.text), ["Erste", "Zweite", "Zweite"]);
  assert.equal(new Set(current.map((message) => message.id)).size, 3);
  assert.equal(previous.length, 1);
  assert.notEqual(previous, current);
});

test("keeps separate chat instances independent and initializes each empty", () => {
  const first = createChatStore();
  first.sendMessage("Nur hier");
  const second = createChatStore();
  assert.deepEqual(get(second.messages), []);
  assert.equal(get(first.messages).length, 1);
  assert.equal(second.messages.set, undefined);
});
