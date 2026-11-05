import test from 'node:test';
import assert from 'node:assert/strict';
import { clampCursor, cursorLabel, moveCursor, scrollTopToReveal } from './cursor.mjs';

const grid = { rowCount: 100, pageRows: 10 };

test('arrow keys move one cell and stop at the edges', () => {
  assert.deepEqual(moveCursor({ row: 5, column: 1 }, 'ArrowDown', grid), { row: 6, column: 1 });
  assert.deepEqual(moveCursor({ row: 0, column: 0 }, 'ArrowUp', grid), { row: 0, column: 0 });
  assert.deepEqual(moveCursor({ row: 0, column: 3 }, 'ArrowRight', grid), { row: 0, column: 3 });
  assert.deepEqual(moveCursor({ row: 0, column: 0 }, 'ArrowLeft', grid), { row: 0, column: 0 });
});

test('Page keys move by a screenful minus one row and clamp', () => {
  assert.deepEqual(moveCursor({ row: 20, column: 2 }, 'PageDown', grid), { row: 29, column: 2 });
  assert.deepEqual(moveCursor({ row: 3, column: 2 }, 'PageUp', grid), { row: 0, column: 2 });
  assert.deepEqual(moveCursor({ row: 95, column: 0 }, 'PageDown', grid), { row: 99, column: 0 });
});

test('Home and End go to the row ends, with Ctrl to the grid corners', () => {
  assert.deepEqual(moveCursor({ row: 7, column: 2 }, 'Home', grid), { row: 7, column: 0 });
  assert.deepEqual(moveCursor({ row: 7, column: 2 }, 'End', grid), { row: 7, column: 3 });
  assert.deepEqual(moveCursor({ row: 7, column: 2 }, 'Home', { ...grid, ctrl: true }), { row: 0, column: 0 });
  assert.deepEqual(moveCursor({ row: 7, column: 2 }, 'End', { ...grid, ctrl: true }), { row: 99, column: 3 });
});

test('an unknown key, or an empty grid, gives no move', () => {
  assert.equal(moveCursor({ row: 1, column: 1 }, 'x', grid), null);
  assert.equal(moveCursor(null, 'ArrowDown', { rowCount: 0, pageRows: 10 }), null);
});

test('the first move from no cursor starts at the top left', () => {
  assert.deepEqual(moveCursor(null, 'ArrowDown', grid), { row: 1, column: 0 });
  assert.deepEqual(clampCursor({ row: 500, column: 9 }, 100), { row: 99, column: 3 });
});

test('scrolling reveals the cursor row only when it is outside the viewport', () => {
  assert.equal(scrollTopToReveal(5, 0, 360, 36), 0);
  assert.equal(scrollTopToReveal(12, 0, 360, 36), 12 * 36 + 36 - 360);
  assert.equal(scrollTopToReveal(2, 360, 360, 36), 72);
});

test('the label names the row and column, and is empty without a cursor', () => {
  assert.equal(cursorLabel({ row: 4, column: 2 }), 'Row 5, State');
  assert.equal(cursorLabel(null), '');
});
