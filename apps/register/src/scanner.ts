import type { CatalogItem } from './types.ts';
export type BarcodeMatch = { item: CatalogItem; config_revision: string };
export function barcodeError(value: string): string {
  return !value || value !== value.trim() || /\p{Cc}/u.test(value) || new TextEncoder().encode(value).length > 128
    ? 'Enter a barcode of 1–128 UTF-8 bytes, without surrounding whitespace or control characters.' : '';
}
/** Each deliberate submission is one scan. Cancel invalidates queued and in-flight work. */
export function createScanQueue(lookup: (barcode: string) => Promise<BarcodeMatch>) {
  let generation = 0;
  let tail: Promise<void> = Promise.resolve();
  return {
    cancel() { generation += 1; tail = Promise.resolve(); },
    submit(barcode: string, revision: string, handlers: { current: () => boolean; matched: (item: CatalogItem) => boolean; failed: (error: Error) => void; finished: () => void }): Promise<void> {
      const ticket = generation;
      const task = tail.then(async () => {
        if (ticket !== generation || !handlers.current()) return;
        try {
          const invalid = barcodeError(barcode);
          if (invalid) throw new Error(invalid);
          const match = await lookup(barcode);
          if (ticket !== generation || !handlers.current()) return;
          if (match.config_revision !== revision) throw new Error('Catalog changed. Reload the register before scanning again.');
          if (match.item.archived) throw new Error('This product is archived.');
          if (match.item.barcode !== barcode) throw new Error('Barcode lookup returned a different product. Try again.');
          if (!handlers.matched(match.item)) generation += 1;
        } catch (error) {
          if (ticket === generation && handlers.current()) handlers.failed(error instanceof Error ? error : new Error('Barcode lookup failed. Try again.'));
        } finally { if (ticket === generation && handlers.current()) handlers.finished(); }
      });
      tail = task.catch(() => {});
      return task;
    },
  };
}
