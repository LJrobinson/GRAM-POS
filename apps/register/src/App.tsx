import { useEffect, useRef, useState, type ReactNode } from 'react';
import { command, RegisterError } from './api';
import { BarcodeScanner } from './BarcodeScanner';
import { SalesReportPanel } from './SalesReport';
import { exportSalesReport } from './reportExport';
import { LedgerForm, LedgerPanel, type LedgerAction } from './CashLedger';
import { readLedgerAttempt, sameCurrency, type CashLedger, type LedgerAttempt } from './ledger';
import { ReceiptContent } from './Receipt';
import { Icon } from './Icon';
import { activeCatalog, newGroup } from './catalog';
import { CatalogEditor, ExportPanel, BlankSetup } from './CatalogEditor';
import { dateTime, minorToInput, money, parseTender, quantityLabel, ruleNames, validCount, validGrams } from './format';
import type { CartLine, CatalogItem, CheckoutPayload, Config, GroupConfig, Quote, Receipt, RegisterState, SaleSummary } from './types';

const PENDING_KEY = 'gram.checkout.pending.v1';
const knownUncommitted = new Set(['INVALID_INPUT', 'INVALID_CONFIG', 'NOT_CONFIGURED', 'INSUFFICIENT_TENDER', 'STALE_QUOTE', 'PRICE_ERROR', 'ARCHIVED_PRODUCT', 'NO_OPEN_SESSION', 'SESSION_CHANGED', 'SESSION_CURRENCY', 'SESSION_CLOSED']);
function readPending(): CheckoutPayload | null { try { return JSON.parse(localStorage.getItem(PENDING_KEY) || 'null'); } catch { return null; } }
function errorText(error: unknown) { return error instanceof Error ? error.message : 'Something went wrong. Please try again.'; }
function emptyCart() { return <div className="empty-cart"><div className="empty-symbol"><Icon name="bag" size={29}/></div><h3>A fresh start.</h3><p>Choose a product to begin.<br/>Every gram, accounted for.</p></div>; }

function Modal({ title, subtitle, children, onClose, wide = false, className = '' }: { title: string; subtitle?: string; children: ReactNode; onClose?: () => void; wide?: boolean; className?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    ref.current?.focus();
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && onClose) onClose();
      if (event.key === 'Tab') {
        const nodes = ref.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex="0"]');
        if (!nodes?.length) return;
        const first = nodes[0], last = nodes[nodes.length - 1];
        if (event.shiftKey && (document.activeElement === first || document.activeElement === ref.current)) { event.preventDefault(); last.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
      }
    };
    document.addEventListener('keydown', key);
    return () => { document.removeEventListener('keydown', key); previous?.focus(); };
  }, [onClose]);
  return <div className={`modal-backdrop ${className}`}><div role="dialog" aria-modal="true" aria-label={title} tabIndex={-1} ref={ref} className={`modal ${wide ? 'modal-wide' : ''}`}><div className="modal-head"><div><span className="eyebrow">GRAM / REGISTER</span><h2>{title}</h2>{subtitle && <p>{subtitle}</p>}</div>{onClose && <button className="icon-button" aria-label="Close dialog" onClick={onClose}><Icon name="close"/></button>}</div>{children}</div></div>;
}

