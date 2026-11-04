import { visibleWindow, displayCell, cellClass, windowTop, scrollTopForRow } from './grid.mjs';
import {
  PAGE_ROWS,
  FIND_CAP,
  acceptReply,
  compareSummaryText,
  createFind,
  filterRowState,
  filterStatus,
  overflowActions,
  pageCount,
  pagerState,
  parseWhere,
  progressText,
  rangeLabel,
  rowsFromQuery,
} from './strip.mjs';
import { commitKey, createEdits, editStatus, isEditable } from './edits.mjs';
import { copyRequest, isSelected, selectionAfterClick, selectionLabel } from './selection.mjs';
import { cursorLabel, moveCursor, scrollTopToReveal } from './cursor.mjs';
import { paletteMatches, paletteSelection } from './palette.mjs';

const invoke = (...args) => globalThis.__TAURI__.core.invoke(...args);
const rowHeight = 36;
const $ = (selector) => document.querySelector(selector);
const viewport = $('#viewport');
const surface = $('#height');
const rows = $('#rows');
const status = $('#status');
const find = createFind();
const edits = createEdits();
let editing = false;
const view = { total: 0, matched: null, page: 0, continuous: false, filterGeneration: 0, matchRow: null, selection: null, cursor: null };
let generation = 0;
let compareGeneration = 0;
let frame = 0;

const datasetCount = () => (view.matched ?? view.total);
const pages = () => pageCount(datasetCount());
const gridBase = () => (view.continuous ? 0 : view.page * PAGE_ROWS);
const gridCount = () => (view.continuous ? datasetCount() : Math.min(PAGE_ROWS, datasetCount() - view.page * PAGE_ROWS));

function fetchRows(offset, limit) {
  if (view.matched !== null) return invoke('filtered_rows', { generation: view.filterGeneration, offset, limit });
  return invoke('sample_rows', { offset, limit, total: view.total });
}

function buildRow(record, local) {
  const row = document.createElement('div');
  row.dataset.local = String(local);
  row.className = record.row_number === view.matchRow ? 'row match' : 'row';
  row.setAttribute('role', 'row');
  row.setAttribute('aria-rowindex', String(record.row_number + 1));
  if (record.row_number === view.matchRow) row.setAttribute('aria-selected', 'true');
  const number = document.createElement('span');
  number.className = 'cell';
  number.setAttribute('role', 'gridcell');
  number.textContent = String(record.row_number + 1);
  row.append(number);
  row.addEventListener('click', (event) => {
    view.selection = selectionAfterClick(view.selection, record.row_number, { shift: event.shiftKey && view.matched === null });
    updateSelection();
  });
  if (isSelected(view.selection, record.row_number)) {
    row.classList.add('selected');
    row.setAttribute('aria-selected', 'true');
  }
  record.cells.forEach((original, column) => {
    const edited = edits.get(record.row_number, column);
    const cell = edited ?? original;
    const item = document.createElement('span');
    item.className = edited ? `${cellClass(cell)} edited` : cellClass(cell);
    item.setAttribute('role', 'gridcell');
    item.textContent = displayCell(cell);
    if (view.cursor && view.cursor.row === local && view.cursor.column === column) {
      item.classList.add('active');
      item.setAttribute('aria-current', 'true');
    }
    if (isEditable(column, cell)) item.addEventListener('dblclick', () => startEdit(item, record.row_number, column, cell));
    row.append(item);
  });
  return row;
}

function startEdit(item, rowNumber, column, cell) {
  if (editing) return;
  editing = true;
  const input = document.createElement('input');
  input.className = 'entry cell-editor';
  input.value = cell.kind === 'null' ? '' : cell.value;
  input.setAttribute('aria-label', `Edit ${cell.kind} cell, Enter to apply, Escape to cancel, Ctrl+Backspace for NULL`);
  item.replaceChildren(input);
  input.focus();
  input.select();
  const finish = () => {
    editing = false;
    schedule();
  };
  const apply = async (asNull) => {
    try {
      const next = await invoke('edit_cell', { kind: cell.kind, text: input.value, asNull });
      edits.set(rowNumber, column, next);
      $('#selection-status').textContent = editStatus(edits.count());
      finish();
    } catch (error) {
      input.setAttribute('aria-invalid', 'true');
      $('#selection-status').textContent = `Not applied: ${error?.message ?? error}`;
    }
  };
  input.addEventListener('keydown', (event) => {
    const action = commitKey(event);
    if (action === null) return;
    event.preventDefault();
    event.stopPropagation();
    if (action === 'cancel') finish();
    else void apply(action === 'null');
  });
  input.addEventListener('blur', () => {
    if (editing) finish();
  });
}

