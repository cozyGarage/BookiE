const MAX_HEIGHT = 8_000_000;
const OVERSCAN = 3;
const CONTROL_MARKERS = { '\n': '↵', '\r': '␍', '\t': '⇥', '\0': '␀', '\u2028': '↵', '\u2029': '↵' };

export function visibleWindow(scrollTop, viewportHeight, rowHeight, totalRows) {
  if (totalRows <= 0 || viewportHeight <= 0 || rowHeight <= 0) return { offset: 0, limit: 0, height: 0 };
  const height = Math.min(totalRows * rowHeight, MAX_HEIGHT);
  const limit = Math.min(totalRows, Math.ceil(viewportHeight / rowHeight) + OVERSCAN * 2);
  const maxScroll = Math.max(0, height - viewportHeight);
  const position = Math.max(0, Math.min(Number.isFinite(scrollTop) ? scrollTop : 0, maxScroll));
  const maxOffset = totalRows - limit;
  const offset = maxScroll === 0 ? 0 : Math.floor((position / maxScroll) * maxOffset);
  return { offset, limit, height };
}

export function displayCell(cell) {
  if (cell.kind === 'null') return 'NULL';
  const value = cell.value;
  if (cell.kind === 'bytes' && value.startsWith('\\x') && value.length > 34 && value.length % 2 === 0) {
    return `${value.slice(0, 34)}… (${(value.length - 2) / 2} bytes total)`;
  }
  let shown = '';
  let count = 0;
  for (const character of value) {
    if (count === 512) return `${shown}…`;
    shown += CONTROL_MARKERS[character] ?? character;
    count += 1;
  }
  return shown;
}

const KINDS = new Set([
  'null', 'boolean', 'integer', 'float', 'text', 'bytes', 'date', 'time', 'datetime',
  'timestamptz', 'decimal', 'uuid', 'json', 'json_text', 'undecodable',
]);

export function cellClass(cell) {
  const kind = KINDS.has(cell.kind) ? cell.kind : 'text';
  return kind === 'null' ? 'cell null' : `cell kind-${kind}`;
}

export function windowTop(scrollTop, height, limit, rowHeight) {
  return Math.max(0, Math.min(scrollTop, height - limit * rowHeight));
}

export function scrollTopForRow(row, viewportHeight, rowHeight, totalRows) {
  const { limit, height } = visibleWindow(0, viewportHeight, rowHeight, totalRows);
  const maxScroll = Math.max(0, height - viewportHeight);
  const maxOffset = totalRows - limit;
  if (maxScroll === 0 || maxOffset <= 0) return 0;
  const target = Math.max(0, Math.min(row - Math.floor(limit / 2), maxOffset));
  return Math.ceil((target / maxOffset) * maxScroll);
}
