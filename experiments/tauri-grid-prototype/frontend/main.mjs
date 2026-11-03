import { visibleWindow, displayCell, windowTop } from './grid.mjs';

const totalRows = 1_000_000;
const rowHeight = 36;
const viewport = document.querySelector('#viewport');
const surface = document.querySelector('#height');
const rows = document.querySelector('#rows');
const status = document.querySelector('#status');
let generation = 0;
let frame = 0;

async function render() {
  const current = ++generation;
  const window = visibleWindow(viewport.scrollTop, viewport.clientHeight, rowHeight, totalRows);
  surface.style.height = `${window.height}px`;
  rows.style.transform = `translateY(${windowTop(viewport.scrollTop, window.height, window.limit, rowHeight)}px)`;
  if (!window.limit) return;
  try {
    const data = await globalThis.__TAURI__.core.invoke('sample_rows', { offset: window.offset, limit: window.limit });
    if (current !== generation) return;
    const fragment = document.createDocumentFragment();
    for (const record of data) {
      const row = document.createElement('div');
      row.className = 'row';
      row.setAttribute('role', 'row');
      row.setAttribute('aria-rowindex', String(record.row_number + 1));
      const number = document.createElement('span');
      number.className = 'cell';
      number.setAttribute('role', 'gridcell');
      number.textContent = String(record.row_number + 1);
      row.append(number);
      for (const cell of record.cells) {
        const item = document.createElement('span');
        item.className = cell.kind === 'null' ? 'cell null' : 'cell';
        item.setAttribute('role', 'gridcell');
        item.textContent = displayCell(cell);
        row.append(item);
      }
      fragment.append(row);
    }
    rows.replaceChildren(fragment);
    status.textContent = `Rows ${window.offset + 1}–${window.offset + data.length} of ${totalRows.toLocaleString()}`;
  } catch {
    if (current === generation) status.textContent = 'Unable to load the synthetic result.';
  }
}

function schedule() {
  if (frame) cancelAnimationFrame(frame);
  frame = requestAnimationFrame(() => { frame = 0; void render(); });
}

document.querySelector('#show-result').addEventListener('click', () => {
  document.querySelector('#result').hidden = false;
  viewport.focus();
  schedule();
});
viewport.addEventListener('scroll', schedule, { passive: true });
new ResizeObserver(schedule).observe(viewport);
