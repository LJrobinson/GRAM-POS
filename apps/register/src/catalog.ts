import type { CatalogItem, GroupConfig } from './types.ts';

/** Legacy catalog rows without an archive flag remain available. */
export function activeCatalog(items: CatalogItem[]): CatalogItem[] {
  return items.filter(item => !item.archived);
}

/** Incomplete form drafts are never persisted without Rust validation. */
export function newProduct(): CatalogItem {
  return { sku: '', name: '', kind: 'measured', declared_content_grams: null, pricing: { kind: 'weight_group', group_id: '' }, archived: false };
}
export function newGroup(): GroupConfig {
  return { id: '', name: '', tiers: [{ nominal_grams: '', tolerance_grams: '', price_minor: '' }], overage_policy: '' as GroupConfig['overage_policy'], rounding: '' as GroupConfig['rounding'], below_smallest: '' as GroupConfig['below_smallest'], tolerance_scope: 'aggregate_pricing_group' };
}
