import test from 'node:test';
import assert from 'node:assert/strict';
import { barcodeError, createScanQueue, type BarcodeMatch } from '../src/scanner.ts';
import type { CatalogItem } from '../src/types.ts';
const item: CatalogItem = { sku: 'SOAP', name: 'Soap', kind: 'unit', declared_content_grams: null, barcode: '00123', pricing: { kind: 'fixed_unit', unit_price_minor: '125' } };
const match: BarcodeMatch = { item, config_revision: '1' };
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (error: Error) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function recorder(keep = true) { const added: CatalogItem[] = []; const errors: string[] = []; let finished = 0; return { added, errors, get finished() { return finished; }, handlers: { current: () => true, matched: (product: CatalogItem) => { added.push(product); return keep; }, failed: (error: Error) => { errors.push(error.message); }, finished: () => { finished++; } } }; }

test('barcodes preserve zeros and case; validation is byte-based and rejects controls without trimming', () => {
  for (const value of ['00123', 'abc', 'ABC', 'a b', 'é'.repeat(64)]) assert.equal(barcodeError(value), '');
  for (const value of ['', ' 00123', '00123 ', 'a\nb', 'a\tb', 'a\u007fb', 'a\u0085b', 'é'.repeat(65)]) assert.notEqual(barcodeError(value), '');
});
test('deliberate repeated scans queue once each, preserving exact leading zeros', async () => {
  const first = deferred<BarcodeMatch>(); const requested: string[] = [];
  const queue = createScanQueue(barcode => { requested.push(barcode); return requested.length === 1 ? first.promise : Promise.resolve(match); });
  const result = recorder();
  const a = queue.submit('00123', '1', result.handlers); const b = queue.submit('00123', '1', result.handlers);
  await Promise.resolve(); assert.deepEqual(requested, ['00123']);
  first.resolve(match); await Promise.all([a, b]);
  assert.deepEqual(requested, ['00123', '00123']); assert.equal(result.added.length, 2); assert.equal(result.finished, 2);
});
test('cancel suppresses pending and queued results while a new scan can proceed', async () => {
  const old = deferred<BarcodeMatch>(); let calls = 0;
  const queue = createScanQueue(() => ++calls === 1 ? old.promise : Promise.resolve(match)); const result = recorder();
  const a = queue.submit('00123', '1', result.handlers); const b = queue.submit('00123', '1', result.handlers);
  await Promise.resolve(); queue.cancel();
  await queue.submit('00123', '1', result.handlers); old.resolve(match); await Promise.all([a, b]);
  assert.equal(calls, 2); assert.equal(result.added.length, 1); assert.equal(result.finished, 1);
});
test('navigation cancellation also suppresses late errors', async () => {
  const old = deferred<BarcodeMatch>(); const queue = createScanQueue(() => old.promise); const result = recorder();
  const task = queue.submit('00123', '1', result.handlers); await Promise.resolve(); queue.cancel(); old.reject(new Error('Old failure')); await task;
  assert.deepEqual(result.errors, []); assert.equal(result.finished, 0);
});
test('measured weight prompt invalidates queued scans rather than opening hidden prompts', async () => {
  let calls = 0; const measured = { ...item, kind: 'measured' as const, pricing: { kind: 'weight_group' as const, group_id: 'weight' } };
  const queue = createScanQueue(async () => { calls++; return { ...match, item: measured }; }); const result = recorder(false);
  await Promise.all([queue.submit('00123', '1', result.handlers), queue.submit('00123', '1', result.handlers)]);
  assert.equal(calls, 1); assert.deepEqual(result.added, [measured]);
});
test('stale revision, archived and non-exact results never add', async () => {
  for (const response of [{ ...match, config_revision: '2' }, { ...match, item: { ...item, archived: true } }, { ...match, item: { ...item, barcode: '123' } }]) {
    const queue = createScanQueue(async () => response); const result = recorder(); await queue.submit('00123', '1', result.handlers);
    assert.equal(result.added.length, 0); assert.equal(result.errors.length, 1);
  }
});
test('unknown barcode errors do not poison a deliberate retry', async () => {
  let calls = 0; const queue = createScanQueue(async () => { if (++calls === 1) throw new Error('Unknown barcode'); return match; }); const result = recorder();
  await queue.submit('00123', '1', result.handlers); await queue.submit('00123', '1', result.handlers);
  assert.deepEqual(result.errors, ['Unknown barcode']); assert.deepEqual(result.added, [item]);
});


test('clearing the sale invalidates a pending scan synchronously before effect cleanup', async () => {
  const old = deferred<BarcodeMatch>(); const queue = createScanQueue(() => old.promise); const result = recorder();
  let resetToken = 0; const capturedToken = resetToken;
  const handlers = { ...result.handlers, current: () => resetToken === capturedToken };
  const first = queue.submit('00123', '1', handlers); const queued = queue.submit('00123', '1', handlers);
  await Promise.resolve(); resetToken++;
  // Deliberately do not call queue.cancel(): result guards precede React's passive effect.
  old.resolve(match); await Promise.all([first, queued]);
  assert.equal(result.added.length, 0); assert.equal(result.finished, 0); assert.deepEqual(result.errors, []);
});
test('clearing the sale suppresses stale failure and finished callbacks before effect cleanup', async () => {
  const old = deferred<BarcodeMatch>(); const queue = createScanQueue(() => old.promise); const result = recorder();
  let current = true; const task = queue.submit('00123', '1', { ...result.handlers, current: () => current });
  await Promise.resolve(); current = false; old.reject(new Error('Old failure')); await task;
  assert.deepEqual(result.errors, []); assert.equal(result.finished, 0);
});
