import test from 'node:test';
import assert from 'node:assert/strict';
import { commitKey, createEdits, editStatus, isEditable, loadEditCell } from './edits.mjs';

test('opening a preview loads the exact value and ordinary cells need no request', async () => {
  const exact = { kind: 'text', value: '💾'.repeat(4096), previewed: false };
  let requests = 0;
  const load = async () => { requests += 1; return exact; };
  const preview = { kind: 'text', value: 'partial…', previewed: true };
  assert.equal(await loadEditCell(preview, load, () => true), exact);
  assert.equal(requests, 1);
  assert.equal(await loadEditCell(exact, load, () => true), exact);
  assert.equal(requests, 1);
  assert.equal(preview.value, 'partial…');
});

test('a full value reply for a replaced view never seeds the editor', async () => {
  let finish;
  let current = true;
  const pending = loadEditCell(
    { kind: 'bytes', value: '\\xab…', previewed: true },
    () => new Promise((resolve) => { finish = resolve; }),
    () => current,
  );
  current = false;
  finish({ kind: 'bytes', value: '\\xabcdef', previewed: false });
  assert.equal(await pending, null);
});

test('a failed or still-previewed reply cannot become editable content', async () => {
  const preview = { kind: 'text', value: 'partial…', previewed: true };
  await assert.rejects(loadEditCell(preview, async () => preview, () => true));
  await assert.rejects(loadEditCell(preview, async () => { throw new Error('unavailable'); }, () => true));
  assert.equal(await loadEditCell(preview, async () => { throw new Error('unavailable'); }, () => false), null);
});

test('edits are kept per row and column and can be looked up by row range', () => {
  const edits = createEdits();
  assert.equal(edits.get(3, 1), null);
  edits.set(3, 1, { kind: 'text', value: 'x' });
  edits.set(9, 3, { kind: 'integer', value: '5' });
  assert.deepEqual(edits.get(3, 1), { kind: 'text', value: 'x' });
  assert.equal(edits.get(3, 2), null);
  assert.equal(edits.count(), 2);
  assert.equal(edits.inRange(0, 2), false);
  assert.equal(edits.inRange(2, 3), true);
  assert.equal(edits.inRange(9, 100), true);
  edits.clear();
  assert.equal(edits.count(), 0);
});

test('only the label, state and typed columns of a decodable cell can be edited', () => {
  assert.equal(isEditable(0, { kind: 'integer' }), false);
  assert.equal(isEditable(1, { kind: 'text' }), true);
  assert.equal(isEditable(2, { kind: 'null' }), true);
  assert.equal(isEditable(3, { kind: 'undecodable' }), false);
});

test('multiline values cannot enter the single-line editor and retain their original text', () => {
  for (const separator of ['\n', '\r', '\r\n']) {
    const cell = { kind: 'text', value: `first${separator}second` };
    assert.equal(isEditable(1, cell), false);
    assert.equal(cell.value, `first${separator}second`);
  }
  assert.equal(isEditable(1, { kind: 'text', value: 'single line' }), true);
});

test('a preview hiding a later newline cannot become an editable flattened value', async () => {
  const value = `${'x'.repeat(4096)}\nlast`;
  const exact = await loadEditCell(
    { kind: 'text', value: 'short preview…', previewed: true },
    async () => ({ kind: 'text', value, previewed: false }),
    () => true,
  );
  assert.equal(isEditable(1, exact), false);
  assert.equal(exact.value, value);
});

test('the status says the edits are local and counts them', () => {
  assert.equal(editStatus(0), '');
  assert.match(editStatus(1), /^1 cell edited in this view only/);
  assert.match(editStatus(1200), /^1,200 cells edited/);
});

test('Enter commits, Escape cancels and Ctrl+Backspace sets NULL', () => {
  assert.equal(commitKey({ key: 'Enter' }), 'commit');
  assert.equal(commitKey({ key: 'Escape' }), 'cancel');
  assert.equal(commitKey({ key: 'Backspace', ctrlKey: true }), 'null');
  assert.equal(commitKey({ key: 'Backspace', ctrlKey: false }), null);
  assert.equal(commitKey({ key: 'a' }), null);
});
