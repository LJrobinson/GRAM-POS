import { validTimezone } from './storeSettings.ts';
import type { Currency } from './types.ts';

export type SalesReportRange = { start_date: string; end_date: string; timezone?: string; expected_config_revision?: string };
export type SalesReportGroup = {
  currency: Currency;
  gross_sales_minor: string;
  refunds_minor: string;
  net_sales_minor: string;
  sale_count: string;
  refund_count: string;
};
export type SalesReportEvent = {
  kind: 'sale' | 'refund';
  event_id: string;
  sale_id: string;
  created_at_unix: string;
  currency: Currency;
  total_minor: string;
  session_id: string | null;
  reason: string | null;
  created_at_local?: string;
  utc_offset_seconds?: string;
};
export type SalesReport = SalesReportRange & {
  timezone: string;
  start_at_unix?: string;
  end_at_unix?: string;
  config_revision?: string;
  groups: SalesReportGroup[];
  events: SalesReportEvent[];
};

/** Legacy callers default to UTC; explicit zones use their local calendar date. */
export function todayReportRange(now = new Date(), timezone = 'UTC'): SalesReportRange {
  let formatter: Intl.DateTimeFormat;
  try { formatter = new Intl.DateTimeFormat('en-CA', { timeZone: timezone, year: 'numeric', month: '2-digit', day: '2-digit' }); }
  catch { throw new Error('This browser cannot calculate Today in that timezone. Enter reporting dates manually.'); }
  const local = formatter.formatToParts(now);
  const part = (type: string) => local.find(item => item.type === type)!.value;
  const start = new Date(`${part('year')}-${part('month')}-${part('day')}T00:00:00.000Z`);
  start.setUTCHours(0, 0, 0, 0);
  const end = new Date(start);
  end.setUTCDate(end.getUTCDate() + 1);
  return { start_date: start.toISOString().slice(0, 10), end_date: end.toISOString().slice(0, 10), ...(timezone === 'UTC' ? {} : { timezone }) };
}

export function validReportDate(value: string): boolean {
  if (!/^(?!0000)\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const date = new Date(`${value}T00:00:00.000Z`);
  return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value;
}

export function reportRangeError(range: SalesReportRange): string {
  if (!validTimezone(range.timezone ?? 'UTC')) return 'Enter a valid IANA report timezone, such as America/New_York or UTC.';
  if (!validReportDate(range.start_date) || !validReportDate(range.end_date)) {
    return 'Choose valid start and end dates (years 0001–9999). Both boundaries are midnight in the selected timezone.';
  }
  if (range.start_date >= range.end_date) return 'End date must be after start date. The end date is excluded.';
  return '';
}

export function reportMatchesRange(report: SalesReport, range: SalesReportRange): boolean {
  return report.timezone === (range.timezone ?? 'UTC') && report.start_date === range.start_date && report.end_date === range.end_date && (!range.expected_config_revision || report.config_revision === range.expected_config_revision);
}

/** Different stored precisions are distinct even when their currency codes match. */
export function reportCurrencyKey(currency: Currency): string {
  return `${currency.code}-${currency.minor_unit_exponent}`;
}

export function reportCount(value: string): string {
  return /^\d+$/.test(value) ? BigInt(value).toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',') : '—';
}

/** Resolved date-range bounds can precede 1970, unlike stored sale timestamps. */
export function reportBoundaryDateTime(unix: string): string {
  if (!/^-?\d+$/.test(unix)) return 'Invalid boundary time';
  const milliseconds = BigInt(unix) * 1000n;
  if (milliseconds < -8640000000000000n || milliseconds > 8640000000000000n) return 'Boundary time outside display range';
  return `${new Date(Number(milliseconds)).toISOString().replace('T', ' ').replace('.000Z', '')} UTC`;
}

/** Convert only bounded date values to Number; monetary values never pass through it. */
export function reportDateTime(unix: string, timezone = 'UTC'): string {
  if (!/^\d+$/.test(unix)) return 'Invalid recorded time';
  const milliseconds = BigInt(unix) * 1000n;
  if (milliseconds > 8640000000000000n) return 'Recorded time outside display range';
  if (timezone !== 'UTC') {
    try {
      const date = new Date(Number(milliseconds));
      const parts = new Intl.DateTimeFormat('en-CA', { timeZone: timezone, year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23', timeZoneName: 'longOffset' }).formatToParts(date);
      const part = (type: string) => parts.find(item => item.type === type)?.value ?? '';
      return `${part('year')}-${part('month')}-${part('day')} ${part('hour')}:${part('minute')}:${part('second')} ${part('timeZoneName')} [${timezone}]`;
    } catch { return `${reportDateTime(unix)} (saved timezone ${timezone} unavailable in this browser)`; }
  }
  return `${new Date(Number(milliseconds)).toISOString().replace('T', ' ').replace('.000Z', '')} UTC`;
}

/** Old loads, edits and unmounted views cannot publish a response or export notice. */
export function createReportRequestGuard() {
  let revision = 0;
  return {
    begin: () => ++revision,
    invalidate: () => { revision += 1; },
    isCurrent: (request: number) => request === revision,
  };
}
