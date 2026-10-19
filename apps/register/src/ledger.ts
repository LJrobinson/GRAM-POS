import { parseTender } from './format.ts';
import type { DraftLock, DraftStorage } from './drafts.ts';
import type { Currency } from './types.ts';

export type CashSession = { session_id: string; opened_at_unix: string; currency: Currency; opening_float_minor: string; sales_total_minor: string; refunds_total_minor: string; paid_in_total_minor: string; paid_out_total_minor: string; expected_cash_minor: string; movement_count: string; closed_at_unix: string | null; counted_minor: string | null; variance_minor: string | null };
export type RefundReceipt = { refund_id: string; refund_number: string; created_at_unix: string; sale_id: string; original_receipt_number: string; session_id: string; reason: string; amount_minor: string; currency: Currency; payment_method: 'cash' };
export type CashMovementKind = 'paid_in' | 'paid_out';
export type CashMovement = { movement_id: string; created_at_unix: string; session_id: string; kind: CashMovementKind; amount_minor: string; reason: string; currency: Currency };
export type CashMovementPayload = { idempotency_key: string; kind: CashMovementKind; amount_minor: string; reason: string; expected_session_id: string; expected_currency: Currency };
export type CashLedger = { active_session: CashSession | null; sessions: CashSession[]; refunds: RefundReceipt[]; cash_movements: CashMovement[]; sale_sessions: { sale_id: string; session_id: string }[] };
export type LedgerAttempt = { command: 'open_cash_session' | 'close_cash_session' | 'refund_cash_sale' | 'record_cash_movement'; payload: Record<string, unknown>; currency: Currency; amount: string; label: string };
export const LEDGER_PENDING_KEY = 'gram.ledger.pending.v1';
export const MAX_CASH_MINOR = 18446744073709551615n;
export const MAX_CASH_REASON_BYTES = 500;
export function sameCurrency(a: Currency, b: Currency) { return a.code === b.code && a.minor_unit_exponent === b.minor_unit_exponent; }
export function variance(counted: string, expected: string) { return (BigInt(counted) - BigInt(expected)).toString(); }
export function parseCashAmount(value: string, exponent: number, positive = false): string | null {
  if (value.length > 64) return null;
  const amount = parseTender(value, exponent);
  return amount !== null && BigInt(amount) <= MAX_CASH_MINOR && (!positive || BigInt(amount) > 0n) ? amount : null;
}
export function cashReasonError(value: string): string {
  if (!value.trim()) return 'Enter a reason for this cash action.';
  if (/[\u0000-\u001f\u007f-\u009f]/.test(value)) return 'Use a single-line reason without control characters.';
  if (new TextEncoder().encode(value).length > MAX_CASH_REASON_BYTES) return 'Use a shorter reason (maximum 500 UTF-8 bytes).';
  return '';
}
export function cashMovementError(kind: CashMovementKind, amount: string | null, reason: string, session: CashSession): string {
  if (session.closed_at_unix !== null) return 'Open a cash session before recording cash movements.';
  if (amount === null || !minor(amount) || BigInt(amount) === 0n) return 'Enter an amount greater than zero with no extra decimal places.';
  if (kind === 'paid_out' && BigInt(amount) > BigInt(session.expected_cash_minor)) return 'Paid out cannot exceed the drawer’s expected cash. Refresh cash records and review the amount.';
  return cashReasonError(reason);
}
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const nonempty = (value: unknown): value is string => typeof value === 'string' && value.trim().length > 0;
const identifier = (value: unknown): value is string => nonempty(value) && value.trim() === value && new TextEncoder().encode(value).length <= 128 && !/[\u0000-\u001f\u007f-\u009f]/.test(value);
const minor = (value: unknown): value is string => typeof value === 'string' && /^(?:0|[1-9]\d{0,19})$/.test(value) && BigInt(value) <= MAX_CASH_MINOR;
const currencyValue = (value: unknown): value is Currency => record(value) && typeof value.code === 'string' && /^[A-Z]{3}$/.test(value.code) && Number.isInteger(value.minor_unit_exponent) && Number(value.minor_unit_exponent) >= 0 && Number(value.minor_unit_exponent) <= 6;
const reasonValue = (value: unknown): value is string => typeof value === 'string' && !cashReasonError(value);
/** Never let an incomplete or misleading saved request become a new cash action. */
export function validLedgerAttempt(value: unknown): value is LedgerAttempt {
  if (!record(value) || !record(value.payload) || !currencyValue(value.currency) || !minor(value.amount) || !identifier(value.label)) return false;
  const p = value.payload;
  const currencyMatches = currencyValue(p.expected_currency) && sameCurrency(value.currency, p.expected_currency);
  switch (value.command) {
    case 'open_cash_session': return identifier(p.idempotency_key) && minor(p.opening_float_minor) && p.opening_float_minor === value.amount && currencyMatches;
    case 'close_cash_session': return identifier(p.session_id) && minor(p.counted_minor) && p.counted_minor === value.amount && minor(p.expected_movement_count);
    case 'refund_cash_sale': return identifier(p.idempotency_key) && identifier(p.sale_id) && reasonValue(p.reason) && identifier(p.expected_session_id) && currencyMatches;
    case 'record_cash_movement': return identifier(p.idempotency_key) && (p.kind === 'paid_in' || p.kind === 'paid_out') && minor(p.amount_minor) && BigInt(p.amount_minor) > 0n && p.amount_minor === value.amount && reasonValue(p.reason) && identifier(p.expected_session_id) && currencyMatches;
    default: return false;
  }
}
export function readLedgerAttempt(storage: Pick<DraftStorage, 'getItem'> = localStorage): LedgerAttempt | null {
  const raw = storage.getItem(LEDGER_PENDING_KEY);
  if (raw === null) return null;
  let value: unknown;
  try { value = JSON.parse(raw); } catch { throw new Error('Saved cash recovery data cannot be read. Keep browser data intact and restore a valid recovery record before continuing.'); }
  if (!validLedgerAttempt(value)) throw new Error('Saved cash recovery data is incomplete or invalid. Do not clear browser storage or record the cash action again; the original request must be recovered.');
  return value;
}
/** Call only inside the real register writer lock. Recovery priority is checkout, cash, then carts. */
export function writeLedgerJournal(storage: DraftStorage, attempt: LedgerAttempt, recovering: boolean) {
  if (!validLedgerAttempt(attempt)) throw new Error('Cash action details are invalid. No cash request was sent.');
  const serialized = JSON.stringify(attempt);
  const existing = storage.getItem(LEDGER_PENDING_KEY);
  if (recovering && existing === null) throw new Error('This cash action was resolved in another window. It has not been sent again. Reload to view the current cash records.');
  if (existing !== null && existing !== serialized) throw new Error('Another window has an unresolved cash action. Its recovery record has been kept intact. Recover it in that window or reload here first.');
  if (storage.getItem('gram.checkout.pending.v1') !== null) throw new Error('A checkout needs recovery before this cash action. Resolve it first.');
  if (!recovering && storage.getItem('gram.draft.pending.v1') !== null) throw new Error('A saved cart action needs recovery before starting a cash action.');
  try { storage.setItem(LEDGER_PENDING_KEY, serialized); }
  catch { throw new Error('This browser could not keep a cash recovery record. No cash request was sent. Make browser storage available, then retry.'); }
}
export function clearLedgerJournal(storage: DraftStorage, attempt: LedgerAttempt) {
  if (storage.getItem(LEDGER_PENDING_KEY) !== JSON.stringify(attempt)) throw new Error('The cash recovery record changed in another window. It has not been removed. Reload to recover the current action.');
  storage.removeItem(LEDGER_PENDING_KEY);
}
const knownRejections = new Set(['INVALID_INPUT', 'NOT_CONFIGURED', 'NO_ACTIVE_SESSION', 'SESSION_ALREADY_OPEN', 'SESSION_CLOSED', 'STALE_SESSION', 'CURRENCY_MISMATCH', 'ALREADY_REFUNDED', 'SALE_NOT_FOUND', 'STALE_CASH_SESSION', 'SESSION_CHANGED', 'SESSION_CURRENCY', 'NO_OPEN_SESSION', 'NOT_FOUND', 'INSUFFICIENT_CASH']);
type Command = <T>(name: string, payload?: unknown) => Promise<T>;
export type LedgerWriterView = { pending: LedgerAttempt | null; busy: boolean; error: string; invalidJournal: boolean; canDiscard: boolean };
/** A single durable request survives a lost response. Every journal change shares the cart/checkout Web Lock. */
export class LedgerWriter {
  view: LedgerWriterView = { pending: null, busy: false, error: '', invalidJournal: false, canDiscard: false };
  private command: Command;
  private storage: DraftStorage;
  private exclusive: DraftLock;
  private listeners = new Set<() => void>();
  constructor(command: Command, storage: DraftStorage, exclusive: DraftLock) {
    this.command = command; this.storage = storage; this.exclusive = exclusive;
    try { this.view.pending = readLedgerAttempt(storage); }
    catch (error) { this.view.invalidJournal = true; this.view.error = error instanceof Error ? error.message : 'Cash recovery is unavailable. Keep browser data intact.'; }
  }
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  snapshot = () => this.view;
  private update(value: Partial<LedgerWriterView>) { this.view = { ...this.view, ...value }; for (const listener of this.listeners) listener(); }
  get blocked() { return this.view.busy || !!this.view.pending || this.view.invalidJournal; }
  clearUnsentError() { if (!this.blocked) this.update({ error: '', canDiscard: false }); }
  async execute(attempt: LedgerAttempt): Promise<boolean> {
    if (this.view.busy) return false;
    if (this.view.invalidJournal) throw new Error(this.view.error);
    if (this.view.pending && JSON.stringify(attempt) !== JSON.stringify(this.view.pending)) throw new Error('Recover the saved cash action before starting a different one.');
    const recovering = !!this.view.pending;
    this.update({ busy: true, error: '', canDiscard: false });
    let knownRejected = false;
    try {
      await this.exclusive(async () => {
        writeLedgerJournal(this.storage, attempt, recovering);
        this.update({ pending: attempt });
        try { await this.command(attempt.command, attempt.payload); }
        catch (error) { knownRejected = record(error) && typeof error.code === 'string' && knownRejections.has(error.code); throw error; }
        clearLedgerJournal(this.storage, attempt);
        this.update({ pending: null, error: '', canDiscard: false });
      });
      return true;
    } catch (error) {
      this.update({ error: error instanceof Error ? error.message : 'The cash action could not be confirmed. Retry the same saved action.', canDiscard: knownRejected });
      throw error;
    } finally { this.update({ busy: false }); }
  }
  async discardRejected(): Promise<boolean> {
    if (this.view.busy || !this.view.canDiscard || !this.view.pending) return false;
    const attempt = this.view.pending;
    this.update({ busy: true });
    try {
      await this.exclusive(async () => { clearLedgerJournal(this.storage, attempt); this.update({ pending: null, error: '', canDiscard: false }); });
      return true;
    } catch (error) { this.update({ error: error instanceof Error ? error.message : 'The saved cash action could not be cleared.' }); throw error; }
    finally { this.update({ busy: false }); }
  }
}
