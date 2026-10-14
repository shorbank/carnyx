import assert from 'node:assert/strict';
import { test } from 'node:test';
import { get } from 'svelte/store';
import { createChatWorkspace } from './chat-workspace.ts';

const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
const flush = () => new Promise(resolve => setImmediate(resolve));
function fixture() {
  let emit;
  let selected = 'alice';
  let removed = 0, connects = 0, reads = 0, stateReads = 0, sends = 0;
  const chatApis = new Map();
  function chatApi(id) {
    if (!chatApis.has(id)) chatApis.set(id, {
      listMessages: async () => { reads++; return []; },
      sendMessage: async text => { sends++; return { id: `${id}-${sends}`, text, createdAt: 1, receivedAt: null, direction: id === 'local' ? 'local' : 'outgoing' }; },
      subscribeExpired: async () => () => removed++,
      subscribeChanged: async () => () => removed++,
    });
    return chatApis.get(id);
  }
  const connection = {
    state: async () => { stateReads++; return { phase: 'disconnected', peerId: null, revision: 0, error: null }; },
    setup: async () => ({ peerId: 'own', publicKey: 'key', addresses: ['192.168.1.2'], session: null, selectedPeer: selected ? { peerId: selected } : null }),
    code: async () => 'own-code', connect: async () => { connects++; }, reconnect: async () => {}, disconnect: async () => {},
    subscribe: async fn => { emit = fn; return () => removed++; },
  };
  const workspace = createChatWorkspace({ local: chatApi('local'), peer: chatApi, connection });
  return { workspace, connection, chatApi, selectPeer: id => { selected = id; }, emit: (id, phase = 'connected', revision = 1) => emit({ phase, peerId: id, revision, error: null }), value: () => get(workspace.state), counts: () => ({ removed, connects, reads, stateReads, sends }) };
}

test('tabs retain independent drafts, setup does not connect, and disconnected peer drafts cannot send', async () => {
  const f = fixture(); await f.workspace.start(); await flush();
  f.value().local.draft.setText('local draft');
  f.workspace.select('connection'); await flush();
  f.workspace.select('peer'); await flush();
  f.value().peer.draft.setText('peer draft');
  assert.equal(await f.value().peer.draft.send(), false);
  assert.equal(f.counts().sends, 0);
  f.workspace.select('local');
  assert.equal(get(f.value().local.draft.state).text, 'local draft');
  f.workspace.select('peer');
  assert.equal(get(f.value().peer.draft.state).text, 'peer draft');
  f.emit('alice');
  assert.equal(f.value().peerCanSend, true);
  assert.equal(await f.value().peer.draft.send(), true);
  assert.equal(get(f.value().peer.draft.state).text, '');
  assert.equal(f.counts().connects, 0);
  f.workspace.dispose();
  assert.equal(f.counts().removed, 5);
});

test('late results stay with their conversation and sending requires the selected authenticated peer', async () => {
  const f = fixture(); await f.workspace.start(); await flush();
  f.workspace.select('connection'); await flush(); f.workspace.select('peer'); await flush(); f.emit('alice');
  const alice = f.value().peer;
  const pending = deferred(); f.chatApi('alice').sendMessage = () => pending.promise;
  alice.draft.setText('for Alice'); const send = alice.draft.send();
  f.selectPeer('bob'); await f.workspace.setup.load(true); await flush();
  const bob = f.value().peer;
  assert.equal(bob.id, 'bob'); assert.equal(f.value().peerCanSend, false);
  pending.resolve({ id: 'message-a', text: 'for Alice', createdAt: 1, receivedAt: null, direction: 'outgoing' }); await send;
  assert.equal(get(bob.chat.state).messages.length, 0);
  assert.equal(get(alice.chat.state).messages[0].id, 'message-a');
  f.emit('bob', 'connected', 2); assert.equal(f.value().peerCanSend, true);
  f.emit('bob', 'reconnecting', 3); assert.equal(f.value().peerCanSend, false);
  f.workspace.dispose();
});

test('focus refreshes active history and connection; dispose ignores pending responses and removes every listener', async () => {
  const f = fixture(); await f.workspace.start(); await flush();
  const before = f.counts(); await f.workspace.refresh();
  assert.equal(f.counts().reads, before.reads + 1);
  assert.equal(f.counts().stateReads, before.stateReads + 1);
  assert.equal(f.counts().connects, 0);
  const pending = deferred(); f.chatApi('local').listMessages = () => pending.promise;
  const refresh = f.workspace.refresh(); f.workspace.dispose();
  pending.resolve([{ id: 'late' }]); await refresh;
  assert.equal(get(f.value().local.chat.state).messages.length, 0);
  const last = f.value(); f.emit('late'); f.workspace.select('peer'); await f.workspace.refresh();
  assert.equal(f.value(), last);
  assert.equal(f.counts().removed, 3);
  f.workspace.dispose(); assert.equal(f.counts().removed, 3);
});

test('saved peer and history are available directly after startup and reconnect preserves a draft across tabs', async () => {
  const f = fixture(); let reconnects = 0;
  const setup = f.connection.setup;
  f.connection.setup = async () => ({ ...(await setup()), session: { localAddress: '192.168.1.2', localPort: 32147, role: 'initiator' } });
  const pending = deferred(); f.connection.reconnect = async () => { reconnects++; await pending.promise; f.emit('alice'); };
  await f.workspace.start(); assert.equal(f.value().peer.id, 'alice'); assert.equal(reconnects, 0);
  f.workspace.select('peer'); await flush(); assert.equal(get(f.value().peer.chat.state).loaded, true);
  f.value().peer.draft.setText('retained');
  const reconnect = f.workspace.setup.reconnect(); f.workspace.select('local');
  pending.resolve(); await reconnect; f.workspace.select('peer');
  assert.equal(reconnects, 1); assert.equal(f.value().peerCanSend, true);
  assert.equal(get(f.value().peer.draft.state).text, 'retained');
  f.workspace.dispose();
});
