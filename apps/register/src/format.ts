import type { Currency, Quantity } from './types.ts';
export const USD: Currency = { code: 'USD', minor_unit_exponent: 2 };
/** Formats integer minor units without sending exact values through floating point. */
export function money(value: string, currency: Currency = USD): string {
  if (!/^-?\d+$/.test(value)) return '—';
  const sign = value.startsWith('-') ? '−' : '';
  const digits = value.replace('-', '').padStart(currency.minor_unit_exponent + 1, '0');
  const exponent = currency.minor_unit_exponent;
  const whole = (exponent ? digits.slice(0, -exponent) : digits).replace(/^0+(?=\d)/, '').replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  const fraction = exponent ? `.${digits.slice(-exponent)}` : '';
  return `${sign}${currency.code === 'USD' ? '$' : `${currency.code} `}${whole}${fraction}`;
}
/** Tender input conversion only. All merchandise pricing is done by Rust. */
export function parseTender(value: string, exponent = 2): string | null {
  const text = value.trim();
  if (!Number.isInteger(exponent) || exponent < 0 || exponent > 6) return null;
  const pattern = exponent === 0 ? /^\d+$/ : new RegExp(`^\\d+(?:\\.\\d{1,${exponent}})?$`);
  if (!pattern.test(text)) return null;
  const [whole, fraction = ''] = text.split('.');
  return (BigInt(whole) * 10n ** BigInt(exponent) + BigInt(fraction.padEnd(exponent, '0') || '0')).toString();
}
export function minorToInput(value: string, exponent = 2): string {
  const digits = value.padStart(exponent + 1, '0');
  return exponent ? `${digits.slice(0, -exponent)}.${digits.slice(-exponent)}` : digits;
}
export function validGrams(value: string): boolean { return /^(?:0|[1-9]\d*)(?:\.\d{1,3})?$/.test(value) && /[1-9]/.test(value); }
export function validCount(value: string): boolean { return /^[1-9]\d*$/.test(value); }
export function quantityLabel(quantity: Quantity, declared?: string | null): string {
  return quantity.kind === 'measured' ? `${quantity.grams} g measured` : `${quantity.count} × ${declared || '—'} g declared`;
}
export function dateTime(unix: string): string { return new Date(Number(unix) * 1000).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' }); }
export const ruleNames: Record<string, string> = {
  within_nominal_tolerance: 'Within tier tolerance', below_smallest_proportional: 'Proportional below first tier', full_actual_at_tier_rate: 'Full weight at tier rate', nominal_plus_excess_beyond_tolerance: 'Tier + excess beyond tolerance',
};