export default function App() {
  const [state, setState] = useState<RegisterState | null>(null);
  const [booting, setBooting] = useState(true);
  const [bootError, setBootError] = useState('');
  const [page, setCurrentPage] = useState<'register' | 'history' | 'settings' | 'cash' | 'reports'>('register');
  const [ledger, setLedger] = useState<CashLedger | null>(null);
  const [ledgerError, setLedgerError] = useState('');
  const [ledgerAction, setLedgerAction] = useState<LedgerAction | null>(null);
  const [ledgerAttempt, setLedgerAttempt] = useState<LedgerAttempt | null>(readLedgerAttempt);
  const refreshLedger = async () => { try { setLedger(await command<CashLedger>('get_cash_ledger')); setLedgerError(''); } catch (e) { setLedger(null); setLedgerError(errorText(e)); } };
  const [settingsDirty, setSettingsDirty] = useState(false);
  const reportReceiptRequest = useRef(0);
  const pageRef = useRef(page);
  const scannerResetToken = useRef(0);
  const setPage = (next: typeof page) => { if (next !== page && settingsDirty && !window.confirm('Discard unsaved configuration changes and leave Configuration?')) return; if (next !== page) { scannerResetToken.current += 1; setSettingsDirty(false); reportReceiptRequest.current += 1; } pageRef.current = next; setCurrentPage(next); };
  useEffect(() => { const warn = (event: BeforeUnloadEvent) => { if (settingsDirty) { event.preventDefault(); event.returnValue = ''; } }; window.addEventListener('beforeunload', warn); return () => window.removeEventListener('beforeunload', warn); }, [settingsDirty]);
  const [lines, setLines] = useState<CartLine[]>([]);
  const [quote, setQuote] = useState<Quote | null>(null);
  const [pricing, setPricing] = useState(false);
  const [quoteError, setQuoteError] = useState('');
  const [draftError, setDraftError] = useState('');
  const [filter, setFilter] = useState('all');
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<CatalogItem | null>(null);
  const [checkout, setCheckout] = useState(false);
  const [attempt, setAttempt] = useState<CheckoutPayload | null>(readPending);
  const [receipt, setReceipt] = useState<Receipt | null>(null);
  const [demoLoading, setDemoLoading] = useState(false);
  const [notice, setNotice] = useState('');
  const saveQueue = useRef<Promise<unknown>>(Promise.resolve());
  const draftTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const checkoutInFlight = useRef(false);
  const loadState = async () => {
    setBooting(true); setBootError('');
    try {
      const next = await command<RegisterState>('get_state');
      setState(next);
      if (next.configured) await refreshLedger();
      const pending = readPending();
      if (pending) {
        const result = await command<{ receipt: Receipt | null }>('get_sale_by_key', { idempotency_key: pending.idempotency_key });
        if (result.receipt) { localStorage.removeItem(PENDING_KEY); setAttempt(null); setReceipt(result.receipt); setLines(next.draft.lines); }
        else { setAttempt(pending); setLines(pending.lines); setCheckout(true); }
      } else setLines(next.draft.lines);
    } catch (error) { setBootError(errorText(error)); }
    finally { setBooting(false); }
  };
  useEffect(() => { void loadState(); }, []);
  useEffect(() => { if (state?.configured && (page === 'cash' || page === 'history' || page === 'reports')) void refreshLedger(); }, [page, state?.configured]);
  useEffect(() => {
    if (booting || !state?.configured) return;
    if (lines.length === 0) { setQuote(null); setPricing(false); setQuoteError(''); return; }
    let live = true;
    setPricing(true); setQuoteError('');
    command<Quote>('quote_cart', { lines }).then(value => { if (live) setQuote(value); }).catch(error => { if (live) { setQuote(null); setQuoteError(errorText(error)); } }).finally(() => { if (live) setPricing(false); });
    return () => { live = false; };
  }, [lines, state?.config_revision, booting]);
  useEffect(() => {
    if (booting || !state?.configured || attempt) return;
    let live = true;
    const timer = setTimeout(() => {
      draftTimer.current = null;
      saveQueue.current = saveQueue.current.catch(() => {}).then(() => command('save_draft', { lines })).then(() => { if (live) setDraftError(''); }).catch(error => { if (live) setDraftError(`Draft not saved: ${errorText(error)}`); });
    }, 300);
    draftTimer.current = timer;
    return () => { live = false; clearTimeout(timer); if (draftTimer.current === timer) draftTimer.current = null; };
  }, [lines, booting, state?.configured, attempt]);
  const changeCart = (next: CartLine[]) => { if (attempt) return; setQuote(null); setLines(next); };
  const addProduct = (product: CatalogItem, value: string) => {
    if (attempt) return;
    const line: CartLine = { id: crypto.randomUUID(), sku: product.sku, quantity: product.kind === 'measured' ? { kind: 'measured', grams: value } : { kind: product.kind, count: value } };
    setQuote(null); setLines(current => [...current, line]);
    setSelected(null);
  };
  const initialize = async (currency?: Config['currency']) => {
    setDemoLoading(true); setNotice('');
    try { const next = await command<RegisterState>(currency ? 'initialize_empty' : 'initialize_demo', currency ? { currency } : {}); setState(next); await refreshLedger(); setLines(next.draft.lines); if (currency) setPage('settings'); }
    catch (error) { setNotice(errorText(error)); }
    finally { setDemoLoading(false); }
  };
  const finishSale = async (tender: string) => {
    if (checkoutInFlight.current) return;
    if (!attempt && (!ledger?.active_session || ledgerAttempt)) throw new Error('Open a cash session and resolve pending cash actions first.');
    if (!quote && !attempt) throw new Error('Wait for a current quote before checkout.');
    checkoutInFlight.current = true;
    if (draftTimer.current) { clearTimeout(draftTimer.current); draftTimer.current = null; }
    try {
      const payload: CheckoutPayload = attempt || { idempotency_key: crypto.randomUUID(), lines, tender_minor: tender, expected_total_minor: quote!.total_minor, expected_config_revision: quote!.config_revision, expected_session_id: ledger!.active_session!.session_id, expected_currency: quote!.currency };
      localStorage.setItem(PENDING_KEY, JSON.stringify(payload));
      setAttempt(payload);
      await saveQueue.current.catch(() => {});
      const paid = await command<Receipt>('complete_cash_sale', payload);
      setReceipt(paid); setCheckout(false); setLines([]); setQuote(null);
      localStorage.removeItem(PENDING_KEY); setAttempt(null); await refreshLedger();
    } finally { checkoutInFlight.current = false; }
  };
  const reviseAttempt = () => { localStorage.removeItem(PENDING_KEY); setAttempt(null); setCheckout(false); setQuote(null); setLines(current => [...current]); };
  const catalog = activeCatalog(state?.catalog || []);
  const visible = catalog.filter(item => (filter === 'all' || item.kind === filter) && `${item.name} ${item.sku}`.toLowerCase().includes(query.toLowerCase()));
  const currency = state?.config?.currency;
  return <div className="app-shell">
    <aside className="sidebar">
      <a className="brand" href="#" onClick={event => { event.preventDefault(); setPage('register'); }} aria-label="GRAM register"><span className="brand-mark">g<span>•</span></span><span className="brand-name">GRAM<span>POINT OF SALE</span></span></a>
      <div className="workspace-label">YOUR WORKSPACE</div>
      <nav aria-label="Main navigation">{([['register', 'register', 'Register'], ['history', 'history', 'Sales history'], ['cash', 'cash', 'Cash sessions'], ['reports', 'receipt', 'Reports'], ['settings', 'settings', 'Configuration']] as const).map(([id, icon, text]) => <button key={id} aria-label={text} className={`nav-item ${page === id ? 'active' : ''}`} onClick={() => setPage(id)}><Icon name={icon}/><span>{text}</span>{page === id && <span className="active-dot"/>}</button>)}</nav>
      <div className="sidebar-bottom"><div className="local-note"><span className={`status-dot ${bootError ? 'offline' : ''}`}/><div><strong>{bootError ? 'Service unavailable' : 'Local-first register'}</strong><span>Your data stays on this device</span></div></div><div className="station"><span className="station-avatar">01</span><div><strong>Register 01</strong><span>Demo workspace</span></div><Icon name="lock" size={15}/></div></div>
    </aside>
    <main className="main-content">
      <header className="topbar"><div className="breadcrumb">WORKSPACE <span>/</span> {page === 'register' ? 'REGISTER' : page === 'history' ? 'SALES HISTORY' : page === 'cash' ? 'CASH SESSIONS' : page === 'reports' ? 'REPORTS' : 'CONFIGURATION'}</div><div className="topbar-right"><span className="training-badge"><span/>TRAINING MODE</span><span className="today">{new Date().toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })}</span></div></header>
      {booting ? <div className="full-state"><div className="loading-wheel"/><h2>Opening your register</h2><p>Connecting to your local workspace…</p></div> : bootError ? <div className="full-state"><Icon name="info" size={40}/><h2>Let’s reconnect.</h2><p>{bootError}</p><button className="primary" onClick={() => void loadState()}><Icon name="refresh"/>Try again</button><small>Browser development needs the local Rust service on port 4174.</small></div> : !state?.configured ? <Welcome loading={demoLoading} error={notice} onInitialize={() => void initialize()} onBlank={currency => void initialize(currency)}/> : page === 'history' ? <History onReceipt={saved => { setLedger(null); setReceipt(saved); void refreshLedger(); }} ledger={ledger}/> : page === 'cash' ? <LedgerPanel ledger={ledger} error={ledgerError} onRefresh={() => void refreshLedger()} onAction={setLedgerAction} currency={currency!} blocked={!!attempt || !!ledgerAttempt}/> : page === 'reports' ? <SalesReportPanel onExport={exportSalesReport} onOpenReceipt={async saleId => { const request = ++reportReceiptRequest.current; const saved = await command<Receipt>('get_receipt', { sale_id: saleId }); if (request !== reportReceiptRequest.current || pageRef.current !== 'reports') return; await refreshLedger(); if (request === reportReceiptRequest.current && pageRef.current === 'reports') setReceipt(saved); }}/> : page === 'settings' ? <Settings onDirtyChange={setSettingsDirty} revision={state.config_revision} config={state.config!} hasCart={lines.length > 0 || !!attempt} onSave={next => { setState(next); setNotice('Saved configuration loaded. New quotes use these rules.'); }}/> : <div className="register-layout">
        <section className="catalog-area">
          <div className="page-heading"><div><span className="eyebrow">A LITTLE PRECISION. A LOT OF POSSIBILITY.</span><h1>Let’s ring it up<span className="emerald">.</span></h1><p>Measured by weight. Priced with confidence.</p></div><span className="catalog-count">{catalog.length} products</span></div>
          <div className="drawer-status"><span>{ledger?.active_session ? `Cash session open · ${money(ledger.active_session.expected_cash_minor, ledger.active_session.currency)} expected` : ledgerError ? 'Cash session unavailable' : 'Open a cash session before checkout'}</span><button className="secondary" onClick={() => setPage('cash')}>Manage cash</button></div><div className="search-field"><Icon name="search"/><input aria-label="Search products" placeholder="Search products or SKU…" value={query} onChange={event => setQuery(event.target.value)}/>{query && <button className="icon-button compact" aria-label="Clear search" onClick={() => setQuery('')}><Icon name="close" size={16}/></button>}<span className="search-hint">CATALOG</span></div>
          <BarcodeScanner resetToken={scannerResetToken} enabled={!attempt && !selected && !checkout && !receipt && !ledgerAction && !ledgerAttempt} revision={state.config_revision} onProduct={product => { if (product.kind === 'measured') { setSelected(product); return false; } addProduct(product, '1'); return true; }}/>
          <div className="filter-row" role="group" aria-label="Product category"><button className={filter === 'all' ? 'filter active' : 'filter'} onClick={() => setFilter('all')}>All products <span>{catalog.length}</span></button><button className={filter === 'measured' ? 'filter active' : 'filter'} onClick={() => setFilter('measured')}>Deli / by weight</button><button className={filter === 'prepacked' ? 'filter active' : 'filter'} onClick={() => setFilter('prepacked')}>Prepacked</button><button className={filter === 'unit' ? 'filter active' : 'filter'} onClick={() => setFilter('unit')}>Units / each</button></div>
          {visible.length ? <div className="product-grid">{visible.map((product, index) => <ProductCard key={product.sku} product={product} index={index} config={state.config!} disabled={!!attempt} onClick={() => { scannerResetToken.current += 1; setSelected(product); }}/>)}</div> : <div className="no-results"><Icon name="search" size={32}/><h3>No products found</h3><p>{catalog.length ? 'Try a different name or clear your filters.' : 'Add or unarchive products in Configuration to begin.'}</p><button className="secondary" onClick={() => { setQuery(''); setFilter('all'); }}>Clear filters</button></div>}
          <div className="precision-note"><div className="precision-icon"><Icon name="scale" size={22}/></div><div><strong>Different products. One thoughtful price.</strong><p>Eligible products share a weight tier while keeping their own sale lines.</p></div><span>EXACT BY DESIGN</span></div>
          <div className="compliance-note"><Icon name="info" size={16}/><span>Training only. Taxes, compliance and inventory tracking are not configured. Do not use for live sales.</span></div>
        </section>
        <aside className="cart-panel" aria-label="Current sale">
          <div className="cart-heading"><div><span className="eyebrow">CURRENT SALE</span><h2>Your cart <span>{lines.length}</span></h2></div><button className="text-button" disabled={!lines.length || !!attempt} onClick={() => { scannerResetToken.current += 1; changeCart([]); }}>Clear</button></div>
          <div className="cart-items">{!lines.length ? emptyCart() : lines.map(line => { const item = catalog.find(item => item.sku === line.sku); const priced = quote?.lines.find(item => item.id === line.id); return <div className="cart-line" key={line.id}><div className={`line-icon ${line.quantity.kind}`}><Icon name={line.quantity.kind === 'measured' ? 'jar' : 'box'} size={20}/></div><div className="line-details"><strong>{item?.name || line.sku}</strong><span>{quantityLabel(line.quantity, item?.declared_content_grams)}</span><small>{item?.pricing.kind === 'weight_group' ? 'Shared weight pricing' : item?.kind === 'unit' ? 'Fixed unit price' : 'Fixed package price'}</small></div><div className="line-end"><strong>{priced ? money(priced.amount_minor, currency) : '—'}</strong><button className="remove-line" disabled={!!attempt} aria-label={`Remove ${item?.name || line.sku}`} onClick={() => changeCart(lines.filter(item => item.id !== line.id))}>Remove</button></div></div>; })}
            {quote && quote.groups.length > 0 && <div className="group-breakdown"><div className="breakdown-title"><Icon name="scale" size={16}/><span>YOUR WEIGHT PRICING</span></div>{quote.groups.map(group => <details key={group.group_id}><summary><div><strong>{group.schedule.name || group.group_id}</strong><span>{group.eligible_grams} g eligible · {group.selected_nominal_grams} g tier</span></div><span>{money(group.amount_minor, quote.currency)}<Icon name="chevron" size={14}/></span></summary><dl><div><dt>Actual measured</dt><dd>{group.actual_measured_grams} g</dd></div><div><dt>Declared pack content</dt><dd>{group.declared_package_grams} g</dd></div><div><dt>Tier tolerance</dt><dd>+{group.tolerance_grams} g</dd></div><div><dt>Billable weight</dt><dd>{group.billable_grams} g</dd></div></dl><p>{ruleNames[group.rule] || group.rule}</p></details>)}</div>}
          </div>
          <div className="cart-summary">{quoteError && <div className="error-banner" role="alert">{quoteError}</div>}{draftError && <div className="error-banner" role="alert">{draftError}</div>}{attempt && <div className="warning-banner">A checkout needs to be resolved. Retry it safely before editing this cart.</div>}<div className="summary-row"><span>Subtotal</span><strong>{quote ? money(quote.total_minor, currency) : lines.length ? '—' : money('0', currency)}</strong></div><div className="summary-row muted"><span>Tax</span><span className="tax-label">Not configured · demo</span></div><div className="total-row"><span>Total</span><strong aria-live="polite">{pricing ? <span className="calculating">Pricing…</span> : quote ? money(quote.total_minor, currency) : lines.length ? '—' : money('0', currency)}</strong></div><button className="checkout-button" disabled={!!ledgerAttempt || (!attempt && (!quote || pricing || !!quoteError || !ledger?.active_session || !sameCurrency(ledger.active_session.currency, currency!)))} onClick={() => { scannerResetToken.current += 1; setCheckout(true); }}><Icon name="cash"/><span>{attempt ? 'Resume checkout' : 'Charge cash'}</span>{quote && <strong>{money(quote.total_minor, currency)}</strong>}<Icon name="arrow" size={18}/></button><div className="checkout-footnote"><Icon name="shield" size={13}/><span>Exact pricing. Saved locally.</span></div></div>
        </aside>
      </div>}
      {notice && state?.configured && page !== 'register' && <div className="toast" role="status">{notice}<button className="icon-button compact" aria-label="Dismiss notification" onClick={() => setNotice('')}><Icon name="close" size={16}/></button></div>}
    </main>
    {(ledgerAction || ledgerAttempt) && <Modal title={ledgerAttempt ? 'Recover cash action' : ledgerAction?.kind === 'refund' ? 'Full receipt refund' : ledgerAction?.kind === 'close' ? 'Close cash session' : 'Open cash session'}><LedgerForm action={ledgerAction} pending={ledgerAttempt} onPending={setLedgerAttempt} onCancel={() => { setLedgerAction(null); void refreshLedger(); }} onDone={message => { setLedgerAction(null); setNotice(message); setPage('cash'); void refreshLedger(); }}/></Modal>}
    {selected && <ProductModal product={selected} config={state!.config!} onClose={() => setSelected(null)} onAdd={value => addProduct(selected, value)}/>}
    {checkout && (quote || attempt) && <CashCheckout total={attempt?.expected_total_minor || quote!.total_minor} currency={attempt?.expected_currency || currency!} attempt={attempt} onClose={() => setCheckout(false)} onSubmit={finishSale} onRevise={reviseAttempt}/>}
    {receipt && <ReceiptModal key={receipt.sale_id} receipt={receipt} ledger={ledger} blocked={!!attempt || !!ledgerAttempt} onRefund={() => { if (ledger?.active_session) { setLedgerAction({ kind: 'refund', receipt, session: ledger.active_session }); setReceipt(null); } }} onClose={() => setReceipt(null)}/>}
  </div>;
}

