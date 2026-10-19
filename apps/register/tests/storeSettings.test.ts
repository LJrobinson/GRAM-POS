import test from 'node:test';
import assert from 'node:assert/strict';
import { DEFAULT_STORE_SETTINGS, storeSettingsError } from '../src/storeSettings.ts';
import { reportBoundaryDateTime, reportDateTime, reportMatchesRange, todayReportRange, type SalesReport } from '../src/reports.ts';

test('store settings validate plain text limits without trimming stored values', () => {
  assert.equal(storeSettingsError(DEFAULT_STORE_SETTINGS), '');
  for (const [field, limit] of [['display_name', 120], ['receipt_header', 500], ['receipt_contact', 240], ['receipt_footer', 500]] as const) {
    assert.equal(storeSettingsError({ ...DEFAULT_STORE_SETTINGS, [field]: 'é'.repeat(limit) }), '');
    assert.match(storeSettingsError({ ...DEFAULT_STORE_SETTINGS, [field]: 'é'.repeat(limit + 1) }), /too long/);
    assert.match(storeSettingsError({ ...DEFAULT_STORE_SETTINGS, [field]: 'unsafe\ntext' }), /single-line/);
  }
  for (const char of ['\u2028', '\u2029', '\u202e', '\u2066']) assert.match(storeSettingsError({ ...DEFAULT_STORE_SETTINGS, receipt_header: `bad${char}text` }), /single-line/);
  assert.match(storeSettingsError({ ...DEFAULT_STORE_SETTINGS, display_name: '   ' }), /display name/);
  assert.match(storeSettingsError({ ...DEFAULT_STORE_SETTINGS, timezone: 'invalid zone' }), /IANA/);
});
test('today uses selected IANA zone and timestamps distinguish repeated DST hour', () => {
  assert.deepEqual(todayReportRange(new Date('2026-01-01T01:00:00Z'), 'America/New_York'), { start_date: '2025-12-31', end_date: '2026-01-01', timezone: 'America/New_York' });
  const a = reportDateTime(String(Date.parse('2026-11-01T05:30:00Z') / 1000), 'America/New_York');
  const b = reportDateTime(String(Date.parse('2026-11-01T06:30:00Z') / 1000), 'America/New_York');
  assert.match(a, /01:30:00 GMT-04:00/);
  assert.match(b, /01:30:00 GMT-05:00/);
});
test('report identity includes exact timezone and saved configuration revision', () => {
  const report = { start_date: '2026-01-01', end_date: '2026-01-02', timezone: 'America/New_York', config_revision: '4', groups: [], events: [] } satisfies SalesReport;
  assert.equal(reportMatchesRange(report, { ...report, expected_config_revision: '4' }), true);
  assert.equal(reportMatchesRange(report, { ...report, timezone: 'UTC' }), false);
  assert.equal(reportMatchesRange(report, { ...report, expected_config_revision: '5' }), false);
});

test('unsupported browser timezone falls back visibly to UTC and preserves saved zone label', () => {
  assert.equal(reportDateTime('1704067200', 'Future/Zone'), '2024-01-01 00:00:00 UTC (saved timezone Future/Zone unavailable in this browser)');
});

test('resolved UTC boundaries render signed Unix timestamps with symmetric limits', () => {
  assert.equal(reportBoundaryDateTime('-1'), '1969-12-31 23:59:59 UTC');
  assert.equal(reportBoundaryDateTime('-62135596800'), '0001-01-01 00:00:00 UTC');
  assert.equal(reportBoundaryDateTime('0'), '1970-01-01 00:00:00 UTC');
  assert.equal(reportBoundaryDateTime('-8640000000001'), 'Boundary time outside display range');
  assert.equal(reportBoundaryDateTime('8640000000001'), 'Boundary time outside display range');
  assert.equal(reportBoundaryDateTime('not-a-time'), 'Invalid boundary time');
  assert.equal(reportDateTime('-1'), 'Invalid recorded time');
});
