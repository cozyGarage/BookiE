import { test } from 'node:test';
import assert from 'node:assert/strict';
import { visibleWindow, displayCell, windowTop } from './grid.mjs';

test('virtual rows stay bounded and cover the beginning and end', () => {
  const first = visibleWindow(0, 360, 36, 1_000_000);
  const last = visibleWindow(99_000_000, 360, 36, 1_000_000);
  assert.equal(first.offset, 0);
  assert.ok(first.limit <= 32);
  assert.equal(last.offset + last.limit, 1_000_000);
  assert.ok(last.limit <= 32);
  assert.ok(first.height <= 8_000_000);
});

test('empty and narrow views never request a negative range', () => {
  assert.deepEqual(visibleWindow(-10, 360, 36, 0), { offset: 0, limit: 0, height: 0 });
  assert.deepEqual(visibleWindow(-10, 36, 36, 1), { offset: 0, limit: 1, height: 36 });
});

test('large integer text and null remain distinct', () => {
  assert.equal(displayCell({ kind: 'integer', value: '9007199254740993' }), '9007199254740993');
  assert.equal(displayCell({ kind: 'null', value: null }), 'NULL');
  assert.equal(displayCell({ kind: 'text', value: '' }), '');
});

test('the final visible row stays within the scrollable surface', () => {
  assert.equal(windowTop(7_999_640, 8_000_000, 16, 36), 7_999_424);
  assert.equal(windowTop(40, 8_000_000, 16, 36), 40);
});