function Welcome({ loading, error, onInitialize, onBlank }: { loading: boolean; error: string; onInitialize: () => void; onBlank: (currency: Config['currency']) => void }) {
  return <section className="welcome"><span className="eyebrow">WELCOME TO GRAM</span><div className="welcome-content"><div><h1>Good things.<br/>Precisely measured<span className="emerald">.</span></h1><p>A calmer way to ring up measured goods.<br/>Exact weight pricing. Clear receipts. All on your device.</p><button className="primary" disabled={loading} onClick={onInitialize}>{loading ? 'Preparing your register…' : 'Load demo register'}<Icon name="arrow"/></button><small>Explicit demo setup. No business pricing is assumed.</small><BlankSetup loading={loading} onInitialize={onBlank}/>{error && <div role="alert" className="error-banner">{error}</div>}</div><div className="welcome-art"><div className="art-orbit one"/><div className="art-orbit two"/><div className="art-center"><Icon name="scale" size={72}/><span>EXACT BY DESIGN</span></div><span className="art-tag"><Icon name="check" size={16}/>Every gram counts.</span></div></div><div className="welcome-features"><div><Icon name="scale"/><h3>Weight, without guesswork</h3><p>Measured grams and declared package content stay distinct.</p></div><div><Icon name="receipt"/><h3>Transparent from the start</h3><p>See the tier, tolerance and pricing rules behind each sale.</p></div><div><Icon name="lock"/><h3>Local by nature</h3><p>Saved drafts and committed receipts on this device.</p></div></div><div className="setup-disclaimer"><Icon name="info"/><p>This build is a training register. Demo prices are examples, not commercial defaults. Tax, compliance, live payments and inventory are not configured.</p></div></section>;
}

