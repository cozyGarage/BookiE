const MAX_HEIGHT = 8_000_000;
const OVERSCAN = 3;

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
  return cell.kind === 'null' ? 'NULL' : cell.value;
}

export function windowTop(scrollTop, height, limit, rowHeight) {
  return Math.max(0, Math.min(scrollTop, height - limit * rowHeight));
}