async function render() {
  const current = ++generation;
  const window = visibleWindow(viewport.scrollTop, viewport.clientHeight, rowHeight, gridCount());
  surface.style.height = `${window.height}px`;
  rows.style.transform = `translateY(${windowTop(viewport.scrollTop, window.height, window.limit, rowHeight)}px)`;
  viewport.setAttribute('aria-rowcount', String(datasetCount()));
  if (!window.limit) {
    rows.replaceChildren();
    return;
  }
  try {
    const data = await fetchRows(gridBase() + window.offset, window.limit);
    if (current !== generation || editing) return;
    rows.replaceChildren(...data.map((record, index) => buildRow(record, window.offset + index)));
    const first = gridBase() + window.offset + 1;
    status.textContent = `Rows ${first.toLocaleString('en-US')}–${(first + data.length - 1).toLocaleString('en-US')} of ${datasetCount().toLocaleString('en-US')}`;
  } catch {
    if (current === generation) status.textContent = 'Unable to load the synthetic result.';
  }
}

function updateSelection(note = '') {
  $('#selection-status').textContent = note || selectionLabel(view.selection) || cursorLabel(view.cursor);
  schedule();
}

async function copySelection() {
  const range = copyRequest(view.selection);
  if (range === null) {
    updateSelection(view.selection ? 'Select at most 5,000 rows to copy.' : '');
    return;
  }
  try {
    const text = await invoke('copy_rows', { first: range.first, last: range.last, withHeaders: false, total: view.total });
    await writeClipboard(text);
    const note = edits.inRange(range.first, range.last) ? ' Edited cells are copied with their original values.' : '';
    updateSelection(`Copied ${(range.last - range.first + 1).toLocaleString('en-US')} rows as tab-separated text.${note}`);
  } catch (error) {
    updateSelection(`Unable to copy the selection: ${error?.message ?? error}`);
  }
}

async function writeClipboard(text) {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    copyThroughSelection(text);
  }
}

function copyThroughSelection(text) {
  const holder = document.createElement('textarea');
  holder.value = text;
  holder.setAttribute('readonly', '');
  holder.style.position = 'fixed';
  holder.style.opacity = '0';
  document.body.append(holder);
  holder.select();
  const copied = document.execCommand('copy');
  holder.remove();
  if (!copied) throw new Error('clipboard unavailable');
}

function schedule() {
  if (frame) cancelAnimationFrame(frame);
  frame = requestAnimationFrame(() => {
    frame = 0;
    void render();
  });
}

function updateStrip() {
  $('#progress').textContent = progressText({ status: 'Complete', rows: datasetCount(), pages: pages() });
  $('#range').textContent = view.continuous ? `Rows 1–${datasetCount().toLocaleString('en-US')}` : rangeLabel(view.page, datasetCount());
  const state = pagerState(view.page, pages());
  const enabled = !view.continuous;
  $('#page-first').disabled = !enabled || !state.first;
  $('#page-previous').disabled = !enabled || !state.previous;
  $('#page-next').disabled = !enabled || !state.next;
  $('#page-last').disabled = !enabled || !state.last;
  const filtering = view.matched !== null;
  $('#find').disabled = filtering;
  $('#find-previous').disabled = filtering;
  $('#find-next').disabled = filtering;
  $('#show-matching').disabled = filtering || !$('#find').value.trim();
  $('#find-status').textContent = filtering ? filterStatus({ matched: view.matched, total: view.total }) : find.status();
  const row = filterRowState({ whereText: $('#where').value, applied: filtering });
  $('#apply-filter').disabled = !row.applyEnabled;
  $('#clear-filter').disabled = !row.clearEnabled;
}