function ProductCard({ product, config, index, disabled, onClick }: { product: CatalogItem; config: Config; index: number; disabled: boolean; onClick: () => void }) {
  const group = product.pricing.kind === 'weight_group' ? config.groups.find(group => group.id === (product.pricing as { group_id: string }).group_id) : null;
  return <button className={`product-card tone-${index % 4}`} disabled={disabled} onClick={onClick}><div className="product-card-top"><span className="product-type">{product.kind === 'measured' ? 'DELI / BY WEIGHT' : product.kind === 'unit' ? 'UNIT / EACH' : 'PREPACKED'}</span><span className="product-add"><Icon name="plus" size={18}/></span></div><div className="product-emblem"><Icon name={product.kind === 'measured' ? 'jar' : 'box'} size={42}/><span>{product.kind === 'measured' ? 'NET WT.' : product.kind === 'unit' ? 'EACH' : `${product.declared_content_grams} g`}</span></div><div className="product-card-body"><h3>{product.name}</h3><p>{product.kind === 'measured' ? 'Enter measured grams' : product.kind === 'unit' ? 'Sold by whole-item count' : `${product.declared_content_grams} g declared per pack`}</p><div className="product-card-bottom"><span className={`pricing-tag ${group ? '' : 'fixed'}`}>{group ? group.name : 'Fixed price'}</span><strong>{product.pricing.kind === 'fixed_unit' ? money(product.pricing.unit_price_minor, config.currency) : 'Tiered'}<Icon name="chevron" size={14}/></strong></div></div></button>;
}

