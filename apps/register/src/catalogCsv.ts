import type { CatalogItem } from './types';
/** The Rust service owns CSV parsing and catalog validation. This module only bounds
 * browser reads and prevents asynchronous UI results from crossing review contexts. */
export const MAX_CATALOG_CSV_BYTES = 1024 * 1024;
export type CatalogCsvFile = { filename: string; content: string };
export type CatalogCsvPreview = {
  expected_config_revision: string; can_apply: boolean;
  rows: { row: number; line: number; sku: string; action: 'create' | 'update' | 'unchanged'; before: CatalogItem | null; after: CatalogItem }[];
  issues: { row: number; line: number; field: string; message: string }[];
  created: number; updated: number; unchanged: number;
};
export async function readCatalogCsv(file: Pick<File, 'name' | 'size' | 'arrayBuffer'>): Promise<CatalogCsvFile> {
  if (file.size > MAX_CATALOG_CSV_BYTES) throw new Error('Choose a CSV file no larger than 1 MiB.');
  const bytes = await file.arrayBuffer();
  if (bytes.byteLength > MAX_CATALOG_CSV_BYTES) throw new Error('Choose a CSV file no larger than 1 MiB.');
  try { return { filename: file.name, content: new TextDecoder('utf-8', { fatal: true }).decode(bytes) }; }
  catch { throw new Error('Choose a valid UTF-8 CSV file.'); }
}
export function createImportGuard() {
  let version = 0;
  let context = '';
  return {
    invalidate() { return ++version; },
    context(next: string) { if (next !== context) { context = next; version++; } },
    current(token: number) { return token === version; },
  };
}
export function canApplyCatalogCsv(preview: CatalogCsvPreview | null, revision: string, blocked: boolean): boolean {
  return !blocked && !!preview?.can_apply && preview.issues.length === 0 && preview.expected_config_revision === revision;
}

/** Exact text only: never normalize identifiers, names, weights or money for review. */
export function catalogReviewFields(item: CatalogItem): [string, string][] {
  return [
    ['SKU', item.sku], ['Name', item.name], ['Type', item.kind],
    ['Barcode', item.barcode || 'Not set'], ['Status', item.archived ? 'Archived' : 'Active'],
    ['Declared grams per package', item.declared_content_grams ?? 'Not applicable'],
    ['Pricing', item.pricing.kind === 'weight_group' ? `Weight group: ${item.pricing.group_id}` : `Fixed price: ${item.pricing.unit_price_minor} minor units`],
  ];
}

/** Refresh replaces the local draft, so it must never run over unsaved edits. */
export function canRefreshCatalogCsv(dirty: boolean, disabled: boolean, busy: boolean): boolean {
  return !dirty && !disabled && !busy;
}
