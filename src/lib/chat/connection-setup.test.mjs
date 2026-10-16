import assert from 'node:assert/strict';
import { test } from 'node:test';
import { get } from 'svelte/store';
import { createConnectionStore } from './connection-store.ts';
import { createConnectionSetup } from './connection-setup.ts';
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
function fixture() {
  let addresses = ['192.168.1.2']; let generated = 0; const connections = [];
  const api = {
    setup: async () => ({ addresses, peerId: 'own', publicKey: 'key', selectedPeer: null, session: null }),
    state: async () => ({ phase: 'disconnected', peerId: null, revision: 0, error: null }),
    subscribe: async () => () => {},
    code: async (settings) => { generated++; return settings.mode === 'lan' ? `${settings.localAddress}:${settings.localPort}` : 'internet-code'; },
    connect: async (...args) => { connections.push(args); }, reconnect: async () => {}, disconnect: async () => {},
  };
  const connection = createConnectionStore(api); const form = createConnectionSetup(connection);
  return { api, connection, form, value: () => get(form.state), addresses: value => { addresses = value; }, generated: () => generated, connections, dispose: () => { form.dispose(); connection.dispose(); } };
}
test('validates ports, missing codes and missing addresses before invoking the backend', async () => {
  const f = fixture(); await f.form.load(); f.form.setMode('lan');
  for (const port of ['', '0', '65536', '1.5', 'NaN', '-2']) {
    f.form.setPort(port); await f.form.generate(); assert.match(f.value().validation, /Port/);
  }
  assert.equal(f.generated(), 0);
  f.form.setPort('32147'); await f.form.connect('initiator'); assert.match(f.value().validation, /Verbindungscode/);
  f.form.setPeerCode('  other-code  '); await f.form.connect('responder');
  assert.deepEqual(f.connections[0], ['other-code', { mode: 'lan', role: 'responder', localAddress: '192.168.1.2', localPort: 32147 }]);
  f.addresses([]); await f.form.load(true); await f.form.generate();
  assert.match(f.value().validation, /Keine Netzwerkadresse/); assert.equal(f.generated(), 0); f.dispose();
});
test('endpoint changes invalidate codes, including responses arriving after an address refresh', async () => {
  const f = fixture(); await f.form.load(); f.form.setMode('lan'); await f.form.generate();
  assert.equal(f.value().ownCode, '192.168.1.2:32147');
  f.form.setPort('32148'); assert.equal(f.value().ownCode, '');
  const late = deferred(); f.api.code = () => late.promise;
  const generating = f.form.generate();
  f.addresses(['192.168.1.3']); await f.form.load(true);
  late.resolve('old-endpoint'); await generating;
  assert.equal(f.value().address, '192.168.1.2'); assert.equal(f.value().ownCode, '');
  f.dispose();
});
test('load failures are retryable, form edits survive reloads and disposed generation cannot update the form', async () => {
  const f = fixture(); const setup = f.api.setup; f.api.setup = async () => { throw { code: 'key_unavailable' }; };
  await f.form.load(); f.form.setMode('lan'); assert.equal(f.value().loading, false); assert.match(get(f.connection.state).setupError, /Schlüsselspeicher/);
  f.api.setup = setup; await f.form.load(true); assert.equal(get(f.connection.state).setupError, null);
  f.form.setPeerCode('draft'); f.form.setPort('32148'); await f.form.load(true);
  assert.equal(f.value().peerCode, 'draft'); assert.equal(f.value().port, '32148');
  const late = deferred(); f.api.code = () => late.promise;
  const generating = f.form.generate(); const before = f.value(); f.dispose(); late.resolve('late'); await generating;
  assert.deepEqual(f.value(), before);
});

