import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { command } from './api';
import { canApplyCatalogCsv, canRefreshCatalogCsv, catalogReviewFields, createImportGuard, readCatalogCsv, type CatalogCsvFile, type CatalogCsvPreview } from './catalogCsv';
import type { ExportFile } from './CatalogEditor';
import type { CatalogItem, RegisterState } from './types';

function ProductReview({ title, item }: { title: string; item: CatalogItem }) {
  return <section className="csv-product-review"><h4>{title}</h4><dl>{catalogReviewFields(item).map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl></section>;
}

function message(error: unknown) { return error instanceof Error ? error.message : error && typeof error === 'object' && 'message' in error ? String(error.message) : typeof error === 'string' ? error : 'The catalog operation could not be completed.'; }
export function CatalogImport({ revision, dirty, disabled, onBusyChange, onApplied }: {
  revision: string; dirty: boolean; disabled: boolean;
  onBusyChange: (busy: boolean) => void; onApplied: (state: RegisterState) => void;
}) {
  const [file, setFile] = useState<CatalogCsvFile | null>(null);
  const [preview, setPreview] = useState<CatalogCsvPreview | null>(null);
  const [busy, setBusy] = useState('');
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [needsRefresh, setNeedsRefresh] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const guard = useRef(createImportGuard()).current;
  const operation = useRef('');
  const previewToken = useRef(-1);
  const context = `${revision}:${dirty}:${disabled}`;
  guard.context(context);
  useEffect(() => { setPreview(null); if (operation.current === 'read' || operation.current === 'preview') { operation.current = ''; setBusy(''); } }, [context]);
  useEffect(() => () => { guard.invalidate(); }, [guard]);
  const begin = (kind: string, lock = false) => { operation.current = kind; setBusy(kind); if (lock) onBusyChange(true); };
  const finish = (lock = false) => { operation.current = ''; setBusy(''); if (lock) onBusyChange(false); };
  const cancel = () => { if (operation.current && !['read', 'preview'].includes(operation.current)) return; guard.invalidate(); setFile(null); setPreview(null); setError(''); setNotice('Import selection cleared.'); finish(); if (input.current) input.current.value = ''; };
  const choose = async (selected?: File) => {
    if ((operation.current && !['read', 'preview'].includes(operation.current)) || disabled || dirty || needsRefresh) return;
    const token = guard.invalidate(); setFile(null); setPreview(null); setError(''); setNotice(''); begin('read');
    try {
      const next = selected ? await readCatalogCsv(selected) : await invoke<CatalogCsvFile | null>('select_catalog_csv');
      if (!guard.current(token)) return;
      setFile(next); if (!next) setNotice('Selection cancelled. No catalog changes were applied.');
    } catch (e) { if (guard.current(token)) setError(message(e)); }
    finally { if (guard.current(token)) finish(); }
  };
  const review = async () => {
    if (!file || operation.current || disabled || dirty || needsRefresh) return;
    const token = guard.invalidate(); begin('preview'); setPreview(null); setError(''); setNotice('');
    try { const result = await command<CatalogCsvPreview>('preview_catalog_csv', { csv: file.content }); if (guard.current(token)) { previewToken.current = token; setPreview(result); } }
    catch (e) { if (guard.current(token)) setError(message(e)); }
    finally { if (guard.current(token)) finish(); }
  };
  const refresh = async () => {
    if (!canRefreshCatalogCsv(dirty, disabled, !!operation.current)) return;
    begin('refresh', true); setError('');
    try { onApplied(await command<RegisterState>('get_state')); setNeedsRefresh(false); setNotice('Saved catalog refreshed. Preview the file again before applying.'); }
    catch (e) { setError(`Could not refresh the saved catalog. ${message(e)}`); }
    finally { finish(true); }
  };
  const apply = async () => {
    if (operation.current || !guard.current(previewToken.current) || !file || !canApplyCatalogCsv(preview, revision, dirty || disabled || needsRefresh)) return;
    const reviewed = preview!; guard.invalidate(); setPreview(null); begin('apply', true); setError(''); setNotice('');
    try { const result = await command<RegisterState>('apply_catalog_csv', { csv: file.content, expected_config_revision: reviewed.expected_config_revision }); onApplied(result); setFile(null); if (input.current) input.current.value = ''; setNotice(`Catalog applied: ${reviewed.created} created, ${reviewed.updated} updated, ${reviewed.unchanged} unchanged.`); }
    catch (e) {
      setError(`${message(e)} The result may be uncertain. This preview cannot be applied again.`);
      setNeedsRefresh(true);
      try { onApplied(await command<RegisterState>('get_state')); setNeedsRefresh(false); setNotice('Saved catalog refreshed. Review a new preview before applying again.'); }
      catch { setNotice('Refresh the saved catalog before previewing or applying again.'); }
    } finally { finish(true); }
  };
  const exportCsv = async (kind: 'catalog_csv' | 'catalog_template') => {
    if (operation.current || disabled) return;
    begin(kind); setError(''); setNotice('');
    try {
      if ('__TAURI_INTERNALS__' in window) { const path = await invoke<string | null>('save_export', { kind }); setNotice(path ? `CSV saved to ${path}` : 'Export cancelled. No file was saved.'); }
      else {
        const exported = await command<ExportFile>(kind === 'catalog_csv' ? 'export_catalog_csv' : 'catalog_csv_template');
        const url = URL.createObjectURL(new Blob([exported.content], { type: exported.media_type }));
        try { const anchor = document.createElement('a'); anchor.href = url; anchor.download = exported.filename; document.body.appendChild(anchor); anchor.click(); anchor.remove(); }
        finally { setTimeout(() => URL.revokeObjectURL(url), 1000); }
        setNotice('CSV download requested. Check your browser downloads.');
      }
    } catch (e) { setError(message(e)); } finally { finish(); }
  };
  return <section className="settings-card catalog-import" aria-label="Catalog CSV">
    <h2>Catalog CSV</h2>
    <p>Import products by exact SKU only. Existing SKUs are updated; new SKUs are created. Names and barcodes never match products. Missing rows are kept, not deleted. Pricing groups must already exist.</p>
    <p className="input-help">Use the template or exported catalog. Keep the gram_catalog_v1 format and text: prefix on nonempty text cells (SKU, name, barcode and group ID). This reversible encoding protects spreadsheet text, including leading zeros. UTF-8 only, up to 1 MiB. Preview never saves changes.</p><details className="csv-format-help"><summary>How to add a unit product</summary><p>Use one row per product. Set format to gram_catalog_v1, sku to text:SOAP-001, name to text:Soap, kind to unit, archived to false, pricing_kind to fixed_unit, and unit_price_minor to 250. Leave barcode, declared_content_grams and group_id empty. Prices are integer minor units in your saved currency. For a barcode such as 00123, enter text:00123. Keep all ten template columns. Use your spreadsheet’s CSV export to quote commas, quotes or multiline names correctly.</p></details>
    {dirty && <p className="warning-banner">Save or discard unsaved configuration changes before importing or refreshing the saved catalog. Exports contain saved data only.</p>}
    <div className="configuration-actions"><button className="secondary" disabled={disabled || !!busy} onClick={() => void exportCsv('catalog_csv')}>Export catalog CSV</button><button className="secondary" disabled={disabled || !!busy} onClick={() => void exportCsv('catalog_template')}>Download CSV template</button></div>
    {'__TAURI_INTERNALS__' in window ? <button className="secondary" disabled={disabled || dirty || !!busy || needsRefresh} onClick={() => void choose()}>Choose catalog CSV</button> : <label className="csv-file-label">Catalog CSV file<input ref={input} type="file" accept=".csv,text/csv" aria-label="Catalog CSV file" disabled={disabled || dirty || (!!busy && !['read', 'preview'].includes(busy)) || needsRefresh} onChange={event => { const chosen = event.target.files?.[0]; if (chosen) void choose(chosen); else cancel(); }}/></label>}
    {file && <p>Selected file: <strong>{file.filename}</strong></p>}
    <div className="configuration-actions"><button className="secondary" disabled={!file || !!busy || disabled || dirty || needsRefresh} onClick={() => void review()}>{busy === 'preview' ? 'Previewing CSV…' : 'Preview CSV'}</button><button className="secondary" disabled={(!!busy && !['read', 'preview'].includes(busy)) || (!file && !busy && !preview)} onClick={cancel}>Cancel import</button></div>
    {preview && <div className="csv-review"><h3>Review import</h3><p role="status">{preview.created} created · {preview.updated} updated · {preview.unchanged} unchanged · {preview.issues.length} errors</p><p>No changes are saved until you choose Apply catalog changes.</p>{preview.issues.length > 0 && <ul role="alert" className="csv-issues">{preview.issues.map((issue, i) => <li key={i}>{issue.row ? `Row ${issue.row}` : 'Header / file'} · line {issue.line} · {issue.field}: {issue.message}</li>)}</ul>}<ul className="csv-rows" aria-label="Catalog import rows">{preview.rows.map((row, i) => <li key={i}><details><summary><strong>{row.sku}</strong><span>Row {row.row} · line {row.line} · {row.action}</span></summary><div className="csv-comparison">{row.before && row.action === 'update' && <ProductReview title="Saved product (before)" item={row.before}/>}<ProductReview title={row.action === 'unchanged' ? 'Unchanged product' : 'Imported product (after)'} item={row.after}/></div></details></li>)}</ul>{preview.expected_config_revision !== revision && <p className="warning-banner">Saved configuration changed. Reload saved configuration and preview again.</p>}<button className="primary" disabled={!!busy || !canApplyCatalogCsv(preview, revision, dirty || disabled || needsRefresh)} onClick={() => void apply()}>Apply catalog changes</button></div>}
    {busy === 'read' && <p role="status">Reading selected CSV…</p>}{busy === 'apply' && <p role="status">Applying reviewed catalog changes…</p>}
    {needsRefresh && <button className="secondary" disabled={!canRefreshCatalogCsv(dirty, disabled, !!busy)} onClick={() => void refresh()}>Refresh saved catalog</button>}
    {error && <p role="alert" className="error-banner">{error}</p>}{notice && <p role="status" className="success-banner">{notice}</p>}
  </section>;
}
