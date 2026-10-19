import test from 'node:test';
import assert from 'node:assert/strict';
import { sameCurrency, variance, readLedgerAttempt, LEDGER_PENDING_KEY, parseCashAmount, cashReasonError, cashMovementError, validLedgerAttempt, writeLedgerJournal, clearLedgerJournal, LedgerWriter, type CashSession, type LedgerAttempt } from '../src/ledger.ts';
import { createRegisterWriter, writeCheckoutJournal } from '../src/registerWriter.ts';
import type { DraftLock, DraftStorage } from '../src/drafts.ts';
import { reportDateTime } from '../src/reports.ts';

const currency = { code: 'USD', minor_unit_exponent: 2 };
const session: CashSession = { session_id: 'session-1', currency, opened_at_unix: '123', opening_float_minor: '5000', sales_total_minor: '0', refunds_total_minor: '0', paid_in_total_minor: '0', paid_out_total_minor: '0', expected_cash_minor: '5000', movement_count: '0', closed_at_unix: null, counted_minor: null, variance_minor: null };
const attempt: LedgerAttempt = { command: 'record_cash_movement', payload: { idempotency_key: 'original-key', kind: 'paid_out', amount_minor: '1250', reason: 'Training safe transfer', expected_session_id: 'session-1', expected_currency: currency }, currency, amount: '1250', label: 'Paid out' };
const direct: DraftLock = work => work();
function storage() { const data = new Map<string, string>(); return { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => { data.set(key, value); }, removeItem: (key: string) => { data.delete(key); } }; }
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (error: unknown) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function serviceError(code: string) { return Object.assign(new Error(code), { code }); }
const withPayload = (patch: Record<string, unknown>): LedgerAttempt => ({ ...attempt, payload: { ...attempt.payload, ...patch } });

