import test from 'node:test';
import assert from 'node:assert/strict';
import { commitKey, createEdits, editStatus, isEditable } from './edits.mjs';

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