function goToPage(page) {
  view.page = Math.max(0, Math.min(page, pages() - 1));
  viewport.scrollTop = 0;
  updateStrip();
  schedule();
}

function jumpTo(row) {
  if (row === null) return;
  view.matchRow = row;
  if (!view.continuous) view.page = Math.floor(row / PAGE_ROWS);
  const local = view.continuous ? row : row - view.page * PAGE_ROWS;
  viewport.scrollTop = scrollTopForRow(local, viewport.clientHeight, rowHeight, gridCount());
  updateStrip();
  schedule();
}

async function runFind() {
  const query = $('#find').value.trim();
  const current = find.start(query);
  updateStrip();
  if (!query) return;
  try {
    const reply = await invoke('find_rows', { total: view.total, query, cap: FIND_CAP, generation: current });
    if (find.accept(reply)) jumpTo(find.current());
  } catch {
    find.fail(current);
  }
  updateStrip();
}

async function applyFilter(predicate) {
  const current = ++view.filterGeneration;
  try {
    const reply = await invoke('filter_rows', { total: view.total, predicate, generation: current });
    if (!acceptReply(view.filterGeneration, reply)) return;
    if (reply.matched === 0) {
      status.textContent = filterStatus({ matched: 0, total: view.total });
      view.matched = null;
    } else {
      view.matched = reply.matched;
      view.page = 0;
      view.matchRow = null;
    }
  } catch {
    status.textContent = 'The filter could not be applied.';
  }
  viewport.scrollTop = 0;
  updateStrip();
  schedule();
}

function clearFilter() {
  view.filterGeneration += 1;
  view.matched = null;
  view.page = 0;
  $('#where').value = '';
  viewport.scrollTop = 0;
  updateStrip();
  schedule();
}

function renderCompare(reply) {
  $('#compare-summary').textContent = `${compareSummaryText(reply.summary)}${reply.capped ? ' · showing the first 200 differences' : ''}`;
  const fragment = document.createDocumentFragment();
  for (const record of reply.rows) {
    const line = document.createElement('div');
    line.className = `diff ${record.change}`;
    line.append(...[record.change, ...record.cells.map(displayCell)].map((text) => {
      const item = document.createElement('span');
      item.textContent = text;
      return item;
    }));
    fragment.append(line);
  }
  $('#compare-rows').replaceChildren(fragment);
}

async function runCompare() {
  const current = ++compareGeneration;
  $('#compare-summary').textContent = 'Comparing…';
  try {
    const keyed = $('#compare-mode').value === 'keyed';
    const reply = await invoke('compare_rows', { total: view.total, keyed, generation: current });
    if (acceptReply(compareGeneration, reply)) renderCompare(reply);
  } catch {
    if (current === compareGeneration) $('#compare-summary').textContent = 'The comparison could not run.';
  }
}

function layoutOverflow() {
  const hidden = overflowActions($('#strip').clientWidth);
  const menu = $('#more-menu');
  for (const name of ['compare', 'export', 'continuous']) {
    const button = $(`#${name}`);
    const target = hidden.includes(name) ? menu : $('#actions');
    if (button.parentElement !== target) target.append(button);
  }
  $('#more').hidden = hidden.length === 0;
}

