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
import { copyRequest, isSelected, selectionAfterClick, selectionLabel } from './selection.mjs';

const invoke = (...args) => globalThis.__TAURI__.core.invoke(...args);
const rowHeight = 36;
const $ = (selector) => document.querySelector(selector);
const viewport = $('#viewport');
const surface = $('#height');
const rows = $('#rows');
const status = $('#status');
const find = createFind();
const view = { total: 0, matched: null, page: 0, continuous: false, filterGeneration: 0, matchRow: null, selection: null };
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

function buildRow(record) {
  const row = document.createElement('div');
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
  for (const cell of record.cells) {
    const item = document.createElement('span');
    item.className = cellClass(cell);
    item.setAttribute('role', 'gridcell');
    item.textContent = displayCell(cell);
    row.append(item);
  }
  return row;
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
    if (current !== generation) return;
    rows.replaceChildren(...data.map(buildRow));
    const first = gridBase() + window.offset + 1;
    status.textContent = `Rows ${first.toLocaleString('en-US')}–${(first + data.length - 1).toLocaleString('en-US')} of ${datasetCount().toLocaleString('en-US')}`;
  } catch {
    if (current === generation) status.textContent = 'Unable to load the synthetic result.';
  }
}

function updateSelection(note = '') {
  $('#selection-status').textContent = note || selectionLabel(view.selection);
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
    updateSelection(`Copied ${(range.last - range.first + 1).toLocaleString('en-US')} rows as tab-separated text.`);
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
}

bindControls();
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
