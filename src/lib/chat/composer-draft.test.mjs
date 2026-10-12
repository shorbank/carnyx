import assert from 'node:assert/strict';
import { test } from 'node:test';
import { get } from 'svelte/store';
import { createComposerDraft } from './composer-draft.ts';

function fixture() {
  let resolve;
  const saved = new Promise((yes) => { resolve = yes; });
  let calls = 0;
  const draft = createComposerDraft(async () => { calls++; return saved; });
  draft.setText('Original');
  return { draft, resolve, value: () => get(draft.state), calls: () => calls };
}

test('unchanged draft clears only on success and rejects a second concurrent send', async () => {
  const f = fixture();
  const saving = f.draft.send();
  assert.equal(f.value().text, 'Original');
  assert.equal(await f.draft.send(), false);
  f.resolve(true);
  assert.equal(await saving, true);
  assert.equal(f.value().text, '');
  assert.equal(f.calls(), 1);
  assert.equal(f.value().sending, false);
});

test('editing while saving preserves the new text even if edited back to the original', async () => {
  for (const edited of ['New draft', 'Original']) {
    const f = fixture();
    const saving = f.draft.send();
    f.draft.setText('Intermediate edit');
    f.draft.setText(edited);
    f.resolve(true);
    await saving;
    assert.equal(f.value().text, edited);
  }
});

test('failure preserves the current draft and allows retry', async () => {
  const f = fixture();
  const saving = f.draft.send();
  f.draft.setText('New draft');
  f.resolve(false);
  assert.equal(await saving, false);
  assert.equal(f.value().text, 'New draft');
  assert.equal(f.value().sending, false);
});

test('unexpected rejection preserves input and resets the pending state', async () => {
  const draft = createComposerDraft(async () => { throw new Error('offline'); });
  draft.setText('Keep me');
  assert.equal(await draft.send(), false);
  assert.equal(get(draft.state).text, 'Keep me');
  assert.equal(get(draft.state).sending, false);
});
