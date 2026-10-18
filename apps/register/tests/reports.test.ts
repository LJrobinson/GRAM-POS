import test from 'node:test';
import assert from 'node:assert/strict';
import {
  createReportRequestGuard, reportCount, reportCurrencyKey, reportDateTime,
  reportMatchesRange, reportRangeError, todayReportRange, validReportDate,
  type SalesReport,
} from '../src/reports.ts';
import { money } from '../src/format.ts';

const range = { start_date: '2026-10-18', end_date: '2026-10-19' };
const emptyReport: SalesReport = { ...range, timezone: 'UTC', groups: [], events: [] };

test('default range uses the UTC day, not the date in the supplied timezone', () => {
  assert.deepEqual(todayReportRange(new Date('2026-10-18T00:30:00+14:00')), { start_date: '2026-10-17', end_date: '2026-10-18' });
  assert.deepEqual(todayReportRange(new Date('2026-12-31T23:59:59Z')), { start_date: '2026-12-31', end_date: '2027-01-01' });
  assert.deepEqual(todayReportRange(new Date('2024-02-29T20:00:00Z')), { start_date: '2024-02-29', end_date: '2024-03-01' });
});

test('report dates follow strict Gregorian calendar and service year boundaries', () => {
  for (const value of ['0001-01-01', '9999-12-31', '2000-02-29', '2400-02-29', '2026-10-18']) assert.equal(validReportDate(value), true, value);
  for (const value of ['', '0000-01-01', '1900-02-29', '2100-02-29', '2026-02-29', '2026-04-31', '2026-13-01', '2026-01-00', '2026-1-01', '2026-01-1', '2026-01-01Z', ' 2026-01-01', '2026/01/01', '10000-01-01', '２０２６-01-01']) assert.equal(validReportDate(value), false, value);
});

test('report range requires a strictly later, exclusive end date', () => {
  assert.equal(reportRangeError(range), '');
  assert.match(reportRangeError({ start_date: range.start_date, end_date: range.start_date }), /after start date/);
  assert.match(reportRangeError({ start_date: range.end_date, end_date: range.start_date }), /end date is excluded/);
  assert.match(reportRangeError({ start_date: '', end_date: range.end_date }), /valid start and end/);
});

test('only an exact UTC range match can be shown or exported', () => {
  assert.equal(reportMatchesRange(emptyReport, range), true);
  assert.equal(reportMatchesRange(emptyReport, { ...range, start_date: '2026-10-17' }), false);
  assert.equal(reportMatchesRange(emptyReport, { ...range, end_date: '2026-10-20' }), false);
  assert.equal(reportMatchesRange({ ...emptyReport, timezone: 'local' as 'UTC' }, range), false);
});

test('same code with different stored decimal precision never shares a group key', () => {
  assert.equal(reportCurrencyKey({ code: 'USD', minor_unit_exponent: 2 }), 'USD-2');
  assert.notEqual(reportCurrencyKey({ code: 'USD', minor_unit_exponent: 2 }), reportCurrencyKey({ code: 'USD', minor_unit_exponent: 0 }));
  assert.notEqual(reportCurrencyKey({ code: 'USD', minor_unit_exponent: 2 }), reportCurrencyKey({ code: 'CAD', minor_unit_exponent: 2 }));
});

test('large signed net totals and counts are never rounded through Number', () => {
  assert.equal(money('-9007199254740993', { code: 'BHD', minor_unit_exponent: 3 }), '−BHD 9,007,199,254,740.993');
  assert.equal(reportCount('9007199254740993'), '9,007,199,254,740,993');
  assert.equal(reportCount('0001'), '1');
  assert.equal(reportCount('0'), '0');
  for (const value of ['NaN', '-1', '1e3', '1.2', '']) assert.equal(reportCount(value), '—');
});

test('event time is explicitly UTC and unsafe dates fail without throwing', () => {
  assert.equal(reportDateTime('0'), '1970-01-01 00:00:00 UTC');
  assert.equal(reportDateTime('1709164800'), '2024-02-29 00:00:00 UTC');
  assert.equal(reportDateTime('253402300799'), '9999-12-31 23:59:59 UTC');
  assert.equal(reportDateTime('9007199254740993'), 'Recorded time outside display range');
  for (const value of ['NaN', '1.1', '1e3', '-1', '']) assert.equal(reportDateTime(value), 'Invalid recorded time');
});

test('a newer load supersedes older success and error responses', () => {
  const requests = createReportRequestGuard();
  const first = requests.begin();
  const second = requests.begin();
  assert.equal(requests.isCurrent(first), false);
  assert.equal(requests.isCurrent(second), true);
});

test('editing boundaries or unmounting invalidates even a not-yet-returned first load', () => {
  const requests = createReportRequestGuard();
  const initial = requests.begin();
  requests.invalidate();
  assert.equal(requests.isCurrent(initial), false);
  const refreshed = requests.begin();
  assert.equal(requests.isCurrent(refreshed), true);
  requests.invalidate();
  assert.equal(requests.isCurrent(refreshed), false);
});

test('repeated exports cannot publish an old result after a new request', () => {
  const requests = createReportRequestGuard();
  const exported = requests.begin();
  requests.invalidate();
  const next = requests.begin();
  assert.equal(requests.isCurrent(exported), false);
  assert.equal(requests.isCurrent(next), true);
});
