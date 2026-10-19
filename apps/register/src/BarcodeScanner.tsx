import { useEffect, useRef, useState, type RefObject } from 'react';
import { command } from './api';
import { barcodeError, createScanQueue, type BarcodeMatch } from './scanner';
import type { CatalogItem } from './types';

export function BarcodeScanner({ enabled, revision, onProduct, resetToken }: { resetToken: RefObject<number>; enabled: boolean; revision: string; onProduct: (item: CatalogItem) => boolean }) {
  const [value, setValue] = useState('');
  const [pending, setPending] = useState(0);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const input = useRef<HTMLInputElement>(null);
  const queue = useRef(createScanQueue(barcode => command<BarcodeMatch>('lookup_barcode', { barcode })));
  const token = resetToken.current;
  const latest = useRef({ enabled, revision, onProduct });
  latest.current = { enabled, revision, onProduct };
  const cancel = () => { queue.current.cancel(); setPending(0); setValue(''); setError(''); setNotice('Scan cancelled.'); };
  useEffect(() => {
    queue.current.cancel(); setPending(0); setValue(''); setError(''); setNotice('');
    return () => queue.current.cancel();
  }, [enabled, revision, token]);
  const scan = () => {
    if (!enabled) return;
    const barcode = input.current?.value ?? value;
    const invalid = barcodeError(barcode);
    if (invalid) { setError(invalid); return; }
    // Clear immediately: a keyboard wedge can start its next scan before React renders.
    if (input.current) input.current.value = '';
    setValue(''); setError(''); setNotice(''); setPending(count => count + 1);
    void queue.current.submit(barcode, revision, {
      current: () => latest.current.enabled && latest.current.revision === revision && resetToken.current === token,
      matched: item => {
        if (!latest.current.enabled || latest.current.revision !== revision) return false;
        setError('');
        const keepScanning = latest.current.onProduct(item);
        setNotice(keepScanning ? `Added ${item.name}.` : `Enter measured weight for ${item.name}.`);
        if (!keepScanning) setPending(0);
        return keepScanning;
      },
      failed: issue => { setError(issue.message); setNotice(''); },
      finished: () => setPending(count => Math.max(0, count - 1)),
    });
  };
  return <section className="barcode-scanner" aria-label="Barcode scanner"><form onSubmit={event => { event.preventDefault(); scan(); }}><label htmlFor="barcode-scan">Barcode</label><div className="barcode-controls"><input ref={input} id="barcode-scan" aria-label="Barcode" type="text" autoComplete="off" spellCheck={false} placeholder="Focus here, scan, then Enter" disabled={!enabled} value={value} onChange={event => setValue(event.target.value)} onKeyDown={event => { if (event.key === 'Enter' && (event.repeat || event.nativeEvent.isComposing)) event.preventDefault(); }}/><button type="submit" className="secondary" disabled={!enabled}>Look up barcode</button><button type="button" className="text-button" disabled={!enabled || (!pending && !value)} onClick={cancel}>Cancel scan</button></div></form><p className="input-help">Keyboard-wedge scanners only. Click this field before scanning. Each Enter adds one unit or package, or asks for measured weight. No camera or global keyboard capture.</p>{pending > 0 && <p role="status">Looking up barcode…</p>}{notice && <p role="status">{notice}</p>}{error && <p className="error-banner" role="alert">{error}</p>}</section>;
}
