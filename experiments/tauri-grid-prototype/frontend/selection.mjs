export const MAX_COPY_ROWS = 5_000;

export function selectionAfterClick(selection, row, { shift = false } = {}) {
  if (!Number.isInteger(row) || row < 0) return selection;
  if (shift && selection) return { anchor: selection.anchor, focus: row };
  return { anchor: row, focus: row };
}

export function selectedRange(selection) {
  if (!selection) return null;
  return { first: Math.min(selection.anchor, selection.focus), last: Math.max(selection.anchor, selection.focus) };
}

export function isSelected(selection, row) {
  const range = selectedRange(selection);
  return range !== null && row >= range.first && row <= range.last;
}

export function selectionCount(selection) {
  const range = selectedRange(selection);
  return range === null ? 0 : range.last - range.first + 1;
}

export function selectionLabel(selection) {
  const count = selectionCount(selection);
  if (count === 0) return '';
  const rows = count === 1 ? 'row' : 'rows';
  const suffix = count > MAX_COPY_ROWS ? ` (copy is limited to ${MAX_COPY_ROWS.toLocaleString('en-US')})` : '';
  return `${count.toLocaleString('en-US')} ${rows} selected${suffix}`;
}

export function copyRequest(selection) {
  const range = selectedRange(selection);
  if (range === null || selectionCount(selection) > MAX_COPY_ROWS) return null;
  return range;
}
