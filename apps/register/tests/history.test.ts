import test from 'node:test';
import assert from 'node:assert/strict';
import { filterSales } from '../src/history.ts';
import { reportDateTime, createReportRequestGuard } from '../src/reports.ts';
import type { SaleSummary } from '../src/types.ts';
const sale = (sale_id: string, receipt_number: string): SaleSummary => ({ sale_id, receipt_number, created_at_unix: '0', total_minor: '9007199254740993', tender_minor: '9007199254740993', change_minor: '0', currency: { code: 'USD', minor_unit_exponent: 2 }, line_count: 1 });
const sales = [sale('9007199254740993', 'GRAM-000012'), sale('9007199254740992', 'GRAM-000013'), sale('ABC-id', 'TRAIN.[01]')];
test('receipt search is trimmed, case-insensitive and literal for full or partial IDs', () => {
  assert.deepEqual(filterSales(sales, ' gram-000012 '), [sales[0]]);
  assert.deepEqual(filterSales(sales, '00001'), sales.slice(0, 2));
  assert.deepEqual(filterSales(sales, 'abc-ID'), [sales[2]]);
  assert.deepEqual(filterSales(sales, '[01]'), [sales[2]]);
  assert.deepEqual(filterSales(sales, '.*'), []);
  assert.deepEqual(filterSales(sales, '9007199254740993'), [sales[0]]);
});
test('blank search returns every sale in original order without changing snapshots', () => {
  const before = structuredClone(sales);
  assert.equal(filterSales(sales, ' \t'), sales);
  filterSales(sales, '000013');
  assert.deepEqual(sales, before);
  assert.deepEqual(filterSales([], ''), []);
});
test('search does not inspect amounts, currency, dates or product names', () => {
  assert.deepEqual(filterSales(sales, 'USD'), []);
  assert.deepEqual(filterSales(sales, 'unmatched'), []);
});
test('all loaded records are searchable, not just a recent subset', () => {
  const many = Array.from({ length: 1501 }, (_, i) => sale(String(i), `RECEIPT-${i}`));
  assert.deepEqual(filterSales(many, 'RECEIPT-1500'), [many[1500]]);
});
test('history timestamps explicitly use store timezone across midnight and DST', () => {
  const unix = (iso: string) => String(Date.parse(iso) / 1000);
  assert.match(reportDateTime(unix('2026-01-01T23:30:00Z'), 'Asia/Tokyo'), /^2026-01-02 08:30:00 GMT\+09:00 \[Asia\/Tokyo\]$/);
  assert.match(reportDateTime(unix('2026-11-01T05:30:00Z'), 'America/New_York'), /01:30:00 GMT-04:00/);
  assert.match(reportDateTime(unix('2026-11-01T06:30:00Z'), 'America/New_York'), /01:30:00 GMT-05:00/);
  assert.equal(reportDateTime('0'), '1970-01-01 00:00:00 UTC');
  assert.match(reportDateTime('0', 'Unavailable/Zone'), /UTC \(saved timezone Unavailable\/Zone unavailable in this browser\)/);
});
test('superseding search, refresh or navigation invalidates pending receipt responses', () => {
  const guard = createReportRequestGuard();
  const first = guard.begin();
  guard.invalidate();
  assert.equal(guard.isCurrent(first), false);
  const second = guard.begin();
  assert.equal(guard.isCurrent(second), true);
  guard.invalidate();
  assert.equal(guard.isCurrent(second), false);
});
