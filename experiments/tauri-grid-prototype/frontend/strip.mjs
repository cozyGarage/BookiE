export const PAGE_ROWS = 5000;
export const ALLOWED_ROWS = [150_000, 500_000, 1_000_000];
export const OVERFLOW_WIDTH = 600;
export const FIND_CAP = 10_000;
const OVERFLOW_ACTIONS = ['compare', 'export', 'continuous'];

const count = (value) => Number(value).toLocaleString('en-US');

export function rowsFromQuery(search, fallback) {
  const requested = Number(new URLSearchParams(search).get('rows'));
  return ALLOWED_ROWS.includes(requested) ? requested : fallback;
}

export function progressText({ status, rows, pages }) {
  const rowWord = rows === 1 ? 'row' : 'rows';
  const pageWord = pages === 1 ? 'page' : 'pages';
  return `${status} · Received ${count(rows)} ${rowWord} · ${count(pages)} ${pageWord}`;
}

export function pageCount(total) {
  return Math.ceil(Math.max(0, total) / PAGE_ROWS);
}

export function rangeLabel(page, total) {
  const start = page * PAGE_ROWS + 1;
  if (total <= 0 || start > total) return 'Rows 0';
  const end = Math.min(total, (page + 1) * PAGE_ROWS);
  return `Rows ${count(start)}–${count(end)}`;
}

export function pagerState(page, pages) {
  const hasPrevious = page > 0;
  const hasNext = page < pages - 1;
  return { first: hasPrevious, previous: hasPrevious, next: hasNext, last: hasNext };
}

export function overflowActions(stripWidth) {
  return stripWidth < OVERFLOW_WIDTH ? [...OVERFLOW_ACTIONS] : [];
}

export function createFind() {
  let generation = 0;
  let state = idle('');
  function idle(query) {
    return { phase: 'idle', query, matches: [], capped: false, index: -1 };
  }
  function step(delta) {
    if (state.matches.length === 0) return null;
    state.index = (state.index + delta + state.matches.length) % state.matches.length;
    return state.matches[state.index];
  }
  return {
    get generation() {
      return generation;
    },
    start(query) {
      generation += 1;
      state = query ? { ...idle(query), phase: 'searching' } : idle('');
      return generation;
    },
    accept(reply) {
      if (reply.generation !== generation || state.phase !== 'searching') return false;
      const matches = reply.matches.slice(0, FIND_CAP);
      state = { ...state, phase: 'done', matches, capped: Boolean(reply.capped), index: matches.length ? 0 : -1 };
      return true;
    },
    fail(failedGeneration) {
      if (failedGeneration !== generation) return false;
      state = { ...state, phase: 'error' };
      return true;
    },
    next: () => step(1),
    previous: () => step(-1),
    current: () => (state.index >= 0 ? state.matches[state.index] : null),
    status() {
      if (state.phase === 'searching') return 'Searching…';
      if (state.phase === 'error') return 'Search failed';
      if (state.phase === 'idle') return '';
      if (state.matches.length === 0) return 'No match';
      const total = `${count(state.matches.length)}${state.capped ? '+' : ''}`;
      const suffix = state.capped ? ' (list capped)' : '';
      return `Match ${state.index + 1} of ${total}${suffix}`;
    },
  };
}

export function parseWhere(text) {
  const match = /^\s*(value|label|state)\s+(contains|=|!=)\s+(.+?)\s*$/i.exec(text ?? '');
  if (!match) return null;
  const op = { contains: 'contains', '=': 'equals', '!=': 'not_equals' }[match[2].toLowerCase()];
  return { column: match[1].toLowerCase(), op, text: match[3] };
}

export function filterRowState({ whereText, applied }) {
  const hasText = Boolean(whereText && whereText.trim());
  return { applyEnabled: parseWhere(whereText) !== null, clearEnabled: applied || hasText };
}

export function filterStatus({ matched, total }) {
  if (matched === 0) return 'No rows match';
  return `Showing ${count(matched)} of ${count(total)} rows`;
}

export function compareSummaryText(summary) {
  const label = summary.mode === 'keyed' ? 'Keyed compare (value)' : 'Whole-row compare';
  const parts = [`${count(summary.equal)} equal`, `${count(summary.added)} added`, `${count(summary.removed)} removed`];
  if (summary.mode === 'keyed') parts.push(`${count(summary.changed)} changed`);
  return `${label}: ${parts.join(' · ')}`;
}

export function acceptReply(currentGeneration, reply) {
  return reply.generation === currentGeneration;
}