test('restores a saved endpoint and role; missing addresses never silently select a replacement', async () => {
  const f = fixture(); let reconnects = 0;
  const originalSetup = f.api.setup;
  f.api.setup = async () => ({ ...(await originalSetup()), selectedPeer: { peerId: 'peer', target: { mode: 'lan', address: '192.168.1.1:32147' } }, session: { mode: 'lan', localAddress: '192.168.1.2', localPort: 32149, role: 'responder' } });
  f.api.reconnect = async () => { reconnects++; };
  await f.form.load(); f.form.setMode('lan');
  assert.equal(f.value().port, '32149'); assert.match(f.value().roleLabel, /Wartet/);
  assert.equal(f.value().canReconnect, true); assert.equal(f.value().showEditor, false);
  await f.form.reconnect(); assert.equal(reconnects, 1);
  f.addresses(['192.168.1.3']); await f.form.load(true);
  assert.equal(f.value().address, '192.168.1.2'); assert.equal(f.value().canReconnect, false);
  assert.match(f.value().reconnectHint, /nicht verfügbar/);
  await f.form.reconnect(); assert.equal(reconnects, 1);
  f.form.edit(); assert.equal(f.value().showEditor, true);
  f.form.setAddress('192.168.1.3'); f.form.setPort('32150'); f.form.setPeerCode('new-code');
  await f.form.load(true);
  assert.equal(f.value().address, '192.168.1.3'); assert.equal(f.value().port, '32150');
  assert.equal(f.value().peerCode, 'new-code'); assert.match(f.value().reconnectHint, /nicht verfügbar/);
  f.dispose();
});
test('legacy peers require one setup and editing a saved port requires a new code exchange', async () => {
  const f = fixture(); let session = null; const setup = f.api.setup;
  f.api.setup = async () => ({ ...(await setup()), selectedPeer: { peerId: 'peer', target: { mode: 'lan', address: '192.168.1.1:32147' } }, session });
  await f.form.load(); f.form.setMode('lan'); assert.equal(f.value().showEditor, true); assert.equal(f.value().canReconnect, false);
  session = { mode: 'lan', localAddress: '192.168.1.2', localPort: 32147, role: 'initiator' };
  await f.form.load(true); assert.equal(f.value().canReconnect, true);
  f.form.setPort('32148'); assert.equal(f.value().canReconnect, false);
  assert.match(f.value().reconnectHint, /Codes austauschen/); f.dispose();
});
test('copy reports success or fallback without losing codes and ignores completion for an invalidated code', async () => {
  const f = fixture(); f.form.dispose();
  const copied = []; let reject = false; let pending = null;
  f.form = createConnectionSetup(f.connection, { writeText: async text => { copied.push(text); if (reject) throw Error('denied'); if (pending) await pending.promise; } });
  f.form.setMode('lan'); f.value = () => get(f.form.state);
  await f.form.load(); f.form.setMode('lan'); await f.form.generate(); const code = f.value().ownCode;
  await f.form.copyCode(); assert.equal(copied[0], code); assert.equal(f.value().copyFeedback, 'Code kopiert.');
  reject = true; await f.form.copyCode(); assert.match(f.value().copyFeedback, /markieren/); assert.equal(f.value().ownCode, code);
  reject = false; pending = deferred(); const copying = f.form.copyCode();
  f.form.setPort('32148'); pending.resolve(); await copying;
  assert.equal(f.value().copyFeedback, ''); assert.equal(f.value().ownCode, '');
  f.form.dispose(); f.connection.dispose();
});

test('fresh setup defaults to Internet and code generation does not need a LAN address', async () => {
  const f = fixture(); await f.form.load(); f.addresses([]); await f.form.load(true);
  assert.equal(f.value().mode, 'internet'); await f.form.generate();
  assert.equal(f.value().ownCode, 'internet-code');
  f.form.setPeerCode('internet-peer'); await f.form.connect();
  assert.deepEqual(f.connections[0], ['internet-peer', { mode: 'internet' }]); f.dispose();
});
test('mode switches retain independent peer codes and LAN settings while invalidating own codes', async () => {
  const f = fixture(); await f.form.load(); f.form.setPeerCode('internet draft'); await f.form.generate();
  f.form.setMode('lan'); assert.equal(f.value().ownCode, ''); f.form.setPort('32149'); f.form.setPeerCode('lan draft');
  await f.form.generate(); assert.equal(f.value().ownCode, '192.168.1.2:32149');
  f.form.setMode('internet'); assert.equal(f.value().peerCode, 'internet draft'); assert.equal(f.value().ownCode, '');
  await f.form.load(true); f.form.setMode('lan');
  assert.equal(f.value().peerCode, 'lan draft'); assert.equal(f.value().port, '32149'); f.dispose();
});
test('internet code responses survive interface changes but not profile-mode changes', async () => {
  const f = fixture(); await f.form.load(); const pending = deferred(); f.api.code = () => pending.promise;
  const generating = f.form.generate(); f.addresses(['192.168.1.9']); await f.form.load(true);
  pending.resolve('stable-internet-code'); await generating; assert.equal(f.value().ownCode, 'stable-internet-code');
  const next = deferred(); f.api.code = () => next.promise; const stale = f.form.generate();
  f.api.setup = async () => ({ addresses: ['192.168.1.9'], peerId: 'own', selectedPeer: { peerId: 'peer', target: { mode: 'lan', address: '192.168.1.8:32147' } }, session: { mode: 'lan', localAddress: '192.168.1.9', localPort: 32147, role: 'initiator' } });
  await f.form.load(true); next.resolve('wrong-mode'); await stale;
  assert.equal(f.value().mode, 'lan'); assert.equal(f.value().ownCode, ''); f.dispose();
});
test('saved Internet reconnect needs no LAN address and restores its mode', async () => {
  const f = fixture(); f.api.setup = async () => ({ addresses: [], peerId: 'own', selectedPeer: { peerId: 'peer', target: { mode: 'internet', endpointId: 'endpoint', relayUrl: 'https://relay/' } }, session: { mode: 'internet' } });
  let retries = 0; f.api.reconnect = async () => retries++;
  await f.form.load(); assert.equal(f.value().mode, 'internet'); assert.equal(f.value().canReconnect, true);
  await f.form.reconnect(); assert.equal(retries, 1); f.form.edit(); f.form.setMode('lan');
  assert.equal(f.value().canReconnect, false); assert.match(f.value().reconnectHint, /Verbindungsmodus/); f.dispose();
});