test('drawer variance uses exact signed minor units above safe integer range', () => {
  assert.equal(variance('9007199254740993', '9007199254740994'), '-1');
  assert.equal(variance('100', '100'), '0');
  assert.equal(variance('101', '100'), '1');
});
test('currency compatibility checks code and exponent', () => {
  assert.equal(sameCurrency(currency, { ...currency }), true);
  assert.equal(sameCurrency(currency, { ...currency, code: 'CAD' }), false);
  assert.equal(sameCurrency(currency, { ...currency, minor_unit_exponent: 0 }), false);
});
test('cash amounts remain exact and bounded at the service u64 limit', () => {
  assert.equal(parseCashAmount('0.29', 2, true), '29');
  assert.equal(parseCashAmount('90071992547409.93', 2, true), '9007199254740993');
  assert.equal(parseCashAmount('184467440737095516.15', 2, true), '18446744073709551615');
  assert.equal(parseCashAmount('184467440737095516.16', 2, true), null);
  assert.equal(parseCashAmount('18446744073709551615', 0, true), '18446744073709551615');
  assert.equal(parseCashAmount('18446744073709551616', 0, true), null);
});
test('cash movements reject zero and imprecise input without rounding', () => {
  for (const value of ['', '0', '0.00', '-1', '1.001', '1e2', '1,000.00', '$5', '.50', '1.', 'NaN', '9'.repeat(65)]) assert.equal(parseCashAmount(value, 2, true), null, value);
  assert.equal(parseCashAmount('0', 2), '0'); // Opening float and closing count may be zero.
  assert.equal(parseCashAmount('1', 0, true), '1');
  assert.equal(parseCashAmount('1.0', 0, true), null);
  assert.equal(parseCashAmount('1.234', 3, true), '1234');
  assert.equal(parseCashAmount('1.2345', 3, true), null);
  assert.equal(parseCashAmount('1.000001', 6, true), '1000001');
  assert.equal(parseCashAmount('1', 7, true), null);
});
test('reasons are required, control-free, and limited by UTF-8 bytes rather than characters', () => {
  for (const reason of ['', '   ', 'Line\nbreak', 'Line\tbreak', 'bad\u0000text', 'bad\u0085text', 'a'.repeat(501), 'é'.repeat(251), '💵'.repeat(126)]) assert.notEqual(cashReasonError(reason), '', reason);
  for (const reason of ['a'.repeat(500), 'é'.repeat(250), '💵'.repeat(125), '  Training float top-up  ']) assert.equal(cashReasonError(reason), '', reason);
});
test('only an open drawer allows movements and paid-out may exactly empty it', () => {
  assert.equal(cashMovementError('paid_out', '5000', 'Training transfer', session), '');
  assert.match(cashMovementError('paid_out', '5001', 'Training transfer', session), /expected cash/);
  assert.equal(cashMovementError('paid_in', '5001', 'Training replenishment', session), '');
  assert.match(cashMovementError('paid_out', '0', 'Training transfer', session), /greater than zero/);
  assert.match(cashMovementError('paid_out', null, 'Training transfer', session), /greater than zero/);
  assert.match(cashMovementError('paid_in', '100', '', session), /reason/);
  assert.match(cashMovementError('paid_in', '100', 'Training transfer', { ...session, closed_at_unix: '124' }), /Open a cash session/);
  assert.match(cashMovementError('paid_out', '1', 'Training transfer', { ...session, expected_cash_minor: '-50' }), /expected cash/);
});
test('restart recovers the exact mutation payload and key without regenerating', () => {
  const local = storage(); local.setItem(LEDGER_PENDING_KEY, JSON.stringify(attempt));
  assert.deepEqual(readLedgerAttempt(local), attempt);
  assert.deepEqual(readLedgerAttempt(local), attempt);
  assert.equal(local.getItem(LEDGER_PENDING_KEY), JSON.stringify(attempt));
});
test('every supported cash command requires the original recovery guards', () => {
  const open: LedgerAttempt = { command: 'open_cash_session', currency, amount: '0', label: 'Opening', payload: { idempotency_key: 'open-key', expected_currency: currency, opening_float_minor: '0' } };
  const close: LedgerAttempt = { command: 'close_cash_session', currency, amount: '0', label: 'Closing', payload: { session_id: 'session-1', counted_minor: '0', expected_movement_count: '10' } };
  const refund: LedgerAttempt = { command: 'refund_cash_sale', currency, amount: '9007199254740993', label: 'Receipt 1', payload: { idempotency_key: 'refund-key', sale_id: 'sale-1', reason: 'Training return', expected_session_id: 'session-1', expected_currency: currency } };
  for (const value of [open, close, refund, attempt]) assert.equal(validLedgerAttempt(value), true);
  assert.equal(validLedgerAttempt({ ...open, amount: '100' }), false);
  assert.equal(validLedgerAttempt({ ...close, payload: { ...close.payload, expected_movement_count: undefined } }), false);
  assert.equal(validLedgerAttempt({ ...refund, payload: { ...refund.payload, expected_session_id: undefined } }), false);
  assert.equal(validLedgerAttempt({ ...refund, payload: { ...refund.payload, expected_currency: { code: 'EUR', minor_unit_exponent: 2 } } }), false);
});
test('malformed, missing, misleading, and wrong-kind journals fail closed without removing evidence', () => {
  const invalid = ['broken JSON', '', 'null', '[]', '{}', JSON.stringify({ ...attempt, command: 'complete_cash_sale' }), JSON.stringify({ ...attempt, amount: '999' }), JSON.stringify({ ...attempt, currency: { code: 'USD', minor_unit_exponent: 3 } }), JSON.stringify({ ...attempt, amount: 1250 }), JSON.stringify({ ...attempt, label: '' }), JSON.stringify({ ...attempt, payload: [] })];
  for (const patch of [{ amount_minor: '0' }, { amount_minor: '-1' }, { amount_minor: '1250.00' }, { amount_minor: '01250' }, { amount_minor: '18446744073709551616' }, { kind: 'refund' }, { expected_session_id: '' }, { expected_session_id: null }, { expected_currency: null }, { expected_currency: { code: 'USD', minor_unit_exponent: 7 } }, { reason: '' }, { reason: 'a'.repeat(501) }, { reason: ` ${'a'.repeat(500)} ` }, { idempotency_key: null }, { idempotency_key: ' padded-key ' }, { idempotency_key: 'é'.repeat(65) }]) invalid.push(JSON.stringify(withPayload(patch)));
  for (const raw of invalid) {
    const local = storage(); local.setItem(LEDGER_PENDING_KEY, raw);
    assert.throws(() => readLedgerAttempt(local), /cash recovery data/);
    const writer = new LedgerWriter(async <T>() => { throw new Error('Must not dispatch'); }, local, direct);
    assert.equal(writer.blocked, true); assert.equal(writer.view.invalidJournal, true); assert.equal(writer.view.pending, null);
    assert.equal(local.getItem(LEDGER_PENDING_KEY), raw);
  }
});
test('an invalid recovery journal blocks direct execution and cannot be discarded', async () => {
  const local = storage(); local.setItem(LEDGER_PENDING_KEY, '{}'); let calls = 0;
  const writer = new LedgerWriter(async <T>() => { calls++; return {} as T; }, local, direct);
  await assert.rejects(writer.execute(attempt), /cash recovery data/); assert.equal(await writer.discardRejected(), false); assert.equal(calls, 0); assert.equal(local.getItem(LEDGER_PENDING_KEY), '{}');
});
test('the exact journal is durable under the register lock before sending', async () => {
  const local = storage(); let locked = false; const events: string[] = [];
  const writer = new LedgerWriter(async <T>(name: string, payload?: unknown) => { assert.equal(locked, true); assert.equal(name, 'record_cash_movement'); assert.deepEqual(readLedgerAttempt(local)?.payload, payload); events.push('send'); return {} as T; }, local, async work => { locked = true; events.push('lock'); try { await work(); } finally { events.push('unlock'); locked = false; } });
  assert.equal(await writer.execute(attempt), true); assert.deepEqual(events, ['lock', 'send', 'unlock']); assert.equal(local.getItem(LEDGER_PENDING_KEY), null); assert.equal(writer.blocked, false);
});
test('repeat clicks are locked synchronously for the full uncertain request', async () => {
  const local = storage(); const response = deferred<unknown>(); let calls = 0;
  const writer = new LedgerWriter(async <T>() => { calls++; return await response.promise as T; }, local, direct);
  const first = writer.execute(attempt); assert.equal(writer.view.busy, true); assert.equal(writer.blocked, true);
  assert.equal(await writer.execute(withPayload({ idempotency_key: 'second-key' })), false); assert.equal(await writer.execute(attempt), false); assert.equal(calls, 1);
  response.resolve({}); assert.equal(await first, true); assert.equal(writer.blocked, false);
});
test('a storage failure sends nothing and never creates a fake recoverable action', async () => {
  let calls = 0; const local: DraftStorage = { getItem: () => null, setItem: () => { throw new Error('quota'); }, removeItem: () => {} };
  const writer = new LedgerWriter(async <T>() => { calls++; return {} as T; }, local, direct);
  await assert.rejects(writer.execute(attempt), /No cash request was sent/); assert.equal(calls, 0); assert.equal(writer.view.pending, null); assert.equal(writer.view.canDiscard, false); assert.equal(writer.view.busy, false);
});
test('lost replies survive reload and retry the identical request with no new key', async () => {
  const local = storage(); const requests: unknown[] = [];
  const first = new LedgerWriter(async <T>(_name: string, payload?: unknown) => { requests.push(payload); throw new Error('lost response'); }, local, direct);
  await assert.rejects(first.execute(attempt), /lost response/); assert.equal(first.blocked, true); assert.equal(first.view.canDiscard, false); assert.equal(await first.discardRejected(), false);
  const restored = new LedgerWriter(async <T>(_name: string, payload?: unknown) => { requests.push(payload); return {} as T; }, local, direct);
  assert.deepEqual(restored.view.pending, attempt); assert.equal(restored.blocked, true); await restored.execute(restored.view.pending!);
  assert.deepEqual(requests, [attempt.payload, attempt.payload]); assert.equal(restored.blocked, false); assert.equal(local.getItem(LEDGER_PENDING_KEY), null);
});
test('known service rejection permits explicit discard under lock, never automatic deletion', async () => {
  for (const code of ['INSUFFICIENT_CASH', 'STALE_SESSION', 'SESSION_CHANGED', 'SESSION_CURRENCY', 'NO_OPEN_SESSION', 'INVALID_INPUT']) {
    const local = storage(); let locks = 0;
    const writer = new LedgerWriter(async <T>() => { throw serviceError(code); }, local, async work => { locks++; await work(); });
    await assert.rejects(writer.execute(attempt)); assert.equal(writer.view.canDiscard, true, code); assert.notEqual(local.getItem(LEDGER_PENDING_KEY), null);
    assert.equal(await writer.discardRejected(), true); assert.equal(locks, 2); assert.equal(local.getItem(LEDGER_PENDING_KEY), null); assert.equal(writer.blocked, false);
  }
});
test('unknown errors and idempotency conflicts cannot discard potentially committed cash', async () => {
  for (const code of ['connection_error', 'INTERNAL', 'IDEMPOTENCY_CONFLICT']) {
    const local = storage(); const writer = new LedgerWriter(async <T>() => { throw serviceError(code); }, local, direct);
    await assert.rejects(writer.execute(attempt)); assert.equal(writer.view.canDiscard, false); assert.equal(await writer.discardRejected(), false); assert.equal(local.getItem(LEDGER_PENDING_KEY), JSON.stringify(attempt));
  }
});
test('a different action cannot replace an in-memory recovery request', async () => {
  const local = storage(); let calls = 0; const writer = new LedgerWriter(async <T>() => { calls++; throw new Error('offline'); }, local, direct);
  await assert.rejects(writer.execute(attempt)); await assert.rejects(writer.execute(withPayload({ idempotency_key: 'different' })), /Recover the saved/); assert.equal(calls, 1); assert.equal(local.getItem(LEDGER_PENDING_KEY), JSON.stringify(attempt));
});
test('another window journal is never overwritten by a new cash action', async () => {
  const local = storage(); let calls = 0; const writer = new LedgerWriter(async <T>() => { calls++; return {} as T; }, local, direct);
  const other = JSON.stringify(withPayload({ idempotency_key: 'other' })); local.setItem(LEDGER_PENDING_KEY, other);
  await assert.rejects(writer.execute(attempt), /Another window/); assert.equal(calls, 0); assert.equal(local.getItem(LEDGER_PENDING_KEY), other); assert.equal(writer.view.pending, null);
});
test('a journal replaced during the server response is preserved and cannot be discarded', async () => {
  const local = storage(); const response = deferred<unknown>(); const writer = new LedgerWriter(async <T>() => await response.promise as T, local, direct);
  const task = writer.execute(attempt); const other = JSON.stringify(withPayload({ idempotency_key: 'other' })); local.setItem(LEDGER_PENDING_KEY, other); response.resolve({});
  await assert.rejects(task, /record changed/); assert.equal(local.getItem(LEDGER_PENDING_KEY), other); assert.equal(writer.view.canDiscard, false); assert.equal(await writer.discardRejected(), false);
});
test('discard uses compare-and-swap and cannot remove a different rejected-window journal', async () => {
  const local = storage(); const writer = new LedgerWriter(async <T>() => { throw serviceError('INSUFFICIENT_CASH'); }, local, direct);
  await assert.rejects(writer.execute(attempt)); const other = JSON.stringify(withPayload({ idempotency_key: 'other' })); local.setItem(LEDGER_PENDING_KEY, other);
  await assert.rejects(writer.discardRejected(), /record changed/); assert.equal(local.getItem(LEDGER_PENDING_KEY), other); assert.deepEqual(writer.view.pending, attempt);
});
test('journal helpers reject incomplete requests and compare exact stored identity', () => {
  const local = storage(); assert.throws(() => writeLedgerJournal(local, withPayload({ expected_session_id: undefined }), false), /invalid/); assert.equal(local.getItem(LEDGER_PENDING_KEY), null);
  writeLedgerJournal(local, attempt, false); assert.throws(() => clearLedgerJournal(local, withPayload({ idempotency_key: 'other' })), /record changed/); clearLedgerJournal(local, attempt); assert.equal(local.getItem(LEDGER_PENDING_KEY), null);
});
test('new cash actions reject pending checkout and cart records before writing or dispatch', async () => {
  for (const key of ['gram.checkout.pending.v1', 'gram.draft.pending.v1']) {
    const local = storage(); let calls = 0; const writer = new LedgerWriter(async <T>() => { calls++; return {} as T; }, local, direct); local.setItem(key, '{}');
    await assert.rejects(writer.execute(attempt), /needs recovery/); assert.equal(calls, 0); assert.equal(local.getItem(LEDGER_PENDING_KEY), null); assert.equal(local.getItem(key), '{}');
  }
});
test('multi-workflow recovery prioritizes checkout then cash then carts without dropping any journal', async () => {
  const local = storage(); local.setItem('gram.draft.pending.v1', 'saved-cart'); local.setItem('gram.checkout.pending.v1', 'saved-checkout'); local.setItem(LEDGER_PENDING_KEY, JSON.stringify(attempt)); let calls = 0;
  const writer = new LedgerWriter(async <T>() => { calls++; return {} as T; }, local, direct);
  await assert.rejects(writer.execute(attempt), /checkout needs recovery/); assert.equal(calls, 0); assert.equal(local.getItem(LEDGER_PENDING_KEY), JSON.stringify(attempt));
  local.removeItem('gram.checkout.pending.v1'); assert.equal(await writer.execute(attempt), true); assert.equal(calls, 1); assert.equal(local.getItem('gram.draft.pending.v1'), 'saved-cart'); assert.equal(local.getItem(LEDGER_PENDING_KEY), null);
});
test('pending cash data, including malformed empty data, blocks NEW checkout but not exact checkout recovery', () => {
  const checkout = { idempotency_key: 'checkout', lines: [], tender_minor: '100', expected_total_minor: '100', expected_config_revision: '1' };
  for (const raw of [JSON.stringify(attempt), '{}', '']) {
    const local = storage(); local.setItem(LEDGER_PENDING_KEY, raw); assert.throws(() => writeCheckoutJournal(local, checkout, false), /cash action needs recovery/); assert.equal(local.getItem('gram.checkout.pending.v1'), null);
    writeCheckoutJournal(local, checkout, true); assert.equal(local.getItem(LEDGER_PENDING_KEY), raw);
  }
});
test('cash writers fail closed when real Web Locks are unavailable', async () => {
  const local = storage(); let calls = 0; const writer = new LedgerWriter(async <T>() => { calls++; return {} as T; }, local, createRegisterWriter(undefined));
  await assert.rejects(writer.execute(attempt), /require browser Web Locks/); assert.equal(calls, 0); assert.equal(local.getItem(LEDGER_PENDING_KEY), null);
});
test('two window writers share the same real lock and cannot race across different actions', async () => {
  let held = false; const response = deferred<unknown>(); const local = storage(); let calls = 0;
  const locks = { request: async (_name: string, _options: unknown, callback: (lock: object | null) => Promise<void>) => { if (held) return callback(null); held = true; try { return await callback({}); } finally { held = false; } } } as unknown as LockManager;
  const exclusive = createRegisterWriter(locks);
  const first = new LedgerWriter(async <T>() => { calls++; return await response.promise as T; }, local, exclusive);
  const second = new LedgerWriter(async <T>() => { calls++; return {} as T; }, local, exclusive);
  const task = first.execute(attempt); assert.equal(held, true);
  await assert.rejects(second.execute(withPayload({ idempotency_key: 'second' })), /Another window is saving/); assert.equal(calls, 1); assert.equal(local.getItem(LEDGER_PENDING_KEY), JSON.stringify(attempt));
  response.resolve({}); await task; assert.equal(held, false); assert.equal(await second.execute(withPayload({ idempotency_key: 'second' })), true); assert.equal(calls, 2);
});
test('cash history uses explicit store timezone rather than machine locale timezone', () => {
  assert.equal(reportDateTime('0', 'Asia/Tokyo'), '1970-01-01 09:00:00 GMT+09:00 [Asia/Tokyo]');
  assert.equal(reportDateTime('0', 'Pacific/Honolulu'), '1969-12-31 14:00:00 GMT-10:00 [Pacific/Honolulu]');
  assert.equal(reportDateTime('0', 'UTC'), '1970-01-01 00:00:00 UTC');
});

