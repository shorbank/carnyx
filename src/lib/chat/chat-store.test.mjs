import assert from 'node:assert/strict';
import { test } from 'node:test';
import { get } from 'svelte/store';
import { createChatStore } from './chat-store.ts';

const message = (id, createdAt = 1234) => ({
  id, text: `Text ${id}`, createdAt,
  senderId: "61f3f556-b896-4b43-bce0-507d166543e8",
  recipientId: null, direction: "local", receivedAt: null,
});
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
function fixture(initial = []) {
  let expired;
  let unsubscribed = 0;
  const api = {
    listMessages: async () => initial,
    sendMessage: async () => message('confirmed'),
    subscribeExpired: async (callback) => { expired = callback; return () => { unsubscribed++; }; },
  };
  const chat = createChatStore(api);
  return { api, chat, value: () => get(chat.state), expire: (ids) => expired(ids), unsubscribed: () => unsubscribed };
}

test('subscribes before loading and blocks sends until initial history is ready', async () => {
  const f = fixture([message('old')]);
  const registration = deferred();
  let lists = 0;
  f.api.subscribeExpired = () => registration.promise;
  f.api.listMessages = async () => { lists++; return [message('old')]; };
  const starting = f.chat.start();
  assert.equal(await f.chat.sendMessage('hello'), false);
  assert.equal(lists, 0);
  registration.resolve(() => {});
  await starting;
  assert.equal(f.value().loaded, true);
  assert.deepEqual(f.value().messages, [message('old')]);
  assert.equal(f.value().loading, false);
});

test('failed initial load can be retried without a second listener', async () => {
  const f = fixture();
  f.api.listMessages = async () => { throw { code: 'storage_error' }; };
  await f.chat.start();
  assert.equal(f.value().loaded, false);
  assert.equal(await f.chat.sendMessage('hello'), false);
  f.api.listMessages = async () => [message('saved')];
  await f.chat.refresh();
  assert.equal(f.value().loaded, true);
  assert.equal(f.value().error, null);
  f.chat.dispose();
  assert.equal(f.unsubscribed(), 1);
});

test('only confirmed messages are appended and parallel sends are rejected', async () => {
  const f = fixture();
  await f.chat.start();
  const saved = deferred();
  let calls = 0;
  f.api.sendMessage = (text) => { calls++; assert.equal(text, ' raw text '); return saved.promise; };
  const sending = f.chat.sendMessage(' raw text ');
  assert.deepEqual(f.value().messages, []);
  assert.equal(f.value().sending, true);
  assert.equal(await f.chat.sendMessage('second'), false);
  saved.resolve(message('rust-id'));
  assert.equal(await sending, true);
  assert.equal(calls, 1);
  assert.deepEqual(f.value().messages, [message('rust-id')]);
  assert.equal(f.value().sending, false);
});

test('empty input and failed saves never create local messages or leak technical errors', async () => {
  const f = fixture();
  await f.chat.start();
  let calls = 0;
  f.api.sendMessage = async () => { calls++; throw new Error('private database path'); };
  for (const text of ['', '  ', '\n\t']) assert.equal(await f.chat.sendMessage(text), false);
  assert.equal(calls, 0);
  assert.equal(await f.chat.sendMessage('hello'), false);
  assert.deepEqual(f.value().messages, []);
  assert.equal(f.value().error.operation, 'send');
  assert.ok(!f.value().error.message.includes('private'));
  assert.equal(f.value().sending, false);
});

test('a stale history response cannot overwrite a confirmed send', async () => {
  const f = fixture([message('old')]);
  await f.chat.start();
  const stale = deferred();
  let lists = 0;
  f.api.listMessages = () => ++lists === 1 ? stale.promise : Promise.resolve([message('old'), message('confirmed')]);
  const loading = f.chat.refresh();
  await f.chat.sendMessage('hello');
  stale.resolve([message('old')]);
  await loading;
  assert.deepEqual(f.value().messages.map((m) => m.id), ['old', 'confirmed']);
  assert.equal(lists, 2);
});

