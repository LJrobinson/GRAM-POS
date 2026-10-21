import type { SaleSummary } from './types.ts';

/** Literal, case-insensitive substring search. Never coerce identifiers to numbers. */
export function filterSales(sales: readonly SaleSummary[], query: string): readonly SaleSummary[] {
  const needle = query.trim().toLowerCase();
  return needle ? sales.filter(sale => sale.receipt_number.toLowerCase().includes(needle) || sale.sale_id.toLowerCase().includes(needle)) : sales;
}
