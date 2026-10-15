import assert from 'node:assert/strict';
import { test } from 'node:test';
import { get } from 'svelte/store';
import { createStorageController, storageErrorText } from './storage-controller.ts';

const status = (phase, revision, code = null) => ({ phase, revision, code });
const deferred = () => { let resolve; const promise = new Promise((yes) => { resolve = yes; }); return { promise, resolve }; };
function fixture() {
  let listener;
  let stops = 0, subscriptions = 0, retries = 0;
  const api = {
    status: async () => status('initializing', 1),
    retry: async () => { retries++; },
    subscribe: async (callback) => { subscriptions++; listener = callback; return () => { stops++; }; },
  };
  const controller = createStorageController(api);
  return { controller, api, emit: (value) => listener(value), value: () => get(controller.state), counts: () => ({ stops, subscriptions, retries }) };
}
test('subscribes before snapshot and ignores a late older snapshot', async () => {
  const f = fixture(); const snapshot = deferred();
  f.api.status = () => { f.emit(status('ready', 3)); return snapshot.promise; };
  const starting = f.controller.start(); await Promise.resolve();
  snapshot.resolve(status('initializing', 1)); await starting;
  assert.equal(f.value().phase, 'ready'); assert.equal(f.value().revision, 3);
  f.controller.dispose(); assert.equal(f.counts().stops, 1);
});
test('start is idempotent and retries reuse the listener without parallel requests', async () => {
  const f = fixture(); await Promise.all([f.controller.start(), f.controller.start()]);
  f.emit(status('blocked', 2, 'key_unavailable'));
  const retry = deferred(); f.api.retry = () => retry.promise;
  const first = f.controller.retry(); await f.controller.retry();
  f.emit(status('initializing', 3)); f.api.status = async () => status('initializing', 3);
  retry.resolve(); await first;
  assert.equal(f.value().phase, 'initializing'); assert.equal(f.counts().subscriptions, 1);
  f.controller.dispose();
});
test('ready remains ready and never requests another initialization', async () => {
  const f = fixture(); await f.controller.start(); f.emit(status('ready', 3));
  f.emit(status('blocked', 4, 'unlock_failed')); await f.controller.retry();
  assert.equal(f.value().phase, 'ready'); assert.equal(f.counts().retries, 0); f.controller.dispose();
});
test('late listener registration after disposal cleans up without loading status', async () => {
  const f = fixture(); const registered = deferred(); let snapshots = 0, stops = 0;
  f.api.subscribe = () => registered.promise; f.api.status = async () => { snapshots++; return status('ready', 1); };
  const starting = f.controller.start(); f.controller.dispose(); registered.resolve(() => stops++); await starting;
  assert.equal(stops, 1); assert.equal(snapshots, 0); assert.equal(f.value().phase, 'initializing');
});
test('late snapshot and event after disposal do not update state', async () => {
  const f = fixture(); const snapshot = deferred(); f.api.status = () => snapshot.promise;
  const starting = f.controller.start(); await Promise.resolve(); f.controller.dispose();
  f.emit(status('ready', 3)); snapshot.resolve(status('ready', 3)); await starting;
  assert.equal(f.value().phase, 'initializing'); assert.equal(f.counts().stops, 1);
});
test('subscription failure can be retried and never reveals native error details', async () => {
  const f = fixture(); const subscribe = f.api.subscribe;
  f.api.subscribe = async () => { throw new Error('secret/native/path'); }; await f.controller.start();
  assert.equal(f.value().code, 'ipc_unavailable');
  f.api.subscribe = subscribe; f.api.status = async () => status('blocked', 2, 'key_missing'); await f.controller.retry();
  assert.equal(f.value().code, 'key_missing'); assert.equal(f.counts().retries, 1); f.controller.dispose();
});
test('snapshot failure retries on the existing listener and honors newer events', async () => {
  const f = fixture(); f.api.status = async () => { throw new Error('native'); }; await f.controller.start();
  assert.equal(f.value().code, 'ipc_unavailable');
  f.api.retry = async () => { f.emit(status('ready', 4)); }; f.api.status = async () => status('initializing', 3);
  await f.controller.retry(); assert.equal(f.value().phase, 'ready'); assert.equal(f.counts().subscriptions, 1); f.controller.dispose();
});
test('storage errors describe missing keys and incomplete cleanup without exposing values', () => {
  assert.match(storageErrorText('key_missing'), /ursprünglichen Schlüssel/);
  assert.match(storageErrorText('cleanup_failed'), /Klartextdatei/);
  assert.match(storageErrorText('unlock_failed'), /beschädigt/);
});
