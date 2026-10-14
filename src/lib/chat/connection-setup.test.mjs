import assert from 'node:assert/strict';
import { test } from 'node:test';
import { get } from 'svelte/store';
import { createConnectionStore } from './connection-store.ts';
import { createConnectionSetup } from './connection-setup.ts';
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
function fixture() {
  let addresses = ['192.168.1.2']; let generated = 0; const connections = [];
  const api = {
    setup: async () => ({ addresses, peerId: 'own', publicKey: 'key', selectedPeer: null }),
    state: async () => ({ phase: 'disconnected', peerId: null, revision: 0, error: null }),
    subscribe: async () => () => {},
    code: async (address, port) => { generated++; return `${address}:${port}`; },
    connect: async (...args) => { connections.push(args); }, disconnect: async () => {},
  };
  const connection = createConnectionStore(api); const form = createConnectionSetup(connection);
  return { api, connection, form, value: () => get(form.state), addresses: value => { addresses = value; }, generated: () => generated, connections, dispose: () => { form.dispose(); connection.dispose(); } };
}
test('validates ports, missing codes and missing addresses before invoking the backend', async () => {
  const f = fixture(); await f.form.load();
  for (const port of ['', '0', '65536', '1.5', 'NaN', '-2']) {
    f.form.setPort(port); await f.form.generate(); assert.match(f.value().validation, /Port/);
  }
  assert.equal(f.generated(), 0);
  f.form.setPort('32147'); await f.form.connect('initiator'); assert.match(f.value().validation, /Verbindungscode/);
  f.form.setPeerCode('  other-code  '); await f.form.connect('responder');
  assert.deepEqual(f.connections[0], ['other-code', 'responder', '192.168.1.2', 32147]);
  f.addresses([]); await f.form.load(true); await f.form.generate();
  assert.match(f.value().validation, /Keine Netzwerkadresse/); assert.equal(f.generated(), 0); f.dispose();
});
test('endpoint changes invalidate codes, including responses arriving after an address refresh', async () => {
  const f = fixture(); await f.form.load(); await f.form.generate();
  assert.equal(f.value().ownCode, '192.168.1.2:32147');
  f.form.setPort('32148'); assert.equal(f.value().ownCode, '');
  const late = deferred(); f.api.code = () => late.promise;
  const generating = f.form.generate();
  f.addresses(['192.168.1.3']); await f.form.load(true);
  late.resolve('old-endpoint'); await generating;
  assert.equal(f.value().address, '192.168.1.3'); assert.equal(f.value().ownCode, '');
  f.dispose();
});
test('load failures are retryable, form edits survive reloads and disposed generation cannot update the form', async () => {
  const f = fixture(); const setup = f.api.setup; f.api.setup = async () => { throw { code: 'key_unavailable' }; };
  await f.form.load(); assert.equal(f.value().loading, false); assert.match(get(f.connection.state).error, /Schlüsselspeicher/);
  f.api.setup = setup; await f.form.load(true); assert.equal(get(f.connection.state).error, null);
  f.form.setPeerCode('draft'); f.form.setPort('32148'); await f.form.load(true);
  assert.equal(f.value().peerCode, 'draft'); assert.equal(f.value().port, '32148');
  const late = deferred(); f.api.code = () => late.promise;
  const generating = f.form.generate(); const before = f.value(); f.dispose(); late.resolve('late'); await generating;
  assert.equal(f.value(), before);
});
