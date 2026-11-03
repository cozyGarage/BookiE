import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  PAGE_ROWS,
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

test('progress text mirrors the GTK stream header', () => {
  assert.equal(progressText({ status: 'Complete', rows: 150_000, pages: 30 }), 'Complete · Received 150,000 rows · 30 pages');
  assert.equal(progressText({ status: 'Receiving…', rows: 1, pages: 1 }), 'Receiving… · Received 1 row · 1 page');
});

test('range labels and pager state cover first, middle, last and empty results', () => {
  assert.equal(rangeLabel(0, 150_000), 'Rows 1–5,000');
  assert.equal(rangeLabel(29, 150_000), 'Rows 145,001–150,000');
  assert.equal(rangeLabel(0, 0), 'Rows 0');
  assert.equal(pageCount(150_000), 30);
  assert.equal(pageCount(PAGE_ROWS + 1), 2);
  assert.deepEqual(pagerState(0, 30), { first: false, previous: false, next: true, last: true });
  assert.deepEqual(pagerState(29, 30), { first: true, previous: true, next: false, last: false });
  assert.deepEqual(pagerState(0, 1), { first: false, previous: false, next: false, last: false });
});

test('narrow strips move the secondary actions into the overflow menu', () => {
  assert.deepEqual(overflowActions(920), []);
  assert.deepEqual(overflowActions(600), []);
  assert.deepEqual(overflowActions(599), ['compare', 'export', 'continuous']);
});

test('find reports the position and wraps in both directions', () => {
  const find = createFind();
  const generation = find.start('sample');
  assert.equal(find.status(), 'Searching…');
  assert.ok(find.accept({ generation, matches: [10, 20, 30], capped: false }));
  assert.equal(find.status(), 'Match 1 of 3');
  assert.equal(find.current(), 10);
  assert.equal(find.next(), 20);
  assert.equal(find.next(), 30);
  assert.equal(find.next(), 10);
  assert.equal(find.previous(), 30);
  assert.equal(find.status(), 'Match 3 of 3');
});

test('a stale find reply is dropped after a newer search starts', () => {
  const find = createFind();
  const stale = find.start('a');
  const fresh = find.start('b');
  assert.equal(find.accept({ generation: stale, matches: [1], capped: false }), false);
  assert.equal(find.status(), 'Searching…');
  assert.ok(find.accept({ generation: fresh, matches: [2], capped: false }));
  assert.equal(find.current(), 2);
  assert.equal(find.fail(stale), false);
});

test('a capped or empty find says so and has no current match', () => {
  const find = createFind();
  let generation = find.start('x');
  find.accept({ generation, matches: [], capped: false });
  assert.equal(find.status(), 'No match');
  assert.equal(find.current(), null);
  assert.equal(find.next(), null);
  generation = find.start('y');
  find.accept({ generation, matches: Array.from({ length: 10_000 }, (_, index) => index), capped: true });
  assert.equal(find.status(), 'Match 1 of 10,000+ (list capped)');
  assert.equal(find.start(''), generation + 1);
  assert.equal(find.status(), '');
});

test('a failed search is reported only for the current generation', () => {
  const find = createFind();
  const generation = find.start('x');
  assert.ok(find.fail(generation));
  assert.equal(find.status(), 'Search failed');
});

test('where text parses into a typed predicate and refuses anything else', () => {
  assert.deepEqual(parseWhere('label contains 7'), { column: 'label', op: 'contains', text: '7' });
  assert.deepEqual(parseWhere('state != true'), { column: 'state', op: 'not_equals', text: 'true' });
  assert.deepEqual(parseWhere('VALUE = 9007199254740993'), { column: 'value', op: 'equals', text: '9007199254740993' });
  assert.equal(parseWhere('label like 7'), null);
  assert.equal(parseWhere('drop table x'), null);
  assert.equal(parseWhere(''), null);
});

test('filter row enables Apply for a valid expression and Clear when something is set', () => {
  assert.deepEqual(filterRowState({ whereText: '', applied: false }), { applyEnabled: false, clearEnabled: false });
  assert.deepEqual(filterRowState({ whereText: 'label contains 7', applied: false }), { applyEnabled: true, clearEnabled: true });
  assert.deepEqual(filterRowState({ whereText: '', applied: true }), { applyEnabled: false, clearEnabled: true });
  assert.equal(filterStatus({ matched: 0, total: 150_000 }), 'No rows match');
  assert.equal(filterStatus({ matched: 11_111, total: 150_000 }), 'Showing 11,111 of 150,000 rows');
});

test('compare summaries name the mode and only keyed compares count changed rows', () => {
  const whole = { mode: 'whole_row', equal: 147_000, added: 1_500, removed: 3_000, changed: 0 };
  assert.equal(compareSummaryText(whole), 'Whole-row compare: 147,000 equal · 1,500 added · 3,000 removed');
  const keyed = { mode: 'keyed', equal: 147_000, added: 0, removed: 1_500, changed: 1_500 };
  assert.equal(compareSummaryText(keyed), 'Keyed compare (value): 147,000 equal · 0 added · 1,500 removed · 1,500 changed');
  assert.ok(acceptReply(3, { generation: 3 }));
  assert.equal(acceptReply(3, { generation: 2 }), false);
});

test('the synthetic row count comes from a listed size only', () => {
  assert.equal(rowsFromQuery('?rows=150000', 1_000_000), 150_000);
  assert.equal(rowsFromQuery('?rows=42', 1_000_000), 1_000_000);
  assert.equal(rowsFromQuery('', 500_000), 500_000);
});
