import test from 'node:test';
import assert from 'node:assert/strict';
import { printSavedReceipt, receiptTimestamp, receiptView } from '../src/receiptModel.ts';
import type { Currency, Receipt } from '../src/types.ts';

function savedReceipt(currency: Currency = { code: 'USD', minor_unit_exponent: 2 }): Receipt {
  return {
    sale_id: 'saved-sale-1', receipt_number: 'GRAM-000001', created_at_unix: '1704067200',
    tender_minor: '15000', change_minor: '2500', payment_method: 'cash', demo: true,
    quote: {
      config_revision: '7', currency, total_minor: '12500', tax_mode: 'demo_no_tax',
      lines: [
        { id: 'measured-1', sku: 'BULK', name: 'Original bulk name', quantity: { kind: 'measured', grams: '7.200' }, declared_content_grams: null, amount_minor: '5500', explanation: { kind: 'group_allocation' } },
        { id: 'pack-1', sku: 'PACK', name: 'Original pack name', quantity: { kind: 'prepacked', count: '2' }, declared_content_grams: '3.500', amount_minor: '7000', explanation: { kind: 'fixed_unit', unit_price_minor: '3500', count: '2' } },
      ],
      groups: [{
        group_id: 'ORIGINAL', actual_measured_grams: '7.200', declared_package_grams: '0.000',
        eligible_grams: '7.200', selected_nominal_grams: '7.000', tolerance_grams: '0.200',
        billable_grams: '7.000', rule: 'within_nominal_tolerance', amount_minor: '5500',
        exact_numerator: '5500', exact_denominator: '1',
        schedule: { id: 'ORIGINAL', name: 'Original schedule', tiers: [{ nominal_grams: '7.000', tolerance_grams: '0.200', price_minor: '5500' }], overage_policy: 'full_actual_at_tier_rate', rounding: 'half_up', below_smallest: 'reject', tolerance_scope: 'aggregate_pricing_group' },
      }],
    },
  };
}

function freezeDeep<T>(value: T): T {
  if (value && typeof value === 'object') {
    Object.values(value).forEach(freezeDeep);
    Object.freeze(value);
  }
  return value;
}

test('receipt display reads only the immutable saved names, quantities, prices and schedule', () => {
  const receipt = freezeDeep(savedReceipt());
  const before = JSON.stringify(receipt);
  const view = receiptView(receipt);
  assert.deepEqual(view.lines, [
    { id: 'measured-1', sku: 'BULK', name: 'Original bulk name', quantity: '7.200 g measured', amount: '$55.00', unitPrice: null },
    { id: 'pack-1', sku: 'PACK', name: 'Original pack name', quantity: '2 × 3.500 g declared', amount: '$70.00', unitPrice: '$35.00' },
  ]);
  assert.deepEqual(view.groups, [{ id: 'ORIGINAL', name: 'Original schedule', eligible: '7.200 g eligible · 7.000 g tier', weights: 'Measured: 7.200 g · Declared: 0.000 g', tolerance: 'Tolerance: +0.200 g · Billable: 7.000 g', rule: 'Within tier tolerance', amount: '$55.00' }]);
  assert.equal(view.total, '$125.00');
  assert.equal(view.tender, '$150.00');
  assert.equal(view.change, '$25.00');
  assert.equal(view.revision, '7');
  assert.equal(view.createdAt, '2024-01-01 00:00:00 UTC');
  assert.equal(JSON.stringify(receipt), before);
  // Rebuilding after a restart uses no ambient catalog/config/ledger state.
  assert.deepEqual(receiptView(JSON.parse(before)), view);
});

test('refund annotations and later catalog configuration cannot rewrite the original display', () => {
  const receipt = savedReceipt();
  const before = receiptView(receipt);
  const laterCatalog = [{ sku: 'BULK', name: 'Renamed and archived', archived: true, unit_price_minor: '99999' }];
  const laterRefund = { sale_id: receipt.sale_id, refund_number: 'REFUND-1', amount_minor: receipt.quote.total_minor };
  laterCatalog[0].name = 'Another name';
  laterRefund.refund_number = 'REFUND-2';
  assert.deepEqual(receiptView(receipt), before);
  assert.equal('refund' in before, false);
  assert.equal('catalog' in before, false);
});

test('receipt formats all amounts using saved zero and three-decimal currencies', () => {
  const jpy = receiptView(savedReceipt({ code: 'JPY', minor_unit_exponent: 0 }));
  assert.equal(jpy.currency, 'JPY');
  assert.equal(jpy.total, 'JPY 12,500');
  assert.equal(jpy.tender, 'JPY 15,000');
  assert.equal(jpy.change, 'JPY 2,500');
  assert.equal(jpy.lines[1].unitPrice, 'JPY 3,500');
  const bhd = receiptView(savedReceipt({ code: 'BHD', minor_unit_exponent: 3 }));
  assert.equal(bhd.total, 'BHD 12.500');
  assert.equal(bhd.lines[1].amount, 'BHD 7.000');
  assert.equal(bhd.groups[0].amount, 'BHD 5.500');
});

test('large original totals are formatted, never repriced through floating point', () => {
  const receipt = savedReceipt();
  receipt.quote.total_minor = '9007199254740993';
  receipt.tender_minor = '9007199254740994';
  receipt.change_minor = '1';
  const view = receiptView(receipt);
  assert.equal(view.total, '$90,071,992,547,409.93');
  assert.equal(view.tender, '$90,071,992,547,409.94');
  assert.equal(view.change, '$0.01');
});

