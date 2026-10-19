import test from 'node:test';
import assert from 'node:assert/strict';
import { canRefreshCatalogCsv, catalogReviewFields, readCatalogCsv, MAX_CATALOG_CSV_BYTES, createImportGuard, canApplyCatalogCsv, type CatalogCsvPreview } from '../src/catalogCsv.ts';
const valid: CatalogCsvPreview = { expected_config_revision: '7', can_apply: true, rows: [{row: 1, line: 2, sku: '001', action: 'create', before: null, after: {sku: '001', name: 'Soap', kind: 'unit', declared_content_grams: null, pricing: {kind: 'fixed_unit', unit_price_minor: '250'}}}], issues: [], created: 1, updated: 0, unchanged: 0 };
const file = (bytes: Uint8Array, size = bytes.byteLength) => ({ name: 'products.csv', size, arrayBuffer: async () => bytes.buffer as ArrayBuffer });
test('bounded reader preserves exact UTF-8 CSV without parsing or changing identity', async () => {
  const csv = 'format,sku,name\r\ngram_catalog_v1,text:001,text:Éclair\r\n';
  assert.deepEqual(await readCatalogCsv(file(new TextEncoder().encode(csv))), { filename: 'products.csv', content: csv });
});
test('reader refuses oversized file before reading it', async () => {
  let read = false;
  await assert.rejects(readCatalogCsv({ name: 'huge.csv', size: MAX_CATALOG_CSV_BYTES + 1, arrayBuffer: async () => { read = true; return new ArrayBuffer(0); } }), /1 MiB/);
  assert.equal(read, false);
});
test('reader checks actual bytes too, allows exact boundary and rejects malformed UTF-8', async () => {
  await assert.rejects(readCatalogCsv(file(new Uint8Array(MAX_CATALOG_CSV_BYTES + 1), 0)), /1 MiB/);
  assert.equal((await readCatalogCsv(file(new Uint8Array(MAX_CATALOG_CSV_BYTES).fill(65)))).content.length, MAX_CATALOG_CSV_BYTES);
  await assert.rejects(readCatalogCsv(file(new Uint8Array([0xc3, 0x28]))), /UTF-8/);
});
test('apply requires explicit valid review at current revision and a clean editable configuration', () => {
  assert.equal(canApplyCatalogCsv(valid, '7', false), true);
  assert.equal(canApplyCatalogCsv(null, '7', false), false);
  assert.equal(canApplyCatalogCsv(valid, '8', false), false);
  assert.equal(canApplyCatalogCsv(valid, '7', true), false);
  assert.equal(canApplyCatalogCsv({...valid, can_apply: false}, '7', false), false);
  assert.equal(canApplyCatalogCsv({...valid, issues: [{row: 1, line: 2, field: 'sku', message: 'Duplicate'}]}, '7', false), false);
});
test('new file selection and cancel reject out-of-order read, preview and error results', () => {
  const guard = createImportGuard(); guard.context('7:false:false');
  const first = guard.invalidate(); const second = guard.invalidate();
  assert.equal(guard.current(first), false); assert.equal(guard.current(second), true);
  guard.invalidate(); assert.equal(guard.current(second), false);
});
test('revision changes, unsaved edits and saves invalidate synchronously, even if context returns to original', () => {
  const guard = createImportGuard(); guard.context('7:false:false');
  const request = guard.invalidate(); guard.context('7:true:false');
  assert.equal(guard.current(request), false);
  guard.context('7:false:false'); assert.equal(guard.current(request), false);
  const next = guard.invalidate(); guard.context('8:false:false'); assert.equal(guard.current(next), false);
  const save = guard.invalidate(); guard.context('8:false:true'); assert.equal(guard.current(save), false);
});
test('unchanged context keeps a current review valid and an apply consumes it', () => {
  const guard = createImportGuard(); guard.context('7:false:false'); const review = guard.invalidate();
  guard.context('7:false:false'); assert.equal(guard.current(review), true);
  guard.invalidate(); assert.equal(guard.current(review), false);
});

test('product review preserves multiline names, exact barcode and high-precision money strings', () => {
  const fields = Object.fromEntries(catalogReviewFields({sku: '001', name: 'Fresh\nSoap, "gentle"', barcode: '000123', archived: true, kind: 'unit', declared_content_grams: null, pricing: {kind: 'fixed_unit', unit_price_minor: '18446744073709551615'}}));
  assert.equal(fields.Name, 'Fresh\nSoap, "gentle"');
  assert.equal(fields.Barcode, '000123');
  assert.equal(fields.Status, 'Archived');
  assert.equal(fields.Pricing, 'Fixed price: 18446744073709551615 minor units');
  assert.equal(fields['Declared grams per package'], 'Not applicable');
});
test('product review explicitly shows group reference and declared package content', () => {
  const fields = Object.fromEntries(catalogReviewFields({sku: 'PACK', name: 'Pack', kind: 'prepacked', declared_content_grams: '3.125', pricing: {kind: 'weight_group', group_id: 'group:001'}}));
  assert.equal(fields.Pricing, 'Weight group: group:001');
  assert.equal(fields['Declared grams per package'], '3.125');
  assert.equal(fields.Status, 'Active');
});

test('recovery refresh cannot overwrite unsaved edits after an uncertain apply and failed automatic refresh', () => {
  assert.equal(canRefreshCatalogCsv(false, false, false), true);
  // A failed recovery leaves the editor usable; subsequent local edits must block replacement.
  assert.equal(canRefreshCatalogCsv(true, false, false), false);
  assert.equal(canRefreshCatalogCsv(false, true, false), false);
  assert.equal(canRefreshCatalogCsv(false, false, true), false);
  // Only a deliberate save or discard makes replacement safe again.
  assert.equal(canRefreshCatalogCsv(false, false, false), true);
});