test('stale pending replay cannot resurrect a journal another window already resolved', async () => {
  const local = storage(); local.setItem(LEDGER_PENDING_KEY, JSON.stringify(attempt)); let calls = 0;
  const writer = new LedgerWriter(async <T>() => { calls++; return {} as T; }, local, direct);
  local.removeItem(LEDGER_PENDING_KEY); // Another window completed or explicitly discarded it while this one was idle.
  await assert.rejects(writer.execute(writer.view.pending!), /resolved in another window/);
  assert.equal(calls, 0); assert.equal(local.getItem(LEDGER_PENDING_KEY), null); assert.deepEqual(writer.view.pending, attempt); assert.equal(writer.view.canDiscard, false);
});
test('a second window retaining a rejected action cannot replay it after explicit discard elsewhere', async () => {
  const local = storage(); local.setItem(LEDGER_PENDING_KEY, JSON.stringify(attempt)); let calls = 0;
  const stale = new LedgerWriter(async <T>() => { calls++; return {} as T; }, local, direct);
  const active = new LedgerWriter(async <T>() => { throw serviceError('INSUFFICIENT_CASH'); }, local, direct);
  await assert.rejects(active.execute(attempt)); await active.discardRejected();
  await assert.rejects(stale.execute(attempt), /resolved in another window/); assert.equal(calls, 0); assert.equal(local.getItem(LEDGER_PENDING_KEY), null);
});
