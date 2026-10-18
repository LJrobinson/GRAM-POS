import test from 'node:test';
import assert from 'node:assert/strict';
import { activeCatalog, newGroup, newProduct } from '../src/catalog.ts';

test('archive flags hide products without changing legacy or active records', () => {
  const legacy = { ...newProduct(), sku: 'legacy', archived: undefined };
  const active = { ...newProduct(), sku: 'active' };
  const archived = { ...newProduct(), sku: 'archived', archived: true };
  const all = [legacy, active, archived];
  assert.deepEqual(activeCatalog(all), [legacy, active]);
  assert.equal(all.length, 3);
  assert.equal(activeCatalog([{ ...archived, archived: false }]).length, 1);
});

test('empty catalog is supported', () => assert.deepEqual(activeCatalog([]), []));

test('new product has no assumed name, group, price or measured quantity', () => {
  assert.deepEqual(newProduct(), { sku: '', name: '', kind: 'measured', declared_content_grams: null, pricing: { kind: 'weight_group', group_id: '' }, archived: false });
});

test('new group requires explicit prices, tolerances and policy choices', () => {
  const group = newGroup();
  assert.equal(group.overage_policy, '');
  assert.equal(group.rounding, '');
  assert.equal(group.below_smallest, '');
  assert.equal(group.tolerance_scope, 'aggregate_pricing_group');
  assert.deepEqual(group.tiers, [{ nominal_grams: '', tolerance_grams: '', price_minor: '' }]);
});

test('form drafts do not share mutable arrays or product pricing objects', () => {
  const group = newGroup(); group.tiers[0].price_minor = '9007199254740993';
  assert.equal(newGroup().tiers[0].price_minor, '');
  const product = newProduct(); product.pricing = { kind: 'fixed_unit', unit_price_minor: '9007199254740993' };
  assert.equal(newProduct().pricing.kind, 'weight_group');
});
