import { useEffect, useRef, useState } from 'react';
import { command } from './api';
import { DEFAULT_STORE_SETTINGS, STORE_LIMITS, storeSettingsError } from './storeSettings';
import type { RegisterState, StoreSettings } from './types';

export function StoreSettingsEditor({ settings, revision, disabled, onSave, onDirtyChange, onBusyChange }: { settings?: StoreSettings; revision: string; disabled: boolean; onSave: (state: RegisterState) => void; onDirtyChange: (dirty: boolean) => void; onBusyChange: (busy: boolean) => void }) {
  const [draft, setDraft] = useState(() => ({ ...(settings ?? DEFAULT_STORE_SETTINGS) }));
  const [preview, setPreview] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [failed, setFailed] = useState(false);
  const lock = useRef(false);
  const saved = settings ?? DEFAULT_STORE_SETTINGS;
  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);
  const error = storeSettingsError(draft);
  useEffect(() => { setDraft({ ...(settings ?? DEFAULT_STORE_SETTINGS) }); setPreview(false); }, [settings, revision]);
  useEffect(() => { onDirtyChange(dirty); }, [dirty, onDirtyChange]);
  const save = async () => {
    if (lock.current || disabled || !dirty || error || !preview) return;
    lock.current = true; setBusy(true); onBusyChange(true); setMessage(''); setFailed(false);
    try {
      const state = await command<RegisterState>('put_store_settings', { store_settings: draft, expected_config_revision: revision });
      onSave(state); setDraft({ ...(state.config?.store_settings ?? DEFAULT_STORE_SETTINGS) }); setPreview(false); setMessage('Store settings saved. Existing receipts are unchanged.');
    } catch (cause) { setFailed(true); setMessage(cause instanceof Error ? cause.message : 'Could not save store settings. Reload saved configuration before retrying a stale revision.'); }
    finally { lock.current = false; setBusy(false); onBusyChange(false); }
  };
  return <section className="settings-card" aria-label="Store settings"><h2>Store settings</h2><p>Receipt identity and default reporting timezone only. Currency and pricing are not changed.</p>
    <fieldset disabled={disabled || busy} className="settings-fields"><div className="editor-grid">{(['display_name', 'receipt_header', 'receipt_contact', 'receipt_footer', 'timezone'] as const).map(field => <label key={field}>{({ display_name: 'Store display name', receipt_header: 'Receipt header', receipt_contact: 'Receipt contact', receipt_footer: 'Receipt footer', timezone: 'Store timezone' })[field]}<input aria-label={({ display_name: 'Store display name', receipt_header: 'Receipt header', receipt_contact: 'Receipt contact', receipt_footer: 'Receipt footer', timezone: 'Store timezone' })[field]} value={draft[field]} onChange={event => { setDraft({ ...draft, [field]: event.target.value }); setPreview(false); setMessage(''); }}/><small>{STORE_LIMITS[field]} characters maximum · single-line plain text</small></label>)}</div>
    <p>Timezone examples: UTC, America/New_York, Europe/London. Reports can use another timezone without changing this default.</p>
    {error && <p role="alert" className="field-error">{error}</p>}
    <div className="configuration-actions"><button className="secondary" disabled={!!error} onClick={() => setPreview(true)}>Preview receipt</button><button className="primary" disabled={!dirty || !!error || !preview} onClick={() => void save()}>{busy ? 'Saving store settings…' : 'Save store settings'}</button><button className="secondary" disabled={!dirty && !preview} onClick={() => { setDraft({ ...saved }); setPreview(false); setMessage(''); }}>Cancel changes</button></div></fieldset>
    {preview && <div className="store-preview" aria-label="Receipt identity preview"><strong>{draft.display_name}</strong><p>{draft.receipt_header}</p><p>{draft.receipt_contact}</p><p>New receipts save this identity. Reports default to {draft.timezone}. Saved receipts keep their original settings.</p><p>{draft.receipt_footer}</p><strong>DEMO / TRAINING RECEIPT</strong><p>Training only. Tax is not configured. Not a tax or compliance receipt.</p></div>}
    {message && <p role={failed ? 'alert' : 'status'}>{message}</p>}
  </section>;
}
