import test from 'node:test';
import assert from 'node:assert/strict';
import { CHECKOUT_PENDING_KEY, REGISTER_WRITER_LOCK, clearCheckoutJournal, createRegisterWriter, writeCheckoutJournal } from '../src/registerWriter.ts';
import type { CheckoutPayload } from '../src/types.ts';
const payload: CheckoutPayload = { idempotency_key: 'payment-1', lines: [], tender_minor: '100', expected_total_minor: '100', expected_config_revision: '1', expected_draft_id: 'draft-1', expected_draft_revision: '2' };
function storage() { const data = new Map<string, string>(); return { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => { data.set(key, value); }, removeItem: (key: string) => { data.delete(key); } }; }
test('safe writers fail closed without Web Locks, never pretending storage is an atomic lock', async () => {
  let called = false; await assert.rejects(createRegisterWriter(undefined)(async () => { called = true; }), /require browser Web Locks/); assert.equal(called, false);
});
test('draft and checkout writers use one exclusive register lock and await the entire action', async () => {
  const events: string[] = []; const locks = { request: async (name: string, options: object, run: (lock: object) => Promise<void>) => { assert.equal(name, REGISTER_WRITER_LOCK); assert.deepEqual(options, { ifAvailable: true }); events.push('locked'); await run({}); events.push('released'); } } as unknown as LockManager;
  await createRegisterWriter(locks)(async () => { events.push('journal'); await Promise.resolve(); events.push('reply'); }); assert.deepEqual(events, ['locked', 'journal', 'reply', 'released']);
});
test('an occupied real lock does not execute the caller', async () => {
  let called = false; const locks = { request: async (_name: string, _options: object, run: (lock: null) => Promise<void>) => run(null) } as unknown as LockManager;
  await assert.rejects(createRegisterWriter(locks)(async () => { called = true; }), /Another window/); assert.equal(called, false);
});
test('checkout journals preserve exact payload and cannot overwrite a different checkout', () => {
  const local = storage(); writeCheckoutJournal(local, payload, false); const saved = local.getItem(CHECKOUT_PENDING_KEY); assert.equal(saved, JSON.stringify(payload)); writeCheckoutJournal(local, payload, true);
  assert.throws(() => writeCheckoutJournal(local, { ...payload, idempotency_key: 'other' }, false), /Another window/); assert.equal(local.getItem(CHECKOUT_PENDING_KEY), saved); clearCheckoutJournal(local, payload); assert.equal(local.getItem(CHECKOUT_PENDING_KEY), null);
});
test('a changed checkout journal cannot be cleared by an older response', () => {
  const local = storage(); const other = { ...payload, idempotency_key: 'other' }; writeCheckoutJournal(local, other, false); assert.throws(() => clearCheckoutJournal(local, payload), /record changed/); assert.equal(local.getItem(CHECKOUT_PENDING_KEY), JSON.stringify(other));
});
test('pending draft blocks new checkout but exact payment recovery retains priority', () => {
  const local = storage(); local.setItem('gram.draft.pending.v1', 'pending'); assert.throws(() => writeCheckoutJournal(local, payload, false), /cart action needs recovery/); assert.equal(local.getItem(CHECKOUT_PENDING_KEY), null); writeCheckoutJournal(local, payload, true); assert.equal(local.getItem(CHECKOUT_PENDING_KEY), JSON.stringify(payload)); assert.equal(local.getItem('gram.draft.pending.v1'), 'pending');
});