test('expiration removes rows and invalidates a pending snapshot', async () => {
  const f = fixture([message('old'), message('valid', 1235)]);
  await f.chat.start();
  const stale = deferred();
  let lists = 0;
  f.api.listMessages = () => ++lists === 1 ? stale.promise : Promise.resolve([message('valid', 1235)]);
  const loading = f.chat.refresh();
  f.expire(['old']);
  assert.deepEqual(f.value().messages.map((m) => m.id), ['valid']);
  stale.resolve([message('old'), message('valid', 1235)]);
  await loading;
  assert.deepEqual(f.value().messages.map((m) => m.id), ['valid']);
});

test('multiple refresh requests are coalesced and background failures preserve history', async () => {
  const f = fixture([message('keep')]);
  await f.chat.start();
  const pending = deferred();
  let lists = 0;
  f.api.listMessages = () => ++lists === 1 ? pending.promise : Promise.reject({ code: 'storage_error' });
  const refreshes = [f.chat.refresh(), f.chat.refresh(), f.chat.refresh()];
  pending.resolve([message('keep')]);
  await Promise.all(refreshes);
  assert.equal(lists, 2);
  assert.deepEqual(f.value().messages, [message('keep')]);
  assert.equal(f.value().loaded, true);
  assert.equal(f.value().error.operation, 'load');
});

test('a snapshot containing an already committed send does not duplicate the confirmation', async () => {
  const f = fixture();
  await f.chat.start();
  const saving = deferred();
  f.api.sendMessage = () => saving.promise;
  const sent = f.chat.sendMessage('hello');
  f.api.listMessages = async () => [message('confirmed')];
  await f.chat.refresh();
  saving.resolve(message('confirmed'));
  await sent;
  assert.equal(f.value().messages.length, 1);
});

test('dispose releases a listener arriving late and does not start history loading', async () => {
  const f = fixture();
  const registration = deferred();
  let lists = 0, unsubscribed = 0;
  f.api.subscribeExpired = () => registration.promise;
  f.api.listMessages = async () => { lists++; return []; };
  const starting = f.chat.start();
  f.chat.dispose();
  registration.resolve(() => { unsubscribed++; });
  await starting;
  assert.equal(unsubscribed, 1);
  assert.equal(lists, 0);
});

test('dispose ignores late load/send responses and further events', async () => {
  const f = fixture([message('keep')]);
  await f.chat.start();
  const loading = deferred(), sending = deferred();
  f.api.listMessages = () => loading.promise;
  f.api.sendMessage = () => sending.promise;
  const load = f.chat.refresh(), send = f.chat.sendMessage('hello');
  f.chat.dispose();
  const before = f.value();
  f.expire(['keep']);
  loading.resolve([]);
  sending.resolve(message('late'));
  await load;
  assert.equal(await send, false);
  assert.equal(f.value(), before);
  assert.equal(f.unsubscribed(), 1);
  await f.chat.refresh();
});

test('connection failure stays unavailable and retry establishes a fresh subscription', async () => {
  const f = fixture();
  const subscribe = f.api.subscribeExpired;
  f.api.subscribeExpired = async () => { throw { code: 'not_available' }; };
  await f.chat.start();
  assert.match(f.value().error.message, /Desktop-App/);
  assert.equal(f.value().loaded, false);
  f.api.subscribeExpired = subscribe;
  await f.chat.refresh();
  assert.equal(f.value().loaded, true);
  f.chat.dispose();
});

test('a delayed save confirmation cannot resurrect a message expired during standby', async () => {
  const f = fixture();
  await f.chat.start();
  const saving = deferred();
  f.api.sendMessage = () => saving.promise;
  const sent = f.chat.sendMessage('hello');
  f.expire(['already-expired']);
  saving.resolve(message('already-expired'));
  assert.equal(await sent, true);
  assert.deepEqual(f.value().messages, []);
});

test('appending a local message keeps received history ordered by local receipt time', async () => {
  const received = { ...message('received', 1), direction: 'incoming', receivedAt: 2000,
    recipientId: '6b9ab3f5-a0d1-47b8-a224-ae3a03a23d8b' };
  const f = fixture([received]);
  await f.chat.start();
  f.api.sendMessage = async () => message('local', 1999);
  await f.chat.sendMessage('hello');
  assert.deepEqual(f.value().messages.map((m) => m.id), ['local', 'received']);
});
