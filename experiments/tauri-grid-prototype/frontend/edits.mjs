export const EDITABLE_COLUMNS = new Set([1, 2, 3]);

export function createEdits() {
  const cells = new Map();
  return {
    get: (row, column) => cells.get(`${row}:${column}`) ?? null,
    set(row, column, cell) {
      cells.set(`${row}:${column}`, cell);
    },
    count: () => cells.size,
    inRange(first, last) {
      return [...cells.keys()].some((key) => {
        const row = Number(key.split(':')[0]);
        return row >= first && row <= last;
      });
    },
    clear: () => cells.clear(),
  };
}

export function isEditable(column, cell) {
  return EDITABLE_COLUMNS.has(column) && cell.kind !== 'undecodable';
}

export function editStatus(count) {
  if (count === 0) return '';
  return `${count.toLocaleString('en-US')} ${count === 1 ? 'cell' : 'cells'} edited in this view only; nothing is saved.`;
}

export function commitKey(event) {
  if (event.key === 'Escape') return 'cancel';
  if (event.key === 'Enter') return 'commit';
  if (event.key === 'Backspace' && event.ctrlKey) return 'null';
  return null;
}
