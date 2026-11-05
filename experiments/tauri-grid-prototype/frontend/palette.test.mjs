import test from 'node:test';
import assert from 'node:assert/strict';
import { fuzzyScore, paletteMatches, paletteSelection } from './palette.mjs';

const actions = [
  { id: 'first', label: 'Go to first page' },
  { id: 'last', label: 'Go to last page' },
  { id: 'filter', label: 'Filter rows' },
  { id: 'clear', label: 'Clear filter', enabled: false },
  { id: 'copy', label: 'Copy selected rows' },
];

test('an empty query lists every enabled action in order', () => {
  assert.deepEqual(paletteMatches('', actions).map((a) => a.id), ['first', 'last', 'filter', 'copy']);
});

test('letters must appear in order; unmatched actions drop out', () => {
  assert.deepEqual(paletteMatches('lpg', actions).map((a) => a.id), ['last']);
  assert.deepEqual(paletteMatches('gtp', actions).map((a) => a.id), ['first', 'last']);
  assert.deepEqual(paletteMatches('zzz', actions), []);
});

test('a disabled action never appears, even when it matches', () => {
  assert.deepEqual(paletteMatches('clear', actions), []);
});

test('word starts and runs of consecutive letters rank higher', () => {
  assert.ok(fuzzyScore('filter', 'Filter rows') > fuzzyScore('filter', 'Clear filter now'));
  assert.equal(fuzzyScore('abc', 'xyz'), 0);
});

test('arrow keys cycle through the list and the index is clamped', () => {
  assert.equal(paletteSelection(0, 3, 'ArrowUp'), 2);
  assert.equal(paletteSelection(2, 3, 'ArrowDown'), 0);
  assert.equal(paletteSelection(9, 3, 'x'), 2);
  assert.equal(paletteSelection(5, 0, 'ArrowDown'), 0);
});
