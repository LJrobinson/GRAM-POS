import { money, quantityLabel, ruleNames } from './format.ts';
import { DEFAULT_STORE_SETTINGS } from './storeSettings.ts';
import { reportDateTime } from './reports.ts';
import type { Receipt } from './types.ts';

/** Keep reprints independent of the device's locale and current time zone. */
export function receiptTimestamp(unix: string): string {
  if (!/^\d+$/.test(unix)) return 'Recorded time unavailable';
  const milliseconds = Number(unix) * 1000;
  if (!Number.isSafeInteger(milliseconds)) return 'Recorded time unavailable';
  const date = new Date(milliseconds);
  return Number.isNaN(date.getTime()) ? 'Recorded time unavailable' : `${date.toISOString().replace('T', ' ').replace('.000Z', '')} UTC`;
}

/**
 * The only input is the committed receipt. Never look up today's catalog,
 * currency, pricing schedule or refund ledger while rendering the original.
 * Values are formatted, not repriced; money remains exact integer strings.
 */
export function receiptView(receipt: Receipt) {
  const { quote } = receipt;
  const store = receipt.config_snapshot?.store_settings ?? { ...DEFAULT_STORE_SETTINGS, display_name: 'GRAM' };
  return {
    store,
    saleId: receipt.sale_id,
    number: receipt.receipt_number,
    createdAt: reportDateTime(receipt.created_at_unix, store.timezone),
    revision: quote.config_revision,
    currency: quote.currency.code,
    lines: quote.lines.map(line => ({
      id: line.id,
      sku: line.sku,
      name: line.name,
      quantity: quantityLabel(line.quantity, line.declared_content_grams),
      amount: money(line.amount_minor, quote.currency),
      unitPrice: line.explanation.kind === 'fixed_unit' && typeof line.explanation.unit_price_minor === 'string'
        ? money(line.explanation.unit_price_minor, quote.currency) : null,
    })),
    groups: quote.groups.map(group => ({
      id: group.group_id,
      name: group.schedule.name || group.group_id,
      eligible: `${group.eligible_grams} g eligible · ${group.selected_nominal_grams} g tier`,
      weights: `Measured: ${group.actual_measured_grams} g · Declared: ${group.declared_package_grams} g`,
      tolerance: `Tolerance: +${group.tolerance_grams} g · Billable: ${group.billable_grams} g`,
      rule: ruleNames[group.rule] || group.rule,
      amount: money(group.amount_minor, quote.currency),
    })),
    total: money(quote.total_minor, quote.currency),
    tender: money(receipt.tender_minor, quote.currency),
    change: money(receipt.change_minor, quote.currency),
  };
}

type PrintHost = {
  document: { getElementById(id: string): { isConnected: boolean; dataset: { saleId?: string } } | null };
  print(): void;
};

/**
 * Called only by the receipt's Print button. Tauri documents window.print()
 * as the cross-platform dialog route, including Windows WebView2. No printer
 * plugin, silent-print command, filesystem permission or mutation is involved.
 * Returning does not mean printed: cancel and print have the same void result.
 */
export function printSavedReceipt(saleId: string, host: PrintHost = window): void {
  const target = host.document.getElementById('print-receipt');
  if (!saleId || !target?.isConnected || target.dataset.saleId !== saleId) {
    throw new Error('Open the saved receipt again before printing.');
  }
  if (typeof host.print !== 'function') throw new Error('The print dialog is not available in this browser.');
  host.print();
}