function bindControls() {
  $('#page-first').addEventListener('click', () => goToPage(0));
  $('#page-previous').addEventListener('click', () => goToPage(view.page - 1));
  $('#page-next').addEventListener('click', () => goToPage(view.page + 1));
  $('#page-last').addEventListener('click', () => goToPage(pages() - 1));
  $('#find').addEventListener('keydown', (event) => {
    if (event.key === 'Enter') void runFind();
  });
  $('#find').addEventListener('input', updateStrip);
  $('#find-next').addEventListener('click', () => jumpTo(find.next()));
  $('#find-previous').addEventListener('click', () => jumpTo(find.previous()));
  $('#show-matching').addEventListener('click', () => {
    const text = $('#find').value.trim();
    if (text) void applyFilter({ column: 'label', op: 'contains', text });
  });
  $('#continuous').addEventListener('click', (event) => {
    view.continuous = !view.continuous;
    event.currentTarget.setAttribute('aria-pressed', String(view.continuous));
    viewport.scrollTop = 0;
    updateStrip();
    schedule();
  });
  $('#export').addEventListener('click', () => {
    status.textContent = 'Synthetic prototype: nothing is exported.';
  });
  $('#compare').addEventListener('click', () => {
    $('#compare-panel').hidden = false;
    $('#more').open = false;
  });
  $('#compare-run').addEventListener('click', () => void runCompare());
  $('#compare-close').addEventListener('click', () => {
    $('#compare-panel').hidden = true;
  });
  bindFilterRow();
}

function bindFilterRow() {
  $('#where').addEventListener('input', updateStrip);
  $('#filter-row').addEventListener('submit', (event) => {
    event.preventDefault();
    const predicate = parseWhere($('#where').value);
    if (predicate) void applyFilter(predicate);
  });
  $('#clear-filter').addEventListener('click', clearFilter);
  $('#close-filter').addEventListener('click', () => {
    $('#filter-row').hidden = true;
  });
  for (const item of document.querySelectorAll('[data-saved]')) {
    item.addEventListener('click', () => {
      $('#where').value = item.dataset.saved;
      item.closest('details').open = false;
      updateStrip();
    });
  }
  document.addEventListener('keydown', (event) => {
    const typing = event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement;
    if (!typing && !$('#result').hidden && event.ctrlKey && event.key.toLowerCase() === 'c' && view.selection) {
      event.preventDefault();
      void copySelection();
    }
    if (!typing && event.key === 'Escape' && view.selection) {
      view.selection = null;
      updateSelection();
    }
    if (event.ctrlKey && event.key.toLowerCase() === 'f' && !$('#result').hidden) {
      event.preventDefault();
      $('#filter-row').hidden = false;
      $('#where').focus();
    }
  });
}

const cursorRows = () => Math.max(1, Math.floor(viewport.clientHeight / rowHeight));

function placeCursor(next) {
  view.cursor = next;
  if (next !== null) {
    viewport.scrollTop = scrollTopToReveal(next.row, viewport.scrollTop, viewport.clientHeight, rowHeight);
  }
  updateSelection();
}

function moveByKey(event) {
  const total = gridCount();
  if (event.key === 'ArrowDown' && view.cursor?.row === total - 1 && !view.continuous && view.page < pages() - 1) {
    goToPage(view.page + 1);
    placeCursor({ row: 0, column: view.cursor.column });
    return true;
  }
  if (event.key === 'ArrowUp' && view.cursor?.row === 0 && !view.continuous && view.page > 0) {
    goToPage(view.page - 1);
    placeCursor({ row: gridCount() - 1, column: view.cursor.column });
    return true;
  }
  const next = moveCursor(view.cursor, event.key, { rowCount: total, pageRows: cursorRows(), ctrl: event.ctrlKey });
  if (next === null) return false;
  placeCursor(next);
  return true;
}

function activeGlobalRow() {
  return view.cursor === null ? null : gridBase() + view.cursor.row;
}

function selectFromCursor(extend) {
  const row = activeGlobalRow();
  if (row === null || view.matched !== null) return;
  view.selection = selectionAfterClick(view.selection, row, { shift: extend });
  updateSelection();
}

function editActiveCell() {
  if (view.cursor === null) return;
  const row = rows.querySelector(`.row[data-local="${view.cursor.row}"]`);
  const cell = row?.querySelectorAll('.cell')[view.cursor.column + 1];
  cell?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
}