function ProductModal({ product, config, onClose, onAdd }: { product: CatalogItem; config: Config; onClose: () => void; onAdd: (value: string) => void }) {
  const [value, setValue] = useState(product.kind === 'measured' ? '' : '1');
  const measured = product.kind === 'measured';
  const valid = measured ? validGrams(value) : validCount(value);
  const group = product.pricing.kind === 'weight_group' ? config.groups.find(group => group.id === (product.pricing as { group_id: string }).group_id) : null;
  return <Modal title={product.name} subtitle={measured ? 'Record the actual net weight from your scale.' : product.kind === 'unit' ? 'Sold by count. No weight is assigned to this item.' : `${product.declared_content_grams} g declared content per package. This is not a scale measurement.`} onClose={onClose}><form onSubmit={event => { event.preventDefault(); if (valid) onAdd(value); }}><label className="field-label" htmlFor="quantity">{measured ? 'Measured weight' : product.kind === 'unit' ? 'Number of units' : 'Number of packages'}</label><div className="quantity-input"><input id="quantity" autoFocus inputMode={measured ? 'decimal' : 'numeric'} placeholder={measured ? '0.000' : '1'} value={value} onChange={event => setValue(event.target.value)} autoComplete="off"/><span>{measured ? 'grams' : product.kind === 'unit' ? 'units' : 'packs'}</span></div><p className="input-help">{measured ? 'Up to 3 decimal places. Your exact entry is preserved on the sale.' : product.kind === 'unit' ? 'Whole units only, starting at 1.' : 'Whole packages only. Declared content is kept separate from measured weight.'}</p>{group && <div className="tier-preview"><span className="eyebrow">{group.name} · SHARED WEIGHT TIERS</span><div>{group.tiers.map(tier => <span key={tier.nominal_grams}><strong>{tier.nominal_grams} g</strong>{money(tier.price_minor, config.currency)}<small>+{tier.tolerance_grams} g tolerance</small></span>)}</div><p>Final cart pricing is calculated by the exact Rust pricing engine.</p></div>}{product.pricing.kind === 'fixed_unit' && <div className="fixed-price-note">{product.kind === 'unit' ? 'Fixed unit price' : 'Fixed package price'} <strong>{money(product.pricing.unit_price_minor, config.currency)}</strong></div>}{value && !valid && <p className="field-error">{measured ? 'Enter a positive weight with no more than 3 decimal places.' : `Enter a whole number of ${product.kind === 'unit' ? 'units' : 'packages'}, starting at 1.`}</p>}<button className="primary full-width" type="submit" disabled={!valid}>Add to cart<Icon name="plus"/></button></form></Modal>;
}

