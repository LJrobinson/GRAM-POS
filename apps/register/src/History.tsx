import { useEffect, useRef, useState } from 'react';
import { command } from './api';
import { Icon } from './Icon';
import { money } from './format';
import { filterSales } from './history';
import { createReportRequestGuard, reportDateTime } from './reports';
import type { CashLedger } from './ledger';
import type { Receipt, SaleSummary } from './types';

export function History({ onReceipt, ledger, timezone }: { onReceipt: (receipt: Receipt) => void; ledger: CashLedger | null; timezone: string }) {
  const [sales, setSales] = useState<SaleSummary[]>([]);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState('');
  const [opening, setOpening] = useState('');
  const [query, setQuery] = useState('');
  const loadGuard = useRef(createReportRequestGuard()).current;
  const receiptGuard = useRef(createReportRequestGuard()).current;
  const searchInput = useRef<HTMLInputElement>(null);
  const cancelOpening = () => { receiptGuard.invalidate(); setOpening(''); };
  const load = async () => {
    cancelOpening();
    const request = loadGuard.begin();
    setBusy(true); setError(''); setSales([]);
    try {
      // The service returns every saved sale, with no recent-only limit.
      const result = await command<{ sales: SaleSummary[] }>('list_sales');
      if (loadGuard.isCurrent(request)) setSales(result.sales);
    } catch (error) {
      if (loadGuard.isCurrent(request)) setError(error instanceof Error ? error.message : 'Could not load saved sales.');
    } finally { if (loadGuard.isCurrent(request)) setBusy(false); }
  };
  useEffect(() => {
    void load();
    return () => { loadGuard.invalidate(); receiptGuard.invalidate(); };
  }, []);
  const changeQuery = (value: string) => { cancelOpening(); setQuery(value); };
  const clear = () => { changeQuery(''); searchInput.current?.focus(); };
  const open = async (id: string) => {
    const request = receiptGuard.begin();
    setOpening(id); setError('');
    try {
      const receipt = await command<Receipt>('get_receipt', { sale_id: id });
      if (receiptGuard.isCurrent(request)) onReceipt(receipt);
    } catch (error) {
      if (receiptGuard.isCurrent(request)) setError(error instanceof Error ? error.message : 'Could not open this receipt.');
    } finally { if (receiptGuard.isCurrent(request)) setOpening(''); }
  };
  const visible = filterSales(sales, query);
  return <section className="standard-page history-page">
    <div className="page-heading"><div><span className="eyebrow">THE RECORD, KEPT STRAIGHT.</span><h1>Sales history<span className="emerald">.</span></h1><p>Saved training sales, with every original detail intact.</p></div><button className="secondary" disabled={busy} onClick={() => void load()}><Icon name="refresh" size={17}/>Refresh</button></div>
    <div className="history-search">
      <label className="field-label" htmlFor="receipt-search">Find a saved receipt</label>
      <div className="history-search-controls"><div className="search-field"><Icon name="search" size={18}/><input id="receipt-search" ref={searchInput} type="search" value={query} onChange={event => changeQuery(event.target.value)} placeholder="Receipt number or sale ID" aria-describedby="receipt-search-help"/></div><button className="secondary" disabled={!query} onClick={clear}>Clear search</button></div>
      <p id="receipt-search-help" className="input-help">Search all saved sales on this device by full or partial receipt number or sale ID. Product names are not searched.</p>
      <p className="history-zone">History times: {timezone}. Original receipt snapshots keep their saved timezone.</p>
    </div>
    {error && <div className="error-banner" role="alert">{error}</div>}
    {busy ? <div className="full-state"><div className="loading-wheel"/><p>Loading saved sales…</p></div> : error && !sales.length ? <p className="input-help">Saved sales are unavailable. Refresh to try again.</p> : <>
      <p role="status" className="history-count">{visible.length} of {sales.length} saved sales</p>
      {!sales.length ? <div className="history-empty"><div className="empty-symbol"><Icon name="receipt" size={32}/></div><h2>Your story starts with a sale.</h2><p>Complete a cash checkout and its saved receipt will appear here.</p></div> : !visible.length ? <div className="history-empty"><h2>No matching receipts.</h2><p>Try another receipt number or sale ID, or clear the search to see every saved sale.</p><button className="secondary" onClick={clear}>Show all saved sales</button></div> : <div className="sales-table">
        <div className="sales-table-head"><span>RECEIPT</span><span>DATE & TIME</span><span>ITEMS</span><span>TOTAL</span><span/></div>
        {visible.map(sale => <button className="sale-row" key={sale.sale_id} disabled={!!opening} onClick={() => void open(sale.sale_id)}>
          <span><span className="receipt-icon"><Icon name="receipt" size={20}/></span><strong>{sale.receipt_number}</strong><small>Sale ID: {sale.sale_id}</small><small>{ledger?.sale_sessions.some(link => link.sale_id === sale.sale_id) ? 'Cash session sale' : ledger ? 'Legacy · not drawer activity' : 'Cash session status unavailable'}{ledger?.refunds.some(r => r.sale_id === sale.sale_id) ? ' · Refunded' : ''}</small></span>
          <span className="history-time">{reportDateTime(sale.created_at_unix, timezone)}</span><span>{sale.line_count} lines</span><strong>{money(sale.total_minor, sale.currency)}</strong><Icon name={opening === sale.sale_id ? 'refresh' : 'chevron'} size={18}/>
        </button>)}
      </div>}
    </>}
    <div className="compliance-note"><Icon name="lock" size={15}/>Receipt snapshots are saved locally and do not change when pricing rules are edited.</div>
  </section>;
}
