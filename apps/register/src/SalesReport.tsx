import { useEffect, useRef, useState, type FormEvent } from 'react';
import { command } from './api';
import { money } from './format';
import { Icon } from './Icon';
import {
  createReportRequestGuard, reportCount, reportCurrencyKey, reportDateTime,
  reportMatchesRange, reportRangeError, todayReportRange,
  type SalesReport, type SalesReportRange,
} from './reports';
import './reports.css';

export type SalesReportPanelProps = {
  onExport: (range: SalesReportRange) => Promise<string | void>;
  onOpenReceipt?: (saleId: string) => void | Promise<void>;
};

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (error && typeof error === 'object' && 'message' in error) return String(error.message);
  return typeof error === 'string' ? error : 'Could not read saved report records. Please try again.';
}

export function SalesReportPanel({ onExport, onOpenReceipt }: SalesReportPanelProps) {
  const [range, setRange] = useState<SalesReportRange>(() => todayReportRange());
  const [report, setReport] = useState<SalesReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [openingReceipt, setOpeningReceipt] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const requests = useRef(createReportRequestGuard());
  const receiptRequests = useRef(createReportRequestGuard());
  const exportLock = useRef(false);
  const receiptLock = useRef(false);
  const rangeError = reportRangeError(range);
  const currentReport = report && reportMatchesRange(report, range) ? report : null;

  const loadReport = async (selection: SalesReportRange) => {
    if (exportLock.current) return;
    const request = requests.current.begin();
    setReport(null); setError(''); setNotice('');
    const invalid = reportRangeError(selection);
    if (invalid) { setLoading(false); setError(invalid); return; }
    setLoading(true);
    try {
      const result = await command<SalesReport>('sales_report', { ...selection });
      if (!requests.current.isCurrent(request)) return;
      if (!reportMatchesRange(result, selection)) throw new Error('The returned report does not match this UTC date range. Load the report again.');
      setReport(result);
    } catch (cause) {
      if (requests.current.isCurrent(request)) setError(errorText(cause));
    } finally {
      if (requests.current.isCurrent(request)) setLoading(false);
    }
  };

  useEffect(() => {
    void loadReport(range);
    return () => { requests.current.invalidate(); receiptRequests.current.invalidate(); };
    // Loading a new range is explicit. Editing invalidates pending requests immediately.
  }, []);

  const changeRange = (next: SalesReportRange) => {
    if (exportLock.current) return;
    requests.current.invalidate();
    setRange(next); setReport(null); setLoading(false); setError(''); setNotice('');
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    void loadReport(range);
  };
  const openReceipt = async (saleId: string) => {
    if (!onOpenReceipt || receiptLock.current || exportLock.current) return;
    receiptLock.current = true;
    const request = receiptRequests.current.begin();
    setOpeningReceipt(true); setError('');
    try { await onOpenReceipt(saleId); }
    catch (cause) { if (receiptRequests.current.isCurrent(request)) setError(`Could not open receipt: ${errorText(cause)}`); }
    finally { receiptLock.current = false; if (receiptRequests.current.isCurrent(request)) setOpeningReceipt(false); }
  };
  const exportCsv = async () => {
    if (exportLock.current || loading || !currentReport || rangeError) return;
    exportLock.current = true;
    const request = requests.current.begin();
    // Freeze the exact successfully loaded boundaries before calling either export adapter.
    const selection = { start_date: currentReport.start_date, end_date: currentReport.end_date };
    setExporting(true); setError(''); setNotice('');
    try {
      const result = await onExport(selection);
      if (requests.current.isCurrent(request)) setNotice(result || 'CSV download requested. Check your downloads.');
    } catch (cause) {
      if (requests.current.isCurrent(request)) setError(errorText(cause));
    } finally {
      exportLock.current = false;
      if (requests.current.isCurrent(request)) setExporting(false);
    }
  };

  return <section className="standard-page reports-page" aria-label="Sales reports">
    <div className="page-heading">
      <div><span className="eyebrow">SAVED RECORDS. A CLEARER PICTURE.</span><h1>Sales reports<span className="emerald">.</span></h1><p>Original sales. Separate refunds. Exact totals.</p></div>
      <span className="reports-timezone"><Icon name="history" size={16}/> All times in UTC</span>
    </div>

    <form className="reports-range-card" onSubmit={submit} aria-label="Report date range">
      <div className="reports-range-heading"><h2>Choose your reporting window</h2><button type="button" className="text-button" disabled={exporting} onClick={() => { if (exportLock.current) return; const today = todayReportRange(); changeRange(today); void loadReport(today); }}>Today (UTC)</button></div>
      <div className="reports-range-fields">
        <label htmlFor="report-start">Start date (inclusive)<input id="report-start" aria-label="Start date (inclusive)" type="date" min="0001-01-01" max="9999-12-31" required disabled={exporting} value={range.start_date} onChange={event => changeRange({ ...range, start_date: event.target.value })} aria-describedby="report-range-help"/><span>From 00:00 UTC, included</span></label>
        <label htmlFor="report-end">End date (exclusive)<input id="report-end" aria-label="End date (exclusive)" type="date" min="0001-01-01" max="9999-12-31" required disabled={exporting} value={range.end_date} onChange={event => changeRange({ ...range, end_date: event.target.value })} aria-describedby="report-range-help"/><span>Until 00:00 UTC, excluded</span></label>
        <button type="submit" className="primary" disabled={loading || exporting || !!rangeError}><Icon name="refresh" size={16}/>{loading ? 'Loading report…' : 'Load report'}</button>
      </div>
      <p id="report-range-help" className="reports-help">To include a whole day, choose the following day as the end date. This is a UTC window, not your device’s local time.</p>
      {rangeError && <p className="reports-range-error" role="alert">{rangeError}</p>}
    </form>

    {error && <p className="error-banner" role="alert">{error}</p>}
    {notice && <p className="success-banner" role="status">{notice}</p>}
    {openingReceipt && <p className="reports-help" role="status">Opening saved receipt…</p>}
    {loading && <div className="reports-empty" role="status"><Icon name="refresh" size={28}/><h2>Reading saved records…</h2><p>Sales and refunds are grouped by their own recorded times.</p></div>}
    {!loading && !currentReport && !error && <div className="reports-empty" role="status"><Icon name="history" size={28}/><h2>Ready for a fresh view</h2><p>Load this date range to see its saved sales and refunds.</p></div>}

    {currentReport && <>
      <div className="reports-result-heading">
        <div><h2>Report overview</h2><p><span>{currentReport.start_date} 00:00 UTC (inclusive)</span><span className="reports-boundary-arrow" aria-hidden="true"> → </span><span>{currentReport.end_date} 00:00 UTC (exclusive)</span></p></div>
        <button type="button" className="secondary" disabled={exporting || loading} onClick={() => void exportCsv()}><Icon name="receipt" size={16}/>{exporting ? 'Preparing report CSV…' : 'Export report CSV'}</button>
      </div>
      {!currentReport.events.length ? <div className="reports-empty"><div className="reports-empty-icon"><Icon name="receipt" size={28}/></div><h2>No activity in this range</h2><p>No saved sales or refunds fall within these UTC boundaries.<br/>Choose another window or export the empty report.</p></div> : <>
        <section className="reports-summary" aria-label="Totals by currency">
          {currentReport.groups.map(group => <article className="reports-currency" key={reportCurrencyKey(group.currency)} data-testid={`report-currency-${reportCurrencyKey(group.currency)}`}>
            <header><h3>{group.currency.code} <span>· {group.currency.minor_unit_exponent} decimal places</span></h3><span className="reports-counts">{reportCount(group.sale_count)} {group.sale_count === '1' ? 'sale' : 'sales'} · {reportCount(group.refund_count)} {group.refund_count === '1' ? 'refund' : 'refunds'}</span></header>
            <dl><div><dt>Gross sales</dt><dd data-testid="gross-sales">{money(group.gross_sales_minor, group.currency)}</dd><span>Original sale amounts</span></div><div className="reports-refunds"><dt>Refunds</dt><dd data-testid="refunds">{money(group.refunds_minor, group.currency)}</dd><span>Recorded separately</span></div><div className="reports-net"><dt>Net sales</dt><dd data-testid="net-sales">{money(group.net_sales_minor, group.currency)}</dd><span>Gross sales − refunds</span></div></dl>
          </article>)}
        </section>
        <p className="reports-explanation"><Icon name="info" size={16}/>Currencies and stored decimal precision are never combined. Refunds use their own recorded date, even if the original sale falls outside this window.</p>
        <div className="reports-events-heading"><h2>Recorded activity</h2><span>Oldest first · UTC</span></div>
        <div className="reports-table-scroll" role="region" aria-label="Report activity table" tabIndex={0}>
          <table className="reports-table" aria-label="Sales and refunds in selected UTC range">
            <thead><tr><th scope="col">Recorded (UTC)</th><th scope="col">Record</th><th scope="col">Session</th><th scope="col">Currency</th><th scope="col" className="reports-amount">Amount</th></tr></thead>
            <tbody>{currentReport.events.map(event => <tr key={`${event.kind}-${event.event_id}`} data-testid="report-event" data-event-kind={event.kind} data-event-id={event.event_id}>
              <td className="reports-recorded">{reportDateTime(event.created_at_unix)}</td>
              <td><span className={`reports-kind reports-kind-${event.kind}`}>{event.kind === 'sale' ? 'Sale' : 'Refund'}</span><span className="reports-event-id">#{event.event_id}</span><div>{onOpenReceipt ? <button type="button" className="reports-receipt-link" disabled={exporting || openingReceipt} onClick={() => void openReceipt(event.sale_id)} aria-label={`Open receipt ${event.sale_id}`}>{event.kind === 'refund' ? 'Original receipt' : 'View receipt'} #{event.sale_id}<Icon name="chevron" size={12}/></button> : <span className="reports-receipt-text">Receipt #{event.sale_id}</span>}</div>{event.reason !== null && <p className="reports-reason">{event.reason}</p>}</td>
              <td>{event.session_id === null ? <span className="reports-legacy">Legacy · no session</span> : `#${event.session_id}`}</td>
              <td>{event.currency.code}<small>{event.currency.minor_unit_exponent} decimal places</small></td>
              <td className={`reports-amount ${event.kind === 'refund' ? 'reports-refund-amount' : ''}`}>{event.kind === 'refund' ? '−' : ''}{money(event.total_minor, event.currency)}</td>
            </tr>)}</tbody>
          </table>
        </div>
      </>}
      <aside className="reports-footnote"><Icon name="shield" size={18}/><div><strong>Training records, not a tax filing.</strong><p>Read-only totals from saved receipts and refunds. No drawer float, tender or change is counted as revenue. CSV is generated from saved records at export time, so newly recorded activity may appear. Exports are unencrypted; keep them private. Import amount and ID columns as text in a spreadsheet to preserve every digit.</p></div></aside>
    </>}
  </section>;
}