function CashCheckout({ total, currency, attempt, onClose, onSubmit, onRevise }: { total: string; currency: Config['currency']; attempt: CheckoutPayload | null; onClose: () => void; onSubmit: (minor: string) => Promise<void>; onRevise: () => void }) {
  const [value, setValue] = useState(attempt ? minorToInput(attempt.tender_minor, currency.minor_unit_exponent) : '');
  const [busy, setBusy] = useState(false);
  const [issue, setIssue] = useState('');
  const legacyPending = !!attempt && !attempt.expected_session_id;
  const [canRevise, setCanRevise] = useState(legacyPending);
  const minor = parseTender(value, currency.minor_unit_exponent);
  const valid = minor !== null && BigInt(minor) >= BigInt(total);
  const submit = async () => { if (busy || legacyPending || (!valid && !attempt)) return; setBusy(true); setIssue(''); try { await onSubmit(attempt?.tender_minor || minor!); } catch (error) { setIssue(errorText(error)); setCanRevise(error instanceof RegisterError && knownUncommitted.has(error.code)); } finally { setBusy(false); } };
  return <Modal title="Cash checkout" subtitle="Training sale · no live payment is collected" onClose={!busy && !attempt ? onClose : undefined}><div className="checkout-total"><span>Amount due</span><strong>{money(total, currency)}</strong><small>Tax not configured · demo only</small></div><label className="field-label" htmlFor="cash-received">Cash received ({currency.code})</label><div className="quantity-input cash-input"><span>{currency.code === 'USD' ? '$' : currency.code}</span><input id="cash-received" autoFocus inputMode="decimal" value={value} placeholder="0.00" disabled={busy || !!attempt} onChange={event => setValue(event.target.value)}/></div><div className="cash-shortcuts"><button className="secondary" disabled={busy || !!attempt} onClick={() => setValue(minorToInput(total, currency.minor_unit_exponent))}>Exact amount</button>{['20', '50', '100'].map(amount => <button key={amount} className="secondary" disabled={busy || !!attempt || BigInt(amount) * 10n ** BigInt(currency.minor_unit_exponent) < BigInt(total)} onClick={() => setValue(amount)}>{currency.code === 'USD' ? '$' : currency.code + ' '}{amount}</button>)}</div>{minor && !valid && <p className="field-error">Cash received must cover the amount due.</p>}{value && !minor && <p className="field-error">Enter a valid amount with up to {currency.minor_unit_exponent} decimal places.</p>}{issue && <div className="error-banner" role="alert">{issue}</div>}{legacyPending && <div className="warning-banner">This older checkout was not committed and has no drawer snapshot. Return to the cart, open a cash session, and review a new checkout.</div>}{attempt && !busy && !legacyPending && <div className="warning-banner">{canRevise ? 'This attempt did not save a sale. You can revise it safely.' : 'This checkout is protected against duplicate sales. Retrying uses the same payment details and checkout key.'}</div>}<button className="primary full-width" disabled={busy || legacyPending || (!attempt && !valid)} onClick={() => void submit()}><Icon name={busy ? 'lock' : 'check'}/>{busy ? 'Saving sale…' : attempt ? 'Retry same checkout' : 'Complete cash sale'}</button>{canRevise && <button className="secondary full-width revise" onClick={onRevise}>Return to cart and revise</button>}<p className="checkout-explainer"><Icon name="shield" size={16}/>A receipt appears only after your sale is saved. Change is calculated by the register service.</p></Modal>;
}

