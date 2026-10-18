import test from 'node:test';
import assert from 'node:assert/strict';
import { sameCurrency, variance, readLedgerAttempt, LEDGER_PENDING_KEY } from '../src/ledger.ts';
test('drawer variance uses exact signed minor units above safe integer range', () => {
  assert.equal(variance('9007199254740993', '9007199254740994'), '-1');
  assert.equal(variance('100', '100'), '0');
  assert.equal(variance('101', '100'), '1');
});
test('currency compatibility checks code and exponent', () => {
  const usd = { code: 'USD', minor_unit_exponent: 2 };
  assert.equal(sameCurrency(usd, { ...usd }), true);
  assert.equal(sameCurrency(usd, { ...usd, code: 'CAD' }), false);
  assert.equal(sameCurrency(usd, { ...usd, minor_unit_exponent: 0 }), false);
});
test('restart recovers the exact mutation payload and key without regenerating', () => {
  const attempt = { command: 'refund_cash_sale', payload: { idempotency_key: 'original-key', sale_id: 'sale', reason: 'return' }, currency: { code: 'USD', minor_unit_exponent: 2 }, amount: '9007199254740993', label: 'receipt' };
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: (key: string) => key === LEDGER_PENDING_KEY ? JSON.stringify(attempt) : null } });
  assert.deepEqual(readLedgerAttempt(), attempt);
  assert.deepEqual(readLedgerAttempt(), attempt);
});
