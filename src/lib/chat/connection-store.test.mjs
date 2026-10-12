import assert from 'node:assert/strict';
import { test } from 'node:test';
import { get } from 'svelte/store';
import { createConnectionStore } from './connection-store.ts';

const deferred = () => { let resolve, reject; const promise = new Promise((a,b) => { resolve=a; reject=b; }); return { promise, resolve, reject }; };
const snapshot = (revision, phase='disconnected') => ({ revision, phase, peerId:'peer', error:null });
function fixture() {
  let callback; let removed=0; let connects=0;
  const api = {
    state: async () => snapshot(0),
    setup: async () => ({ peerId:'own', publicKey:'public', selectedPeer:null, addresses:['127.0.0.1'] }),
    code: async () => 'code',
    connect: async () => { connects++; }, disconnect: async () => {},
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
  const first=f.store.connect('code','initiator','127.0.0.1',32147);await f.store.connect('code','responder','127.0.0.1',32147);
  assert.equal(f.value().busy,true);waiting.reject({ code:'authentication_failed', detail:'private path' });await first;
  assert.equal(f.value().busy,false);assert.ok(f.value().error.includes('verifiziert'));assert.ok(!f.value().error.includes('private'));
  f.store.dispose();assert.equal(f.removed(),1);
});
test('overlapping setup responses cannot restore a previously selected peer', async () => {
  const f=fixture();const first=deferred();const second=deferred();let reads=0;f.api.setup=()=>++reads===1?first.promise:second.promise;
  const old=f.store.setup();const current=f.store.setup();second.resolve({ peerId:'new' });await current;first.resolve({ peerId:'old' });await old;
  assert.equal(f.value().setup.peerId,'new');
});
test('focus refresh can recover a failed event subscription without starting a connection', async () => {
  const f=fixture();const original=f.api.subscribe;f.api.subscribe=async()=>{throw {code:'internal_error'};};
  await f.store.start();f.api.subscribe=original;await f.store.refresh();f.emit(snapshot(4,'connected'));
  assert.equal(f.value().connection.phase,'connected');assert.equal(f.connects(),0);f.store.dispose();assert.equal(f.removed(),1);
});
