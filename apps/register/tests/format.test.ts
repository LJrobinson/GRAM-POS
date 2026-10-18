import test from 'node:test';
import assert from 'node:assert/strict';
import { money, parseTender, minorToInput, validGrams, validCount, quantityLabel } from '../src/format.ts';

test('money retains exact cents above JavaScript safe integer range', () => {
  assert.equal(money('9007199254740993'), '$90,071,992,547,409.93');
});
test('money formats zero, small, and negative values', () => {
  assert.equal(money('0'), '$0.00');
  assert.equal(money('1'), '$0.01');
  assert.equal(money('-12345'), '−$123.45');
});
test('currency formatting respects exponent and never implies USD for other currencies', () => {
  assert.equal(money('12000', { code: 'JPY', minor_unit_exponent: 0 }), 'JPY 12,000');
  assert.equal(money('1234', { code: 'BHD', minor_unit_exponent: 3 }), 'BHD 1.234');
});
test('invalid money does not show as a paid value', () => {
  for (const value of ['NaN', '1.1', '', '1e3']) assert.equal(money(value), '—');
});
test('cash tender converts exact decimal input without floats', () => {
  assert.equal(parseTender('0.29'), '29');
  assert.equal(parseTender('50'), '5000');
  assert.equal(parseTender(' 50.5 '), '5050');
  assert.equal(parseTender('90071992547409.93'), '9007199254740993');
});
test('cash tender rejects ambiguous and imprecise inputs', () => {
  for (const value of ['', '.', '1.', '1.001', '-1', '1e3', '$1', '1,000', 'NaN', '1/2']) assert.equal(parseTender(value), null, value);
});
test('tender supports exact zero-decimal and three-decimal currencies', () => {
  assert.equal(parseTender('123', 0), '123');
  assert.equal(parseTender('123.1', 0), null);
  assert.equal(parseTender('1.234', 3), '1234');
});
test('exact amount shortcut round-trips large integer strings', () => {
  for (const value of ['0', '1', '10', '999', '9007199254740993']) assert.equal(parseTender(minorToInput(value)), value);
});
test('measured grams accept precision through one milligram', () => {
  for (const value of ['0.001', '1', '3.5', '3.500', '28.349']) assert.equal(validGrams(value), true, value);
});
test('measured grams reject zero, excessive precision and package-style arithmetic', () => {
  for (const value of ['0', '0.000', '-1', '.5', '3.5000', '01', '1e3', '1/8', 'NaN']) assert.equal(validGrams(value), false, value);
});
test('package counts are positive whole-number strings', () => {
  for (const value of ['1', '12', '9007199254740993']) assert.equal(validCount(value), true, value);
  for (const value of ['0', '1.0', '1.5', '-1', '01', '', '1e3']) assert.equal(validCount(value), false, value);
});
test('quantity display preserves original recorded measured input', () => {
  assert.equal(quantityLabel({ kind: 'measured', grams: '3.500' }), '3.500 g measured');
});
test('prepack display does not claim a measured weight', () => {
  assert.equal(quantityLabel({ kind: 'prepacked', count: '2' }, '3.500'), '2 × 3.500 g declared');
});
