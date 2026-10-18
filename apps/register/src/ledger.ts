import type { Currency } from './types';
export type CashSession = { session_id: string; opened_at_unix: string; currency: Currency; opening_float_minor: string; sales_total_minor: string; refunds_total_minor: string; expected_cash_minor: string; movement_count: string; closed_at_unix: string | null; counted_minor: string | null; variance_minor: string | null };
export type RefundReceipt = { refund_id: string; refund_number: string; created_at_unix: string; sale_id: string; original_receipt_number: string; session_id: string; reason: string; amount_minor: string; currency: Currency; payment_method: 'cash' };
export type CashLedger = { active_session: CashSession | null; sessions: CashSession[]; refunds: RefundReceipt[]; sale_sessions: { sale_id: string; session_id: string }[] };
export type LedgerAttempt = { command: 'open_cash_session' | 'close_cash_session' | 'refund_cash_sale'; payload: Record<string, unknown>; currency: Currency; amount: string; label: string };
export const LEDGER_PENDING_KEY = 'gram.ledger.pending.v1';
export function sameCurrency(a: Currency, b: Currency) { return a.code === b.code && a.minor_unit_exponent === b.minor_unit_exponent; }
export function readLedgerAttempt(): LedgerAttempt | null { try { return JSON.parse(localStorage.getItem(LEDGER_PENDING_KEY) || 'null'); } catch { return null; } }
export function variance(counted: string, expected: string) { return (BigInt(counted) - BigInt(expected)).toString(); }
