export const COLUMNS = 4;

export function clampCursor(cursor, rowCount) {
  if (rowCount <= 0) return null;
  const row = Math.max(0, Math.min(cursor?.row ?? 0, rowCount - 1));
  const column = Math.max(0, Math.min(cursor?.column ?? 0, COLUMNS - 1));
  return { row, column };
}

export function moveCursor(cursor, key, { rowCount, pageRows, ctrl = false }) {
  const start = clampCursor(cursor, rowCount);
  if (start === null) return null;
  const step = Math.max(1, pageRows - 1);
  const moves = {
    ArrowUp: { row: start.row - 1 },
    ArrowDown: { row: start.row + 1 },
    ArrowLeft: { column: start.column - 1 },
    ArrowRight: { column: start.column + 1 },
    PageUp: { row: start.row - step },
    PageDown: { row: start.row + step },
    Home: ctrl ? { row: 0, column: 0 } : { column: 0 },
    End: ctrl ? { row: rowCount - 1, column: COLUMNS - 1 } : { column: COLUMNS - 1 },
  };
  const next = moves[key];
  if (next === undefined) return null;
  return clampCursor({ ...start, ...next }, rowCount);
}

export function scrollTopToReveal(row, scrollTop, viewportHeight, rowHeight) {
  const top = row * rowHeight;
  if (top < scrollTop) return top;
  const bottom = top + rowHeight;
  if (bottom > scrollTop + viewportHeight) return bottom - viewportHeight;
  return scrollTop;
}

export function cursorLabel(cursor, names = ['Value', 'Label', 'State', 'Typed']) {
  if (cursor === null) return '';
  return `Row ${cursor.row + 1}, ${names[cursor.column]}`;
}