function History({ onReceipt, ledger }: { onReceipt: (receipt: Receipt) => void; ledger: CashLedger | null }) {
  const [sales, setSales] = useState<SaleSummary[]>([]);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState('');
  const [opening, setOpening] = useState('');
  const load = async () => { setBusy(true); setError(''); try { setSales((await command<{ sales: SaleSummary[] }>('list_sales')).sales); } catch (error) { setError(errorText(error)); } finally { setBusy(false); } };
  useEffect(() => { void load(); }, []);
  const open = async (id: string) => { setOpening(id); setError(''); try { onReceipt(await command<Receipt>('get_receipt', { sale_id: id })); } catch (error) { setError(errorText(error)); } finally { setOpening(''); } };
  return <section className="standard-page"><div className="page-heading"><div><span className="eyebrow">THE RECORD, KEPT STRAIGHT.</span><h1>Sales history<span className="emerald">.</span></h1><p>Saved training sales, with every original detail intact.</p></div><button className="secondary" disabled={busy} onClick={() => void load()}><Icon name="refresh" size={17}/>Refresh</button></div>{error && <div className="error-banner" role="alert">{error}</div>}{busy ? <div className="full-state"><div className="loading-wheel"/><p>Loading saved sales…</p></div> : !sales.length ? <div className="history-empty"><div className="empty-symbol"><Icon name="receipt" size={32}/></div><h2>Your story starts with a sale.</h2><p>Complete a cash checkout and its saved receipt will appear here.</p></div> : <div className="sales-table"><div className="sales-table-head"><span>RECEIPT</span><span>DATE & TIME</span><span>ITEMS</span><span>TOTAL</span><span/></div>{sales.map(sale => <button className="sale-row" key={sale.sale_id} disabled={!!opening} onClick={() => void open(sale.sale_id)}><span><span className="receipt-icon"><Icon name="receipt" size={20}/></span><strong>{sale.receipt_number}</strong><small>{ledger?.sale_sessions.some(link => link.sale_id === sale.sale_id) ? 'Cash session sale' : ledger ? 'Legacy · not drawer activity' : 'Cash session status unavailable'}{ledger?.refunds.some(r => r.sale_id === sale.sale_id) ? ' · Refunded' : ''}</small></span><span>{dateTime(sale.created_at_unix)}</span><span>{sale.line_count} lines</span><strong>{money(sale.total_minor, sale.currency)}</strong><Icon name={opening === sale.sale_id ? 'refresh' : 'chevron'} size={18}/></button>)}</div>}<div className="compliance-note"><Icon name="lock" size={15}/>Receipt snapshots are saved locally and do not change when pricing rules are edited.</div></section>;
}

function ReceiptModal({ receipt, onClose, ledger, blocked, onRefund }: { receipt: Receipt; onClose: () => void; ledger: CashLedger | null; blocked: boolean; onRefund: () => void }) {
  return <Modal title="Sale saved." subtitle="Your local receipt is ready." onClose={onClose} className="receipt-modal-backdrop"><ReceiptContent receipt={receipt} ledger={ledger} blocked={blocked} onRefund={onRefund} onClose={onClose}/></Modal>;
}

