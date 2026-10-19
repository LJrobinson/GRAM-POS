import { useState } from 'react';
import { money } from './format';
import { reportDateTime } from './reports';
import { cashMovementError, cashReasonError, parseCashAmount, sameCurrency, variance, type CashLedger, type CashMovementKind, type CashSession, type LedgerAttempt, type LedgerWriter, type LedgerWriterView } from './ledger';
import type { Currency, Receipt } from './types';

export type LedgerAction = { kind: 'open'; currency: Currency } | { kind: 'close'; session: CashSession } | { kind: 'refund'; receipt: Receipt; session: CashSession } | { kind: CashMovementKind; session: CashSession };
const movementLabel = (kind: CashMovementKind) => kind === 'paid_in' ? 'Paid in' : 'Paid out';
function CashTime({ unix, timezone }: { unix: string; timezone: string }) {
  const date = new Date(Number(unix) * 1000);
  return <time dateTime={Number.isFinite(date.getTime()) ? date.toISOString() : undefined}>{reportDateTime(unix, timezone)}</time>;
}
export function LedgerPanel({ ledger, error, onRefresh, onAction, currency, blocked, timezone = 'UTC' }: { ledger: CashLedger | null; error: string; onRefresh: () => void; onAction: (action: LedgerAction) => void; currency: Currency; blocked: boolean; timezone?: string }) {
  const active = ledger?.active_session;
  return <section className="standard-page cash-page">
    <div className="page-heading"><div><span className="eyebrow">CASH, ACCOUNTED FOR.</span><h1>Cash sessions<span className="emerald">.</span></h1><p>Training drawer records. Closed counts and cash movements are permanent.</p></div><button className="secondary" onClick={onRefresh}>Refresh cash records</button></div>
    {error && <div className="error-banner" role="alert">{error}</div>}
    {!ledger ? <p>Cash records unavailable. Refresh before taking cash.</p> : <>
      <section className="settings-card"><h2>{active ? 'Open drawer session' : 'No open cash session'}</h2>{active ? <>
        <p>Opened <CashTime unix={active.opened_at_unix} timezone={timezone}/> · {active.currency.code}</p>
        <SessionTotals session={active}/>
        <div className="cash-session-actions"><button className="secondary" disabled={blocked} onClick={() => onAction({ kind: 'paid_in', session: active })}>Paid in</button><button className="secondary" disabled={blocked} onClick={() => onAction({ kind: 'paid_out', session: active })}>Paid out</button><button className="primary" disabled={blocked} onClick={() => onAction({ kind: 'close', session: active })}>Count and close session</button></div>
        <p className="cash-accounting-note">Paid in and paid out adjust the drawer only. They are not sales, refunds, or revenue.</p>
      </> : <><p>Enter the starting float before making cash sales, refunds, or drawer movements.</p><button className="primary" disabled={blocked} onClick={() => onAction({ kind: 'open', currency })}>Open cash session</button></>}</section>
      <h2>Cash movements</h2><p className="cash-section-note">Permanent drawer-only records · times shown in {timezone}.</p>
      {(ledger.cash_movements ?? []).map(movement => <article className={`settings-card cash-movement ${movement.kind}`} data-testid="cash-movement" key={movement.movement_id}>
        <div className="cash-movement-heading"><strong>{movementLabel(movement.kind)}</strong><strong>{movement.kind === 'paid_out' ? '−' : '+'}{money(movement.amount_minor, movement.currency)}</strong></div>
        <p className="cash-movement-reason">{movement.reason}</p><p><CashTime unix={movement.created_at_unix} timezone={timezone}/></p>
        <dl className="cash-movement-meta"><div><dt>Movement ID</dt><dd>{movement.movement_id}</dd></div><div><dt>Session ID</dt><dd>{movement.session_id}</dd></div></dl><small>Immutable cash movement · not a sale or refund</small>
      </article>)}
      {!(ledger.cash_movements ?? []).length && <p className="cash-section-note">No paid-in or paid-out movements recorded.</p>}
      <h2>Session history</h2>
      {ledger.sessions.filter(session => session.closed_at_unix).map(session => <details className="settings-card" key={session.session_id}>
        <summary>Closed <CashTime unix={session.closed_at_unix!} timezone={timezone}/> · {money(session.counted_minor!, session.currency)} counted · variance {money(session.variance_minor!, session.currency)}</summary>
        <p>Opened <CashTime unix={session.opened_at_unix} timezone={timezone}/> · {session.session_id}</p><SessionTotals session={session}/><p>Immutable closing record. Counts cannot be edited.</p>
      </details>)}
      {!ledger.sessions.some(session => session.closed_at_unix) && <p className="cash-section-note">No closed sessions yet.</p>}
      <h2>Refund records</h2>{ledger.refunds.map(refund => <div className="settings-card" key={refund.refund_id}><strong>{refund.refund_number} · {money(refund.amount_minor, refund.currency)}</strong><p>Original receipt {refund.original_receipt_number} · <CashTime unix={refund.created_at_unix} timezone={timezone}/></p><p>{refund.reason}</p><small>Full cash refund · no stock return or inventory change</small></div>)}{!ledger.refunds.length && <p>No refunds recorded.</p>}
    </>}
  </section>;
}
function SessionTotals({ session: s }: { session: CashSession }) {
  return <dl className="ledger-totals">
    <div><dt>Opening float</dt><dd>{money(s.opening_float_minor, s.currency)}</dd></div>
    <div><dt>Cash sales (net of change)</dt><dd>{money(s.sales_total_minor, s.currency)}</dd></div>
    <div><dt>Cash refunds</dt><dd>−{money(s.refunds_total_minor, s.currency)}</dd></div>
    <div><dt>Paid in</dt><dd>+{money(s.paid_in_total_minor ?? '0', s.currency)}</dd></div>
    <div><dt>Paid out</dt><dd>−{money(s.paid_out_total_minor ?? '0', s.currency)}</dd></div>
    <div className="cash-expected-total"><dt>Expected cash</dt><dd>{money(s.expected_cash_minor, s.currency)}</dd></div>
    {s.closed_at_unix && <><div><dt>Counted cash</dt><dd>{money(s.counted_minor!, s.currency)}</dd></div><div><dt>Variance (counted − expected)</dt><dd>{money(s.variance_minor!, s.currency)}</dd></div></>}
  </dl>;
}
export function LedgerForm({ action, writer, view, onDone, onCancel }: { action: LedgerAction | null; writer: LedgerWriter; view: LedgerWriterView; onDone: (message: string) => void; onCancel: () => void }) {
  const [value, setValue] = useState('');
  const [reason, setReason] = useState('');
  const [review, setReview] = useState(false);
  const { pending, busy, error, canDiscard } = view;
  const kind = pending ? pending.command === 'open_cash_session' ? 'open' : pending.command === 'close_cash_session' ? 'close' : pending.command === 'record_cash_movement' ? pending.payload.kind as CashMovementKind : 'refund' : action!.kind;
  const movement = kind === 'paid_in' || kind === 'paid_out';
  const currency = pending?.currency || (action?.kind === 'open' ? action.currency : action!.session.currency);
  const amount = pending?.amount ?? (action?.kind === 'refund' ? action.receipt.quote.total_minor : parseCashAmount(value, currency.minor_unit_exponent, movement));
  const movementIssue = action && (action.kind === 'paid_in' || action.kind === 'paid_out') ? cashMovementError(action.kind, amount, reason, action.session) : '';
  const reasonIssue = (kind === 'refund' || movement) ? cashReasonError(reason) : '';
  const valid = amount !== null && !movementIssue && !reasonIssue && (action?.kind !== 'refund' || sameCurrency(action.receipt.quote.currency, action.session.currency)) && (action?.kind === 'open' || !action || action.session.closed_at_unix === null);
  const session = action && action.kind !== 'open' ? action.session : null;
  const afterAmount = movement && session && amount !== null ? (BigInt(session.expected_cash_minor) + (kind === 'paid_in' ? BigInt(amount) : -BigInt(amount))).toString() : null;
  const submit = async () => {
    if (busy || (!pending && (!review || !valid))) return;
    const attempt: LedgerAttempt = pending || {
      command: movement ? 'record_cash_movement' : kind === 'open' ? 'open_cash_session' : kind === 'close' ? 'close_cash_session' : 'refund_cash_sale',
      currency, amount: amount!, label: movement ? movementLabel(kind as CashMovementKind) : action?.kind === 'refund' ? action.receipt.receipt_number : kind === 'close' ? 'Close drawer session' : 'Open drawer session',
      payload: action?.kind === 'open' ? { idempotency_key: crypto.randomUUID(), opening_float_minor: amount, expected_currency: currency }
        : action?.kind === 'close' ? { session_id: action.session.session_id, counted_minor: amount, expected_movement_count: action.session.movement_count }
        : action?.kind === 'refund' ? { idempotency_key: crypto.randomUUID(), sale_id: action.receipt.sale_id, reason: reason.trim(), expected_session_id: action.session.session_id, expected_currency: action.receipt.quote.currency }
        : action && (action.kind === 'paid_in' || action.kind === 'paid_out') ? { idempotency_key: crypto.randomUUID(), kind: action.kind, amount_minor: amount, reason: reason.trim(), expected_session_id: action.session.session_id, expected_currency: action.session.currency } : {},
    };
    try {
      if (await writer.execute(attempt)) onDone(movement ? `${movementLabel(kind as CashMovementKind)} saved. Drawer updated; sales and revenue are unchanged.` : kind === 'refund' ? 'Full cash refund saved. Original receipt unchanged. No stock was returned.' : kind === 'close' ? 'Cash session closed. The count and variance are saved permanently.' : 'Cash session opened. Ready for training sales.');
    } catch { /* The writer retains and displays the exact recovery state. */ }
  };
  return <div className="ledger-form">
    <p className="warning-banner">Training only. No live payment is moved. {movement ? 'This is a permanent drawer adjustment, not a sale, refund, or revenue.' : kind === 'refund' ? 'Full original-receipt amount only. This does not return stock.' : kind === 'close' ? 'Closing is final. Recount before confirming.' : 'Count the cash already in the drawer.'}</p>
    {pending ? <><h3>{busy ? 'Saving cash action' : 'Resolve saved cash action'}</h3><p>{pending.label} · {money(pending.amount, pending.currency)}</p>{typeof pending.payload.reason === 'string' && <p className="cash-movement-reason">Reason: {pending.payload.reason}</p>}<p className="cash-recovery-identity">{typeof pending.payload.expected_session_id === 'string' && <>Session: {pending.payload.expected_session_id}<br/></>}{typeof pending.payload.idempotency_key === 'string' && <>Action key: {pending.payload.idempotency_key}</>}</p><p>Its outcome may be uncertain. Retry uses the same saved details, even after restart. Do not record this action again.</p></>
      : <>
        {action?.kind === 'refund' ? <><h3>{action.receipt.receipt_number} · {money(amount!, currency)}</h3><p>The original receipt and prices stay unchanged.</p></>
          : <><label className="field-label" htmlFor="drawer-amount">{movement ? 'Amount' : kind === 'open' ? 'Opening float' : 'Counted cash'} ({currency.code})</label><div className="quantity-input"><input id="drawer-amount" inputMode="decimal" maxLength={64} disabled={review || busy} value={value} onChange={event => setValue(event.target.value)} aria-describedby="cash-amount-help"/></div><p id="cash-amount-help" className="input-help">{movement ? 'Greater than zero. ' : ''}Use {currency.minor_unit_exponent === 0 ? 'whole amounts only' : `up to ${currency.minor_unit_exponent} decimal places`}; amounts are never rounded.</p>{value && amount === null && <p className="field-error">Enter a valid {movement ? 'positive ' : ''}amount within the supported range and currency precision.</p>}</>}
        {(movement || kind === 'refund') && <><label className="field-label" htmlFor={movement ? 'cash-reason' : 'refund-reason'}>{movement ? 'Reason' : 'Refund reason'}</label><input id={movement ? 'cash-reason' : 'refund-reason'} maxLength={500} disabled={review || busy} value={reason} onChange={event => setReason(event.target.value)} aria-describedby="cash-reason-help"/><p id="cash-reason-help" className="input-help">Required, single-line reason. Maximum 500 UTF-8 bytes.</p>{reason && reasonIssue && <p className="field-error">{reasonIssue}</p>}</>}
        {movement && session && <div className="cash-movement-preview"><div><span>Current expected cash</span><strong>{money(session.expected_cash_minor, currency)}</strong></div>{afterAmount !== null && <div><span>Expected after this action</span><strong>{money(afterAmount, currency)}</strong></div>}<p>Paid out is limited to the drawer’s expected cash. The register checks again when you confirm.</p></div>}
        {movement && value && amount !== null && movementIssue && !reasonIssue && <p className="field-error" role="alert">{movementIssue}</p>}
        {action?.kind === 'close' && <><SessionTotals session={action.session}/>{amount !== null && <p className="closing-variance">Variance: {money(variance(amount, action.session.expected_cash_minor), currency)}</p>}</>}
      </>}
    {error && <div className="error-banner" role="alert">{error}</div>}
    {review && !pending && <div className="warning-banner cash-review"><strong>Review: {movement ? `${movementLabel(kind as CashMovementKind)} · ` : ''}{money(amount!, currency)}</strong>{(movement || kind === 'refund') && <p>Reason: {reason.trim()}</p>}{movement && session && <p>Session: {session.session_id}</p>}<p>{movement ? 'Save this permanent cash movement? Sales, refunds, and revenue will not change.' : kind === 'close' ? 'Save this final count and close the drawer?' : kind === 'refund' ? 'Record this full cash refund?' : 'Open the drawer with this float?'}</p></div>}
    {!review && !pending ? <button className="primary full-width" disabled={!valid || busy} onClick={() => setReview(true)}>Review {movement ? movementLabel(kind as CashMovementKind).toLowerCase() : kind === 'refund' ? 'full refund' : kind === 'close' ? 'closing count' : 'opening float'}</button>
      : <button className="primary full-width" disabled={busy || (!pending && !valid)} onClick={() => void submit()}>{busy ? 'Saving cash record…' : pending ? 'Retry same cash action' : movement ? `Confirm ${movementLabel(kind as CashMovementKind).toLowerCase()}` : kind === 'refund' ? 'Confirm full cash refund' : kind === 'close' ? 'Confirm and close session' : 'Confirm opening float'}</button>}
    {!pending && review && <button className="secondary full-width" disabled={busy} onClick={() => setReview(false)}>Edit details</button>}
    {(!pending || canDiscard) && !busy && <button className="secondary full-width" onClick={() => { if (!pending) onCancel(); else void writer.discardRejected().then(cleared => { if (cleared) onCancel(); }).catch(() => {}); }}>{pending ? 'Discard rejected cash action' : 'Cancel'}</button>}
  </div>;
}