function bindGridKeys() {
  viewport.addEventListener('keydown', (event) => {
    if (editing || event.altKey || event.metaKey) return;
    if (event.key === ' ') {
      event.preventDefault();
      selectFromCursor(false);
    } else if (event.shiftKey && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
      event.preventDefault();
      if (moveByKey({ key: event.key, ctrlKey: false })) selectFromCursor(true);
    } else if (event.key === 'Enter' || event.key === 'F2') {
      event.preventDefault();
      editActiveCell();
    } else if (moveByKey(event)) {
      event.preventDefault();
    }
  });
  viewport.addEventListener('mousedown', (event) => {
    const item = event.target.closest?.('.cell');
    const row = event.target.closest?.('.row');
    if (!item || !row) return;
    const column = [...row.querySelectorAll('.cell')].indexOf(item) - 1;
    if (column >= 0) {
      view.cursor = { row: Number(row.dataset.local), column };
      updateSelection();
    }
  });
}

const palette = { index: 0, shown: [] };

function paletteActions() {
  const filtering = view.matched !== null;
  const hasSelection = view.selection !== null;
  return [
    { id: 'first', label: 'Go to first page', enabled: !view.continuous, run: () => goToPage(0) },
    { id: 'previous', label: 'Go to previous page', enabled: !view.continuous && view.page > 0, run: () => goToPage(view.page - 1) },
    { id: 'next', label: 'Go to next page', enabled: !view.continuous && view.page < pages() - 1, run: () => goToPage(view.page + 1) },
    { id: 'last', label: 'Go to last page', enabled: !view.continuous, run: () => goToPage(pages() - 1) },
    { id: 'find', label: 'Find in all rows', enabled: !filtering, run: () => $('#find').focus() },
    { id: 'filter', label: 'Filter rows (WHERE)', run: () => { $('#filter-row').hidden = false; $('#where').focus(); } },
    { id: 'clear-filter', label: 'Clear filter', enabled: filtering, run: clearFilter },
    { id: 'compare', label: 'Compare results', run: () => { $('#compare-panel').hidden = false; } },
    { id: 'continuous', label: view.continuous ? 'Switch to paged scrolling' : 'Switch to continuous scrolling', run: () => $('#continuous').click() },
    { id: 'copy', label: 'Copy selected rows', enabled: hasSelection, run: () => void copySelection() },
    { id: 'clear-selection', label: 'Clear row selection', enabled: hasSelection, run: () => { view.selection = null; updateSelection(); } },
    { id: 'edit', label: 'Edit the active cell', enabled: view.cursor !== null, run: editActiveCell },
  ];
}

function renderPalette() {
  const list = $('#palette-list');
  palette.shown = paletteMatches($('#palette-input').value, paletteActions());
  palette.index = paletteSelection(palette.index, palette.shown.length, '');
  list.replaceChildren(...palette.shown.map((action, index) => {
    const item = document.createElement('li');
    item.id = `palette-item-${index}`;
    item.setAttribute('role', 'option');
    item.setAttribute('aria-selected', String(index === palette.index));
    item.textContent = action.label;
    item.addEventListener('click', () => runPalette(index));
    return item;
  }));
  $('#palette-input').setAttribute('aria-activedescendant', palette.shown.length ? `palette-item-${palette.index}` : '');
  $('#palette-empty').hidden = palette.shown.length > 0;
}

function runPalette(index) {
  const action = palette.shown[index];
  if (!action) return;
  $('#palette').close();
  action.run();
}

function bindPalette() {
  const dialog = $('#palette');
  const input = $('#palette-input');
  document.addEventListener('keydown', (event) => {
    if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'p' && !$('#result').hidden) {
      event.preventDefault();
      input.value = '';
      palette.index = 0;
      renderPalette();
      dialog.showModal();
      input.focus();
    }
  });
  input.addEventListener('input', () => {
    palette.index = 0;
    renderPalette();
  });
  input.addEventListener('keydown', (event) => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      palette.index = paletteSelection(palette.index, palette.shown.length, event.key);
      renderPalette();
    } else if (event.key === 'Enter') {
      event.preventDefault();
      runPalette(palette.index);
    }
  });
  dialog.addEventListener('close', () => viewport.focus({ preventScroll: true }));
}

