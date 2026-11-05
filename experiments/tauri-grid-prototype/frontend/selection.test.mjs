import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  MAX_COPY_ROWS,
  copyRequest,
  isSelected,
  selectedRange,
  selectionAfterClick,
  selectionCount,
  selectionLabel,
} from './selection.mjs';

test('a plain click selects one row and replaces the previous selection', () => {
  const first = selectionAfterClick(null, 10);
  assert.deepEqual(selectedRange(first), { first: 10, last: 10 });
  const second = selectionAfterClick(first, 40);
  assert.deepEqual(selectedRange(second), { first: 40, last: 40 });
});

test('shift extends from the anchor in either direction and keeps the anchor', () => {
  const anchored = selectionAfterClick(null, 100);
  const down = selectionAfterClick(anchored, 105, { shift: true });
  assert.deepEqual(selectedRange(down), { first: 100, last: 105 });
  const up = selectionAfterClick(down, 90, { shift: true });
  assert.deepEqual(selectedRange(up), { first: 90, last: 100 });
  assert.equal(selectionCount(up), 11);
});

test('shift without an earlier selection behaves like a plain click', () => {
  assert.deepEqual(selectedRange(selectionAfterClick(null, 7, { shift: true })), { first: 7, last: 7 });
});

test('a bad row number leaves the selection unchanged', () => {
  const selection = selectionAfterClick(null, 3);
  for (const bad of [-1, 1.5, NaN, undefined, '4']) assert.equal(selectionAfterClick(selection, bad), selection);
});

test('rows inside the range are selected and rows outside are not', () => {
  const selection = selectionAfterClick(selectionAfterClick(null, 5), 8, { shift: true });
  assert.deepEqual([4, 5, 7, 8, 9].map((row) => isSelected(selection, row)), [false, true, true, true, false]);
  assert.equal(isSelected(null, 5), false);
});

test('the label counts rows and warns when the range is too large to copy', () => {
  assert.equal(selectionLabel(null), '');
  assert.equal(selectionLabel(selectionAfterClick(null, 0)), '1 row selected');
  const big = selectionAfterClick(selectionAfterClick(null, 0), MAX_COPY_ROWS, { shift: true });
  assert.match(selectionLabel(big), /5,001 rows selected \(copy is limited to 5,000\)/);
});

test('copy is requested only for a range within the limit', () => {
  assert.equal(copyRequest(null), null);
  const ok = selectionAfterClick(selectionAfterClick(null, 10), 10 + MAX_COPY_ROWS - 1, { shift: true });
  assert.deepEqual(copyRequest(ok), { first: 10, last: 10 + MAX_COPY_ROWS - 1 });
  const big = selectionAfterClick(selectionAfterClick(null, 10), 10 + MAX_COPY_ROWS, { shift: true });
  assert.equal(copyRequest(big), null);
});