function Settings({ config, revision, hasCart, onSave, onDirtyChange }: { onDirtyChange: (dirty: boolean) => void; config: Config; revision: string; hasCart: boolean; onSave: (state: RegisterState) => void }) {
  const [draft, setDraft] = useState<Config>(() => structuredClone(config));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [saved, setSaved] = useState(false);
  const [dirty, setDirty] = useState(false);
  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);
  const [baseRevision, setBaseRevision] = useState(revision);
  const updateDraft = (next: Config) => { setDraft(next); setSaved(false); setDirty(true); };
  const reload = async () => { setBusy(true); setError(''); try { const next = await command<RegisterState>('get_state'); setDraft(structuredClone(next.config!)); setBaseRevision(next.config_revision); setDirty(false); setSaved(false); onSave(next); } catch (error) { setError(errorText(error)); } finally { setBusy(false); } };
  const updateGroup = (index: number, next: GroupConfig) => { setDraft(current => ({ ...current, groups: current.groups.map((group, i) => i === index ? next : group) })); setSaved(false); setDirty(true); };
  const save = async () => { if (busy || hasCart) return; setBusy(true); setError(''); try { const result = await command<RegisterState>('put_config', { config: draft, expected_config_revision: baseRevision }); setDraft(structuredClone(result.config!)); setBaseRevision(result.config_revision); onSave(result); setSaved(true); setDirty(false); } catch (error) { setError(errorText(error)); } finally { setBusy(false); } };
  return <section className="standard-page settings-page"><div className="page-heading"><div><span className="eyebrow">YOUR RULES. NOTHING ASSUMED.</span><h1>Configuration<span className="emerald">.</span></h1><p>Explicit pricing policies for this training workspace.</p></div><button className="primary" disabled={busy || hasCart || !dirty} onClick={() => void save()}><Icon name="check" size={18}/>{busy ? 'Saving…' : 'Save configuration'}</button></div><div className="settings-warning"><Icon name="info"/><div><strong>Demo configuration only</strong><p>Currency: {draft.currency.code}. Tax is not configured. These settings do not enable live trading or regulatory compliance.</p></div></div>{hasCart && <div className="warning-banner">Finish or clear the current cart before editing pricing. This prevents changing the rules during a sale.</div>}{error && <div className="error-banner" role="alert">{error}</div>}{saved && <div className="success-banner" role="status">Configuration saved and validated by the pricing engine.</div>}<div className="configuration-actions"><button className="secondary" disabled={busy || hasCart} onClick={() => { if (!dirty || window.confirm('Discard unsaved configuration changes and reload?')) void reload(); }}>Reload saved configuration</button>{dirty && <span>Unsaved changes · save to apply</span>}</div><fieldset disabled={busy || hasCart} className="settings-fields"><CatalogEditor config={draft} savedProductCount={config.catalog.length} onChange={updateDraft}/><div className="settings-section-heading"><h2>Weight pricing groups</h2><button className="secondary" onClick={() => updateDraft({ ...draft, groups: [...draft.groups, newGroup()] })}>Add pricing group</button></div><p className="input-help">Choose every pricing policy explicitly before saving. No policy is selected for a new group. Tolerance applies once per group, not per cart row.</p>{draft.groups.map((group, groupIndex) => <section className="settings-card" key={groupIndex}><div className="settings-card-title"><span className="settings-card-icon"><Icon name="scale"/></span><div><h2>{group.name}</h2><p>Shared weight pricing · {group.id}</p></div><span className="chip">Aggregate group</span></div><div className="editor-grid"><label>Group ID<input aria-label={`Group ID ${groupIndex + 1}`} value={group.id} onChange={event => { const id = event.target.value; updateDraft({ ...draft, groups: draft.groups.map((g, i) => i === groupIndex ? { ...g, id } : g), catalog: draft.catalog.map(item => item.pricing.kind === 'weight_group' && item.pricing.group_id === group.id && group.id !== '' ? { ...item, pricing: { kind: 'weight_group', group_id: id } } : item) }); }}/></label><label>Group name<input aria-label={`Group name ${groupIndex + 1}`} value={group.name} onChange={event => updateGroup(groupIndex, { ...group, name: event.target.value })}/></label></div><button className="text-button" disabled={draft.catalog.some(item => item.pricing.kind === 'weight_group' && item.pricing.group_id === group.id)} onClick={() => updateDraft({ ...draft, groups: draft.groups.filter((_, i) => i !== groupIndex) })}>Remove pricing group {groupIndex + 1}</button><p className="input-help">Reassign all products, including archived products, before removing a group.</p><div className="tier-table"><div className="tier-table-head"><span>NOMINAL GRAMS</span><span>TOLERANCE +GRAMS</span><span>PRICE ({draft.currency.code} MINOR UNITS)</span><span/></div>{group.tiers.map((tier, tierIndex) => <div className="tier-edit-row" key={tierIndex}>{(['nominal_grams', 'tolerance_grams', 'price_minor'] as const).map(key => <label key={key}><span className="sr-only">{key.replaceAll('_', ' ')} tier {tierIndex + 1}</span><input inputMode={key === 'price_minor' ? 'numeric' : 'decimal'} value={tier[key]} onChange={event => updateGroup(groupIndex, { ...group, tiers: group.tiers.map((item, i) => i === tierIndex ? { ...item, [key]: event.target.value } : item) })}/></label>)}<button className="icon-button" aria-label={`Remove tier ${tierIndex + 1}`} disabled={group.tiers.length === 1} onClick={() => updateGroup(groupIndex, { ...group, tiers: group.tiers.filter((_, i) => i !== tierIndex) })}><Icon name="close" size={16}/></button></div>)}</div><button className="text-button add-tier" onClick={() => updateGroup(groupIndex, { ...group, tiers: [...group.tiers, { nominal_grams: '', tolerance_grams: '', price_minor: '' }] })}><Icon name="plus" size={15}/>Add tier</button><p className="input-help">Prices use integer minor units: 2500 = {money('2500', draft.currency)}. Tiers must increase and tolerance bands cannot overlap.</p><div className="policy-grid"><label>Outside tolerance<select aria-label="Outside tolerance" value={group.overage_policy} onChange={event => updateGroup(groupIndex, { ...group, overage_policy: event.target.value as GroupConfig['overage_policy'] })}><option value="">Choose overage policy</option><option value="full_actual_at_tier_rate">Price full weight at tier rate</option><option value="excess_beyond_tolerance_at_tier_rate">Tier + excess beyond tolerance</option></select></label><label>Money rounding<select aria-label="Money rounding" value={group.rounding} onChange={event => updateGroup(groupIndex, { ...group, rounding: event.target.value as GroupConfig['rounding'] })}><option value="">Choose rounding policy</option><option value="floor">Round down (floor)</option><option value="ceil">Round up (ceiling)</option><option value="half_up">Nearest, halves up</option><option value="half_even">Nearest, halves to even</option></select></label><label>Below the smallest tier<select aria-label="Below the smallest tier" value={group.below_smallest} onChange={event => updateGroup(groupIndex, { ...group, below_smallest: event.target.value as GroupConfig['below_smallest'] })}><option value="">Choose below-minimum policy</option><option value="proportional_at_smallest_tier">Proportional at smallest tier</option><option value="reject">Reject the sale</option></select></label></div></section>)}</fieldset><ExportPanel disabled={busy} hasUnsavedChanges={dirty}/></section>;
}
