import type { Currency } from './types.ts';

export type SalesReportRange = { start_date: string; end_date: string };
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
};
export type SalesReport = SalesReportRange & {
  timezone: 'UTC';
  groups: SalesReportGroup[];
  events: SalesReportEvent[];
};

/** A date-only range is always UTC, independent of the register's local timezone. */
export function todayReportRange(now = new Date()): SalesReportRange {
  const start = new Date(now);
  start.setUTCHours(0, 0, 0, 0);
  const end = new Date(start);
  end.setUTCDate(end.getUTCDate() + 1);
  return { start_date: start.toISOString().slice(0, 10), end_date: end.toISOString().slice(0, 10) };
}

export function validReportDate(value: string): boolean {
  if (!/^(?!0000)\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const date = new Date(`${value}T00:00:00.000Z`);
  return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value;
}

export function reportRangeError(range: SalesReportRange): string {
  if (!validReportDate(range.start_date) || !validReportDate(range.end_date)) {
    return 'Choose valid start and end dates (years 0001–9999). Both boundaries are midnight UTC.';
  }
  if (range.start_date >= range.end_date) return 'End date must be after start date. The end date is excluded.';
  return '';
}

export function reportMatchesRange(report: SalesReport, range: SalesReportRange): boolean {
  return report.timezone === 'UTC' && report.start_date === range.start_date && report.end_date === range.end_date;
}

/** Different stored precisions are distinct even when their currency codes match. */
export function reportCurrencyKey(currency: Currency): string {
  return `${currency.code}-${currency.minor_unit_exponent}`;
}

export function reportCount(value: string): string {
  return /^\d+$/.test(value) ? BigInt(value).toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',') : '—';
}

/** Convert only bounded date values to Number; monetary values never pass through it. */
export function reportDateTime(unix: string): string {
  if (!/^\d+$/.test(unix)) return 'Invalid recorded time';
  const milliseconds = BigInt(unix) * 1000n;
  if (milliseconds > 8640000000000000n) return 'Recorded time outside display range';
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
