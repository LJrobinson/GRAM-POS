import { useState } from 'react';
import { Icon } from './Icon';
import { money } from './format';
import { sameCurrency, type CashLedger } from './ledger';
import { printSavedReceipt, receiptTimestamp, receiptView } from './receiptModel';
import type { Receipt } from './types';
import './receipt.css';

/** Only this element is printed. Current refund information stays outside it. */
export function SavedReceipt({ receipt }: { receipt: Receipt }) {
  const view = receiptView(receipt);
  return <div className="receipt-paper" id="print-receipt" data-sale-id={view.saleId}>
    <div className="receipt-brand">GRAM<span>DEMO / TRAINING RECEIPT</span></div>
    <div className="receipt-meta"><strong>{view.number}</strong><span>{view.createdAt}</span></div>
    <div className="saved-badge"><Icon name="check" size={14}/>Saved locally · cash · {view.currency}</div>
    <p className="receipt-snapshot-note">Original saved sale · not a tax receipt</p>
    <div className="receipt-lines">{view.lines.map(line => <div key={line.id}>
      <span><strong>{line.name}</strong><small>{line.quantity}</small><small>SKU: {line.sku}{line.unitPrice && ` · ${line.unitPrice} each`}</small></span>
      <strong>{line.amount}</strong>
    </div>)}</div>
    {view.groups.map(group => <div className="receipt-group" key={group.id}>
      <strong>{group.name}</strong><span>{group.eligible}</span><span>{group.weights}</span><span>{group.tolerance}</span><span>{group.rule}</span><span>Group total: {group.amount}</span>
    </div>)}
    <div className="receipt-totals">
      <div><span>Subtotal</span><strong>{view.total}</strong></div>
      <div className="muted"><span>Tax</span><span>Not configured · demo</span></div>
      <div className="receipt-total"><span>Total</span><strong>{view.total}</strong></div>
      <div><span>Cash received</span><strong>{view.tender}</strong></div>
      <div className="change-row"><span>Change due</span><strong>{view.change}</strong></div>
    </div>
    <p className="receipt-reference">Sale: {view.saleId}<br/>Saved pricing revision: {view.revision}</p>
    <p className="receipt-fine">Training only. Tax is not configured.<br/>Not a tax or compliance receipt.<br/>Later refunds are separate records; this original sale does not change.</p>
  </div>;
}

export function ReceiptContent({ receipt, onClose, ledger, blocked, onRefund }: { receipt: Receipt; onClose: () => void; ledger: CashLedger | null; blocked: boolean; onRefund: () => void }) {
  const [printError, setPrintError] = useState('');
  const refund = ledger?.refunds.find(item => item.sale_id === receipt.sale_id);
  const print = () => {
    setPrintError('');
    try { printSavedReceipt(receipt.sale_id); }
    catch (error) { setPrintError(`${error instanceof Error ? error.message : 'The print dialog could not open.'} No sale or refund records were changed.`); }
  };
  return <>
    <SavedReceipt receipt={receipt}/>
    <div className="refund-status" aria-label="Current refund status">
      <strong>Current refund status · separate from the original sale</strong>
      {!ledger ? <p>Cash records unavailable. Refresh cash records before refunding.</p> : refund ? <>
        <p>Fully refunded · original receipt preserved</p>
        <p>Refund {refund.refund_number} · {money(refund.amount_minor, refund.currency)} cash</p>
        <p>{receiptTimestamp(refund.created_at_unix)} · Original receipt {refund.original_receipt_number}</p>
        <p>Reason: {refund.reason}</p>
      </> : <>
        <p>No refund recorded.</p>
        <button className="secondary full-width" disabled={blocked || !ledger.active_session || !sameCurrency(ledger.active_session.currency, receipt.quote.currency)} onClick={onRefund}>Refund full receipt</button>
        <p>{!ledger.active_session ? 'Open a cash session to refund.' : !sameCurrency(ledger.active_session.currency, receipt.quote.currency) ? 'Refund unavailable: drawer and receipt currencies differ.' : 'Full cash refund only · no stock return'}</p>
      </>}
      {ledger && !ledger.sale_sessions.some(link => link.sale_id === receipt.sale_id) && <p>Legacy sale: not included in drawer sales totals.</p>}
    </div>
    {printError && <div className="error-banner" role="alert">{printError}</div>}
    <div className="receipt-actions"><button className="secondary" onClick={print}><Icon name="receipt" size={18}/>Print receipt</button><button className="primary" onClick={onClose}>Done<Icon name="arrow" size={17}/></button></div>
    <p className="receipt-print-help">Opens your print dialog for the original saved sale only. Choose a printer or PDF and paper size there. Canceling does not change any records.</p>
  </>;
}