async function autoscroll(steps) {
  const started = performance.now();
  const height = () => Math.max(0, viewport.scrollHeight - viewport.clientHeight);
  for (let step = 0; step <= steps; step += 1) {
    viewport.scrollTop = (step / steps) * height();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    await new Promise((resolve) => setTimeout(resolve, 30));
  }
  await invoke('scroll_done', { steps, elapsedMs: Math.round(performance.now() - started) });
}

async function openResult(config) {
  view.total = rowsFromQuery(location.search, config.rows);
  $('#result-title').textContent = `${view.total.toLocaleString('en-US')} synthetic rows`;
  $('#result').hidden = false;
  $('#result').scrollIntoView({ block: 'start' });
  viewport.focus({ preventScroll: true });
  layoutOverflow();
  updateStrip();
  await render();
  if (config.selftest) await selfTest();
  if (config.autoscroll_steps > 0) await autoscroll(config.autoscroll_steps);
}

async function selfTest() {
  view.selection = selectionAfterClick(selectionAfterClick(null, 1), 3, { shift: true });
  updateSelection();
  await copySelection();
  await new Promise((resolve) => setTimeout(resolve, 300));
  const selected = document.querySelectorAll('.row.selected').length;
  await invoke('report', { line: `selected_rows_drawn=${selected} status=${$('#selection-status').textContent}` });
  await editSelfTest();
  await navigationSelfTest();
}

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function editSelfTest() {
  const cells = () => [...document.querySelectorAll('.row')].find((row) => row.getAttribute('aria-rowindex') === '3').querySelectorAll('.cell');
  const typeInto = async (column, text) => {
    cells()[column + 1].dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    const input = document.querySelector('.cell-editor');
    input.value = text;
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    await pause(300);
    return input;
  };
  const bad = await typeInto(3, '2026-99-99');
  const invalid = `${bad.getAttribute('aria-invalid')} ${$('#selection-status').textContent}`;
  bad.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
  await pause(300);
  const good = await typeInto(3, '2026-02-03');
  await pause(300);
  const edited = [...document.querySelectorAll('.cell.edited')].map((node) => node.textContent);
  await invoke('report', { line: `edit_invalid=${invalid} edit_applied=${edited.join('|')} editors_left=${document.querySelectorAll('.cell-editor').length} status=${$('#selection-status').textContent}` });
  return good;
}

async function navigationSelfTest() {
  const press = (target, key, extra = {}) => target.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...extra }));
  viewport.focus();
  press(viewport, 'ArrowDown');
  press(viewport, 'ArrowDown');
  press(viewport, 'ArrowRight');
  await pause(300);
  const active = document.querySelector('.cell.active');
  const where = active ? `${active.closest('.row').getAttribute('aria-rowindex')}/${[...active.parentElement.querySelectorAll('.cell')].indexOf(active)}` : 'none';
  press(viewport, ' ');
  await pause(100);
  const selected = document.querySelectorAll('.row.selected').length;
  press(document, 'P', { ctrlKey: true, shiftKey: true });
  const input = $('#palette-input');
  input.value = 'last page';
  input.dispatchEvent(new Event('input', { bubbles: true }));
  const offered = [...document.querySelectorAll('#palette-list li')].map((item) => item.textContent).join('|');
  press(input, 'Enter');
  await pause(500);
  await invoke('report', { line: `cursor_at=${where} space_selected=${selected} palette_offered=${offered} page_after=${view.page + 1}/${pages()} dialog_open=${$('#palette').open}` });
}

bindControls();
bindGridKeys();
bindPalette();
viewport.addEventListener('scroll', schedule, { passive: true });
new ResizeObserver(() => {
  layoutOverflow();
  schedule();
}).observe(viewport);
new ResizeObserver(layoutOverflow).observe($('#strip'));

let config = { rows: 1_000_000, autoscroll_steps: 0, selftest: false };
try {
  config = await invoke('startup_config');
} catch {
  config = { rows: 1_000_000, autoscroll_steps: 0, selftest: false };
}
$('#show-result').addEventListener('click', () => void openResult(config));
if (config.autoscroll_steps > 0 || config.selftest) void openResult(config);