test('u64-maximum amounts retain every digit in each printable money column', () => {
  const receipt = savedReceipt();
  receipt.quote.total_minor = '18446744073709551615';
  receipt.tender_minor = '18446744073709551615';
  receipt.change_minor = '0';
  receipt.quote.lines[0].amount_minor = '18446744073709551615';
  receipt.quote.groups[0].amount_minor = '18446744073709551615';
  const view = receiptView(receipt);
  for (const amount of [view.total, view.tender, view.lines[0].amount, view.groups[0].amount]) {
    assert.equal(amount, '$184,467,440,737,095,516.15');
  }
});

test('receipt timestamps are deterministic UTC and reject invalid recorded values', () => {
  assert.equal(receiptTimestamp('0'), '1970-01-01 00:00:00 UTC');
  for (const value of ['', 'NaN', '-1', '1.5', '9999999999999999999999999999']) {
    assert.equal(receiptTimestamp(value), 'Recorded time unavailable');
  }
});

test('print uses exactly the selected attached saved receipt and no implicit invocation', () => {
  let calls = 0;
  const target = { isConnected: true, dataset: { saleId: 'saved-sale-1' } };
  const host = { document: { getElementById: (id: string) => id === 'print-receipt' ? target : null }, print: () => { calls++; } };
  receiptView(savedReceipt());
  assert.equal(calls, 0);
  printSavedReceipt('saved-sale-1', host);
  assert.equal(calls, 1);
  assert.throws(() => printSavedReceipt('some-other-sale', host), /Open the saved receipt again/);
  assert.throws(() => printSavedReceipt('', host), /Open the saved receipt again/);
  target.isConnected = false;
  assert.throws(() => printSavedReceipt('saved-sale-1', host), /Open the saved receipt again/);
  assert.equal(calls, 1);
});

test('missing target never opens a print dialog', () => {
  const host = { document: { getElementById: () => null }, print: () => { assert.fail('Must not print a missing receipt'); } };
  assert.throws(() => printSavedReceipt('saved-sale-1', host), /Open the saved receipt again/);
});

test('a browser without a print function reports the limitation without mutating records', () => {
  const host = { document: { getElementById: () => ({ isConnected: true, dataset: { saleId: 'saved-sale-1' } }) }, print: undefined };
  assert.throws(() => printSavedReceipt('saved-sale-1', host as unknown as Parameters<typeof printSavedReceipt>[1]), /print dialog is not available/);
});

test('print/cancel/reprint makes no record changes and propagates dialog failure', () => {
  const receipt = freezeDeep(savedReceipt());
  const before = JSON.stringify(receipt);
  let calls = 0;
  const host = {
    document: { getElementById: () => ({ isConnected: true, dataset: { saleId: receipt.sale_id } }) },
    // Browsers return void both for cancellation and submitted jobs.
    print: () => { calls++; },
  };
  assert.equal(printSavedReceipt(receipt.sale_id, host), undefined);
  assert.equal(printSavedReceipt(receipt.sale_id, host), undefined);
  assert.equal(calls, 2);
  assert.equal(JSON.stringify(receipt), before);
  host.print = () => { throw new Error('Print dialog unavailable'); };
  assert.throws(() => printSavedReceipt(receipt.sale_id, host), /Print dialog unavailable/);
  assert.equal(JSON.stringify(receipt), before);
});

test('saved unit receipts preserve count and exact unit price without a weight', () => {
  const receipt = savedReceipt();
  receipt.quote.lines = [{ id: 'unit', sku: 'SOAP', name: 'Original soap', quantity: { kind: 'unit', count: '3' }, declared_content_grams: null, amount_minor: '375', explanation: { kind: 'fixed_unit', count: '3', unit_price_minor: '125' } }];
  receipt.quote.groups = [];
  const view = receiptView(freezeDeep(receipt));
  assert.deepEqual(view.lines, [{ id: 'unit', sku: 'SOAP', name: 'Original soap', quantity: '3 units', amount: '$3.75', unitPrice: '$1.25' }]);
  assert.deepEqual(view.groups, []);
  assert.deepEqual(receiptView(JSON.parse(JSON.stringify(receipt))), view);
});

test('legacy receipts retain original GRAM identity and UTC without current settings', () => {
  const receipt = savedReceipt();
  const view = receiptView(receipt);
  assert.equal(view.store.display_name, 'GRAM');
  assert.equal(view.store.receipt_header, '');
  assert.equal(view.store.receipt_footer, '');
  assert.equal(view.createdAt, '2024-01-01 00:00:00 UTC');
});
test('new receipt identity and timezone come only from immutable config snapshot', () => {
  const receipt = savedReceipt();
  receipt.config_snapshot = { store_settings: { display_name: 'Saved store', receipt_header: '<b>Plain text</b>', receipt_contact: 'Saved contact', receipt_footer: 'Saved footer', timezone: 'America/New_York' } };
  const view = receiptView(receipt);
  assert.equal(view.store.display_name, 'Saved store');
  assert.equal(view.store.receipt_header, '<b>Plain text</b>');
  assert.equal(view.store.receipt_footer, 'Saved footer');
  assert.match(view.createdAt, /2023-12-31 19:00:00 GMT-05:00 \[America\/New_York\]/);
  assert.equal(view.total, '$125.00');
});
