import { useEffect, useRef, useState } from 'react';
import { command } from './api';
import { Icon } from './Icon';
import { dateTime, money } from './format';
import type { CartLine, RegisterState } from './types';
import type { DraftIssue, DraftPreview, DraftView, DraftWorkspace, ParkedDraft } from './drafts';

const errorText = (error: unknown) => error instanceof Error ? error.message : 'The saved cart could not be loaded.';
export function savedQuantity(line: CartLine) {
  return line.quantity.kind === 'measured' ? `${line.quantity.grams} g measured` : `${line.quantity.count} ${line.quantity.kind === 'unit' ? (line.quantity.count === '1' ? 'unit' : 'units') : (line.quantity.count === '1' ? 'package' : 'packages')}`;
}
export function DraftWarnings({ issues }: { issues: DraftIssue[] }) {
  if (!issues.length) return null;
  return <div className="draft-warnings" role="alert"><strong>Review this cart</strong><ul>{issues.map((issue, index) => <li key={`${issue.line_id}-${issue.code}-${index}`}>{issue.message}{issue.blocking && <span> Remove or resolve this line before checkout.</span>}</li>)}</ul><p>Saved quantities keep their original meaning. Prices are recalculated using the current catalog.</p></div>;
}
export function ParkCartForm({ busy, error, onSubmit }: { busy: boolean; error: string; onSubmit: (label: string) => Promise<void> }) {
  const [label, setLabel] = useState('');
  const submitting = useRef(false);
  const valid = Array.from(label.trim()).length <= 80 && !/\p{Cc}/u.test(label);
  return <form className="park-cart-form" onSubmit={event => { event.preventDefault(); if (busy || submitting.current || !valid) return; submitting.current = true; void onSubmit(label).finally(() => { submitting.current = false; }); }}><div className="park-explainer"><Icon name="bag" size={26}/><p>Your exact items and quantities stay together.<br/>No sale, payment, stock hold or reservation is created.</p></div><label className="field-label" htmlFor="cart-label">Cart label (optional)</label><input id="cart-label" autoFocus autoComplete="off" maxLength={160} placeholder="e.g. Counter 2" value={label} disabled={busy} onChange={event => setLabel(event.target.value)}/><p className="input-help">Use a short, non-personal label. Do not enter names, phone numbers, customer details or other personal information. Up to 80 characters.</p>{!valid && <p className="field-error">Use at most 80 characters and no control characters.</p>}{error && <div className="error-banner" role="alert">{error}</div>}<button className="primary full-width" type="submit" disabled={busy || !valid}><Icon name={busy ? 'lock' : 'check'}/>{busy ? 'Saving parked cart…' : 'Park this cart'}</button></form>;
}
export function ParkedCarts({ workspace, activeLines, onBusy, onDone }: { workspace: DraftWorkspace; activeLines: CartLine[]; onBusy: (busy: boolean) => void; onDone: (message: string) => void }) {
  const [drafts, setDrafts] = useState<ParkedDraft[]>(workspace.view.state?.parked_drafts ?? []);
  const [preview, setPreview] = useState<DraftPreview | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [reviewed, setReviewed] = useState(false);
  const request = useRef(0);
  const actionLock = useRef(false);
  useEffect(() => {
    let live = true;
    command<RegisterState>('get_state').then(state => { if (live) setDrafts(state.parked_drafts); }).catch(error => { if (live) setError(errorText(error)); }).finally(() => { if (live) setLoading(false); });
    return () => { live = false; request.current++; };
  }, []);
  const open = async (id: string) => {
    if (actionLock.current) return;
    const current = ++request.current;
    setLoading(true); setError(''); setPreview(null); setReviewed(false);
    try { const next = await command<DraftPreview>('preview_parked_draft', { draft_id: id }); if (current === request.current) setPreview(next); }
    catch (error) { if (current === request.current) setError(errorText(error)); }
    finally { if (current === request.current) setLoading(false); }
  };
  const act = async (kind: 'resume' | 'discard') => {
    if (actionLock.current || !preview || (kind === 'resume' && (activeLines.length > 0 || preview.issues.length > 0 && !reviewed))) return;
    if (kind === 'discard' && !window.confirm(`Discard parked cart “${preview.label || 'Unlabeled cart'}”? It will no longer be available to resume. Other carts and saved sales will not change.`)) return;
    actionLock.current = true; setBusy(true); onBusy(true); setError('');
    try {
      await workspace.mutate(kind === 'resume' ? { command: 'resume_parked_draft', draft_id: preview.draft_id, expected_config_revision: preview.config_revision } : { command: 'discard_parked_draft', draft_id: preview.draft_id });
      if (kind === 'resume') onDone('Parked cart resumed. Review current prices before checkout.');
      else { setDrafts(workspace.view.state?.parked_drafts ?? []); setPreview(null); }
    } catch (error) { setError(errorText(error)); }
    finally { actionLock.current = false; setBusy(false); onBusy(false); }
  };
  return <div className="parked-workspace"><div className="parked-list" aria-label="Saved parked carts">{!drafts.length && !loading ? <div className="parked-empty"><div className="empty-symbol"><Icon name="bag" size={28}/></div><h3>A place to pause.</h3><p>Park an active cart and it will be waiting here.</p></div> : drafts.map(draft => <button key={draft.id} type="button" data-testid={`parked-draft-${draft.id}`} aria-label={`Preview ${draft.label || 'Unlabeled cart'}`} className={`parked-card ${preview?.draft_id === draft.id ? 'selected' : ''}`} disabled={busy} onClick={() => void open(draft.id)}><span className="parked-card-icon"><Icon name="bag" size={21}/></span><span><strong>{draft.label || 'Unlabeled cart'}</strong><small>{draft.line_count} {draft.line_count === 1 ? 'line' : 'lines'} · {dateTime(draft.parked_at_unix)}</small></span><Icon name="chevron" size={16}/></button>)}</div><section className="parked-preview" data-testid="parked-preview" aria-label="Parked cart preview">{loading ? <p role="status">Loading saved cart…</p> : preview ? <><span className="eyebrow">SAVED QUANTITIES · CURRENT PRICES</span><h3>{preview.label || 'Unlabeled cart'}</h3><p className="input-help">Nothing is charged or reserved. Resume keeps every saved line, even one that needs attention.</p><div className="parked-preview-lines">{preview.lines.map(line => { const priced = preview.quote?.lines.find(item => item.id === line.id); return <div key={line.id}><span><strong>{priced?.name || line.sku}</strong><small>{savedQuantity(line)} · {line.sku}</small></span><strong>{priced ? money(priced.amount_minor, preview.currency) : '—'}</strong></div>; })}</div><DraftWarnings issues={preview.issues}/><div className="parked-total"><span>Current total</span><strong>{preview.quote ? money(preview.quote.total_minor, preview.currency) : 'Needs review'}</strong></div>{preview.issues.length > 0 && <label className="draft-review-check"><input type="checkbox" checked={reviewed} disabled={busy} onChange={event => setReviewed(event.target.checked)}/><span>I reviewed the changes. Keep the original quantities when resuming.</span></label>}{activeLines.length > 0 && <p className="warning-banner">Your active cart has items. Park or clear it before resuming another cart. No carts will be merged or overwritten.</p>}<div className="parked-preview-actions"><button className="primary" disabled={busy || activeLines.length > 0 || preview.issues.length > 0 && !reviewed} onClick={() => void act('resume')}><Icon name={busy ? 'lock' : 'arrow'} size={16}/>Resume cart</button><button className="text-button danger-text" disabled={busy} onClick={() => void act('discard')}>Discard parked cart</button></div></> : drafts.length > 0 ? <div className="parked-preview-placeholder"><Icon name="receipt" size={32}/><h3>Pick a cart to review.</h3><p>See its saved quantities, current prices and any catalog changes before resuming.</p></div> : null}{error && <div className="error-banner" role="alert">{error}</div>}</section></div>;
}
export function DraftRecovery({ workspace, view, onDone }: { workspace: DraftWorkspace; view: DraftView; onDone: () => void }) {
  const [error, setError] = useState('');
  const operation = view.pending?.command === 'save_active_draft' ? 'Save active cart' : view.pending?.command === 'park_draft' ? 'Park cart' : view.pending?.command === 'resume_parked_draft' ? 'Resume parked cart' : 'Discard parked cart';
  const retry = async () => { setError(''); try { await workspace.retry(); if (!workspace.blocked) onDone(); } catch (error) { setError(errorText(error)); } };
  return <div className="draft-recovery"><div className="warning-banner" role={view.busy ? "status" : "alert"}>{view.busy ? 'Checking the saved cart action. Keep this window open until its outcome is confirmed.' : view.error}</div>{view.pending && <><p>Pending action: <strong>{operation}</strong>. Retry sends the exact same request and recovery key, never a second action.</p><details open><summary>Lines kept in this browser ({view.pending.source_lines.length})</summary><ul>{view.pending.source_lines.map(line => <li key={line.id}><strong>{line.sku}</strong> · {savedQuantity(line)}</li>)}</ul></details>{view.conflict && <div className="draft-conflict"><h3>Current saved active cart</h3><ul>{view.conflict.draft.lines.length ? view.conflict.draft.lines.map(line => <li key={line.id}><strong>{line.sku}</strong> · {savedQuantity(line)}</li>) : <li>The saved active cart is empty.</li>}</ul><p>{view.conflict.parked_drafts.length} parked carts remain saved. The current workspace will not be overwritten.</p><button className="secondary full-width" disabled={view.busy} onClick={() => { if (!window.confirm('Use the current saved cart shown above? This replaces the local unsaved view and abandons this failed or already-resolved action. Compare the lines before continuing.')) return; void workspace.acceptCurrent().then(() => { if (!workspace.blocked) onDone(); }).catch(error => setError(errorText(error))); }}>Use current saved cart</button></div>}<button className="primary full-width" disabled={view.busy} onClick={() => void retry()}><Icon name={view.busy ? 'lock' : 'refresh'} size={17}/>{view.busy ? 'Checking saved action…' : 'Retry saved action'}</button></>}{error && error !== view.error && <p className="error-banner" role="alert">{error}</p>}<p className="input-help">Keep this browser’s data intact until recovery finishes. Other windows may have newer saved work.</p></div>;
}
