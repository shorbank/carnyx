import assert from 'node:assert/strict';
import { test } from 'node:test';
import { get } from 'svelte/store';
import { createConnectionStore } from '../../../src/lib/stores/connection-store.ts';

const deferred = () => { let resolve, reject; const promise = new Promise((a,b) => { resolve=a; reject=b; }); return { promise, resolve, reject }; };
const snapshot = (revision, phase='disconnected') => ({ revision, phase, peerId:'peer', error:null });
function fixture() {
  let callback; let removed=0; let connects=0;
  const api = {
    state: async () => snapshot(0),
    setup: async () => ({ peerId:'own', publicKey:'public', selectedPeer:null, session:null, addresses:['127.0.0.1'] }),
    code: async () => 'code',
    connect: async () => { connects++; }, reconnect: async () => {}, disconnect: async () => {},
    subscribe: async (handler) => { callback=handler; return () => removed++; },
  };
  const store=createConnectionStore(api);
  return { api, store, value:()=>get(store.state), emit:(s)=>callback(s), removed:()=>removed, connects:()=>connects };
}
test('subscription precedes snapshot and old snapshot cannot overwrite a new event', async () => {
  const f=fixture(); const read=deferred(); f.api.state=()=>read.promise;
  const start=f.store.start(); await Promise.resolve();
  f.emit(snapshot(3,'connected')); read.resolve(snapshot(1,'connecting')); await start;
  assert.equal(f.value().connection.phase,'connected');
  assert.equal(f.connects(),0); // No automatic network startup.
});
test('setup arriving after removal is ignored and late listener is removed', async () => {
  const f=fixture();const listener=deferred();f.api.subscribe=()=>listener.promise;
  const start=f.store.start();f.store.dispose();let removed=0;listener.resolve(()=>removed++);await start;
  assert.equal(removed,1);
  const other=fixture();const read=deferred();other.api.setup=()=>read.promise;
  const loading=other.store.setup();other.store.dispose();read.resolve({ peerId:'late' });await loading;
  assert.equal(other.value().setup,null);
});
test('parallel actions are blocked and key failures are sanitized', async () => {
  const f=fixture();await f.store.start();const waiting=deferred();f.api.connect=()=>waiting.promise;
  const first=f.store.connect('code',{mode:'lan',role:'initiator',localAddress:'127.0.0.1',localPort:32147});await f.store.connect('code',{mode:'lan',role:'responder',localAddress:'127.0.0.1',localPort:32147});
  assert.equal(f.value().busy,true);waiting.reject({ code:'authentication_failed', detail:'private path' });await first;
  assert.equal(f.value().busy,false);assert.ok(f.value().error.includes('verifiziert'));assert.ok(!f.value().error.includes('private'));
  f.store.dispose();assert.equal(f.removed(),1);
});
test('overlapping setup responses cannot restore a previously selected peer', async () => {
  const f=fixture();const first=deferred();const second=deferred();let reads=0;f.api.setup=()=>++reads===1?first.promise:second.promise;
  const old=f.store.setup();const current=f.store.setup(true);second.resolve({ peerId:'new' });await current;first.resolve({ peerId:'old' });await old;
  assert.equal(f.value().setup.peerId,'new');
});
test('focus refresh can recover a failed event subscription without starting a connection', async () => {
  const f=fixture();const original=f.api.subscribe;f.api.subscribe=async()=>{throw {code:'internal_error'};};
  await f.store.start();f.api.subscribe=original;await f.store.refresh();f.emit(snapshot(4,'connected'));
  assert.equal(f.value().connection.phase,'connected');assert.equal(f.connects(),0);f.store.dispose();assert.equal(f.removed(),1);
});

test('startup restores setup independently of listener failure, without starting a session', async () => {
  const f = fixture(); f.api.subscribe = async () => { throw { code: 'internal_error' }; };
  const saved = { peerId: 'own', publicKey: null, addresses: ['127.0.0.1'], selectedPeer: { peerId: 'peer', target: { mode: 'lan', address: '127.0.0.1:32147' } }, session: { mode: 'lan', localAddress: '127.0.0.1', localPort: 32147, role: 'responder' } };
  f.api.setup = async () => saved;
  await f.store.start();
  assert.deepEqual(f.value().setup, saved);
  assert.equal(f.value().setupStatus, 'ready'); assert.equal(f.connects(), 0);
  f.store.dispose();
});
test('reconnect blocks duplicate actions and discards snapshots from before a failed mutation', async () => {
  const f = fixture(); await f.store.start();
  const old = deferred(); const pending = deferred(); let calls = 0;
  const setup = f.api.setup; f.api.setup = () => old.promise;
  const refresh = f.store.setup(true);
  f.api.reconnect = () => { calls++; return pending.promise; };
  const reconnect = f.store.reconnect();
  await f.store.reconnect(); assert.equal(calls, 1); assert.equal(f.value().busy, true);
  f.api.setup = async () => ({ ...(await setup()), selectedPeer: { peerId: 'new' } });
  pending.reject({ code: 'network_error' }); await reconnect;
  old.resolve({ selectedPeer: { peerId: 'old' } }); await refresh;
  assert.equal(f.value().setup.selectedPeer.peerId, 'new');
  assert.match(f.value().error, /nicht erreichbar/); assert.equal(f.value().busy, false);
  f.store.dispose();
});
